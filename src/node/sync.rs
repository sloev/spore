//! Node — gossip: INV/WANT, and the store queries the sync loop reads.
//!
//! Split out of the 3977-line `lib.rs` (task #23): `impl Node` in a descendant
//! module of the crate root, so it keeps full access to `Node`'s private fields
//! with no visibility change. A pure move — wire format and public API identical.

use crate::*;

impl Node {
    /// INV = concatenated 16-byte IDs of stored envelopes relevant to a peer
    /// that follows `peer_topics` (public + those topics + unicast for custody).
    pub fn build_inv(&self, peer_topics: &HashSet<Addr>) -> Vec<u8> {
        let mut ids: Vec<(&Id, &store::Stored)> = self.store.entries().collect();
        ids.sort_by_key(|(_, s)| std::cmp::Reverse(s.expiry)); // newest first
        let mut p = Vec::new();
        for (id, s) in ids {
            let relevant =
                s.dest == ZERO_DEST || peer_topics.contains(&s.dest) || !self.topics.contains(&s.dest); // unicast -> carry (custody)
            if relevant {
                p.extend_from_slice(id);
            }
        }
        Envelope::new(ty::INV, ZERO_DEST, 0, p).wire()
    }

    pub(crate) fn on_inv(&self, e: &Envelope, iface: Iface, nbr: Option<Addr>) -> Rx {
        let mut want = Vec::new();
        // Bounded for the same reason `on_want` is, and additionally because the
        // reply is *our* traffic: an INV listing thousands of ids we lack would
        // have us emit a WANT just as large.
        for chunk in e.payload.chunks(16).take(MAX_IDS_PER_GOSSIP) {
            if chunk.len() == 16 {
                let mut id = [0u8; 16];
                id.copy_from_slice(chunk);
                if !self.store.contains(&id) {
                    want.extend_from_slice(&id); // request what we lack
                }
            }
        }
        let mut rx = Rx::default();
        if !want.is_empty() {
            rx.forwards.push(Forward::Directed {
                iface,
                nbr,
                bytes: Envelope::new(ty::WANT, ZERO_DEST, 0, want).wire(),
            });
        }
        rx
    }

    pub(crate) fn on_want(&mut self, e: &Envelope, iface: Iface, nbr: Option<Addr>, now: u32) -> Rx {
        let mut rx = Rx::default();
        // Sweep here too. `on_rx` returns early for INV and WANT, so this path
        // never reached `enforce_bounds` at all — and `interests` is the one
        // table a WANT *grows*, which made it the worst possible path to skip:
        // a node whose neighbours only ever ask it for things would fill the
        // table and never expire an entry. Same shape as the M11-A finding, one
        // layer along.
        self.enforce_bounds(now);
        let rate = self.gossip_rate;
        // A WANT may carry one trailing byte of remaining depth (M11-I). Ids are
        // 16 bytes, so an odd byte on the end is unambiguous; without one the
        // asker is a plain requester and gets the default budget.
        let (ids, depth) = match e.payload.len() % 16 {
            1 => (&e.payload[..e.payload.len() - 1], e.payload[e.payload.len() - 1]),
            _ => (&e.payload[..], DEFAULT_WANT_DEPTH),
        };

        let mut adopt: Vec<Id> = Vec::new();
        for chunk in ids.chunks(16).take(MAX_IDS_PER_GOSSIP) {
            if chunk.len() != 16 {
                continue;
            }
            let mut id = [0u8; 16];
            id.copy_from_slice(chunk);
            // `store.wire` may read from the spill directory, so an unbounded WANT
            // buys disk reads as well as bandwidth.
            let Some(wire) = self.store.wire(&id) else {
                // Not held. This is where a fetch more than one hop from a
                // holder used to end.
                if self.may_adopt(&id, depth) {
                    adopt.push(id);
                }
                continue;
            };
            let bucket = self.gossip.entry(iface).or_insert_with(|| congestion::TokenBucket::new(rate));
            if !bucket.allow(wire.len() as u32, now) {
                break; // this link has spent its share; it can ask again later
            }
            rx.forwards.push(Forward::Directed { iface, nbr, bytes: wire });
        }

        if !adopt.is_empty() {
            rx.forwards.append(&mut self.adopt_interest(&adopt, iface, nbr, depth, now));
        }
        rx
    }

    /// May this node go looking for `id` on someone else's behalf?
    ///
    /// Only if a manifest it already holds names the id. The flooded root is the
    /// capability: the mesh has agreed the file exists and the Merkle tree names
    /// every legal child, so an id nobody has a manifest for starts no hunt.
    /// Without that rule, "WANT this random id" is a request to search the mesh,
    /// which is S-012 at depth n.
    ///
    /// Sealed files never recurse. Their chunks are ciphertext, so
    /// confidentiality holds either way, but copying one across the mesh and
    /// advertising that it exists is not what sealing to one recipient meant. A
    /// chunk does not carry its own sealed-ness — only the root does — which is
    /// exactly why this gate is the right place to ask: a node entitled to
    /// recurse is by construction holding the manifest that knows.
    fn may_adopt(&self, id: &Id, depth: u8) -> bool {
        if depth == 0 {
            return false; // the budget for asking onward is spent
        }
        let Some(root) = self.named_by(id) else { return false };
        match self.manifests.get(&root) {
            Some(m) => !m.sealed(),
            // The root is named but not held as a manifest — we know of the id
            // only through a tree we are still resolving, which is legitimate.
            None => true,
        }
    }

    /// Adopt a neighbour's interest and ask onward for it.
    ///
    /// **The WANT envelope is not forwarded.** This node acquires an interest of
    /// its own and emits its own request, which is what keeps §6's bound intact:
    /// WANT stays hops=0, unsigned, consumed, never relayed.
    ///
    /// Re-asking is deduplicated by the interest table, which is what stops
    /// depth × degree from being exponential: a node that already has a live
    /// interest in an id records the new waiter and stays quiet, so the total
    /// traffic is bounded by the number of nodes reached rather than by the
    /// number of paths to them.
    fn adopt_interest(
        &mut self,
        ids: &[Id],
        iface: Iface,
        nbr: Option<Addr>,
        depth: u8,
        now: u32,
    ) -> Vec<Forward> {
        let mut fresh: Vec<Id> = Vec::new();
        for id in ids {
            match self.interests.get_mut(id) {
                Some(entry) if entry.until > now => {
                    // Someone is already looking. Just get in the queue.
                    if !entry.waiters.contains(&(iface, nbr)) {
                        entry.waiters.push((iface, nbr));
                    }
                    continue;
                }
                _ => {}
            }
            if self.interests.len() >= self.limits.interests && !self.interests.contains_key(id) {
                break; // a bounded promise-to-return-something, and it is full
            }
            self.interests
                .insert(*id, Interest { waiters: vec![(iface, nbr)], until: now + INTEREST_LEASE_SECS });
            fresh.push(*id);
        }
        if fresh.is_empty() {
            return Vec::new();
        }
        // One WANT, to every interface except the one that asked. On a node with
        // a handful of links that *is* the fanout; the interest table above is
        // what keeps it from compounding hop over hop.
        let mut payload: Vec<u8> = fresh.iter().flatten().copied().collect();
        payload.push(depth.saturating_sub(1));
        vec![Forward::Flood { except: iface, bytes: Envelope::new(ty::WANT, ZERO_DEST, 0, payload).wire() }]
    }

    /// Hand a just-arrived envelope to whoever adopted an interest in it.
    ///
    /// The return leg. Directed at each waiter rather than broadcast, because
    /// they asked and nobody else did — which is the same reason chunks are
    /// link-local in the first place.
    pub(crate) fn serve_interest(&mut self, id: &Id, now: u32) -> Vec<Forward> {
        let Some(entry) = self.interests.remove(id) else { return Vec::new() };
        if entry.until <= now {
            return Vec::new(); // the lease ran out; nobody is waiting any more
        }
        let Some(wire) = self.store.wire(id) else { return Vec::new() };
        entry
            .waiters
            .into_iter()
            .map(|(iface, nbr)| Forward::Directed { iface, nbr, bytes: wire.clone() })
            .collect()
    }

    pub fn store_len(&self) -> usize {
        self.store.len()
    }
    pub fn has(&self, id: &Id) -> bool {
        self.store.contains(id)
    }

    /// All stored envelope IDs, concatenated (16 B each) — a bag INV.
    pub fn stored_ids(&self) -> Vec<u8> {
        let mut v = Vec::with_capacity(self.store.len() * 16);
        for id in self.store.ids() {
            v.extend_from_slice(id);
        }
        v
    }
    /// The wire bytes of a stored envelope, if held.
    pub fn get_wire(&self, id: &Id) -> Option<Vec<u8>> {
        self.store.wire(id)
    }
    /// Every stored envelope as `(id, wire)` — the whole bag.
    pub fn store_wires(&self) -> Vec<(Id, Vec<u8>)> {
        self.store
            .ids()
            .copied()
            .collect::<Vec<_>>()
            .iter()
            .filter_map(|id| self.store.wire(id).map(|w| (*id, w)))
            .collect()
    }

    // ---- datagram sessions (§ application layer, tag 0x04) ---------------
}
