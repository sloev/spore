//! Node — gossip: INV/WANT, and the store queries the sync loop reads.
//!
//! Split out of the 3977-line `lib.rs` (task #23): `impl Node` in a descendant
//! module of the crate root, so it keeps full access to `Node`'s private fields
//! with no visibility change. A pure move — wire format and public API identical.

use crate::*;

/// A WANT carrying `fl::CANCEL` — "I no longer want these ids".
///
/// No depth byte: depth budgets how far a request may *travel*, and a cancel
/// travels exactly as far as the interests it retires, which the receivers
/// already know. An even payload length also means an older build parses it as
/// a plain WANT rather than mistaking a trailing depth byte for half an id.
fn cancel_frame(ids: &[Id]) -> Vec<u8> {
    let payload: Vec<u8> = ids.iter().flatten().copied().collect();
    let mut e = Envelope::new(ty::WANT, ZERO_DEST, 0, payload);
    e.flags |= fl::CANCEL;
    e.wire()
}

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
        //
        // **Clamped, because the asker writes it** (M11-L). This byte arrives on
        // an unsigned frame from anyone in radio range, and it is the only one of
        // recursive pull's three bounds that was taken on trust: the manifest
        // gate is checked locally and the interest table dedupes locally, but
        // reach was whatever the stranger claimed. `spore-sim`'s
        // `malicious-want` measured the difference on a 24-node line — an honest
        // depth of 8 leaves 8 nodes holding an interest, a forged 255 leaves 23.
        // One frame, and since M11-P each of those interests is a multi-day
        // obligation re-stated on a cadence.
        //
        // `min` rather than a rejection: a legitimate relayed WANT carries a
        // *decremented* depth, so asking for less is normal and must keep
        // working. Only claiming more is refused, and it degrades to local
        // policy rather than dropping the request.
        let (ids, depth) = match e.payload.len() % 16 {
            1 => (&e.payload[..e.payload.len() - 1], e.payload[e.payload.len() - 1].min(DEFAULT_WANT_DEPTH)),
            _ => (&e.payload[..], DEFAULT_WANT_DEPTH),
        };

        if e.flags & fl::CANCEL != 0 {
            return self.on_cancel(ids, iface, nbr);
        }

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

    /// A neighbour has stopped wanting these ids (M11-K).
    ///
    /// Without this, the only way an adopted interest ends is `INTEREST_LEASE_SECS`
    /// — fifteen minutes of a chain of relays still hunting for a file whose only
    /// asker walked away. The lease is the backstop for a fetcher that vanishes
    /// without saying so; this is the fast path for one that can.
    ///
    /// **A cancel only ever removes the sender's own waiter.** That is what keeps
    /// it from being a denial-of-service primitive: a hostile neighbour cancelling
    /// ids it never asked for finds nothing of its own to remove and changes
    /// nothing for anyone else. The upstream cancel is emitted only when the last
    /// waiter leaves, so n ids in produce at most n ids out, once — a second
    /// cancel for the same id finds no interest and dies here. Unlike a WANT,
    /// this path can only shrink the table, so it needs no admission check.
    fn on_cancel(&mut self, ids: &[u8], iface: Iface, nbr: Option<Addr>) -> Rx {
        let mut rx = Rx::default();
        let mut orphaned: Vec<Id> = Vec::new();
        for chunk in ids.chunks(16).take(MAX_IDS_PER_GOSSIP) {
            if chunk.len() != 16 {
                continue;
            }
            let mut id = [0u8; 16];
            id.copy_from_slice(chunk);
            let Some(entry) = self.interests.get_mut(&id) else { continue };
            entry.waiters.retain(|w| *w != (iface, nbr));
            if entry.waiters.is_empty() {
                self.interests.remove(&id);
                orphaned.push(id);
            }
        }
        if !orphaned.is_empty() {
            rx.forwards.push(Forward::Flood { except: iface, bytes: cancel_frame(&orphaned) });
        }
        rx
    }

    /// Stop fetching `magnet`: drop what we were still asking for and tell the
    /// neighbours we asked, so the cancel unwinds the chain they adopted.
    ///
    /// Returns the frames to send. Emitting them is the caller's choice — a node
    /// that simply stops calling `fetch` is still correct, just fifteen minutes
    /// slower to stop costing the mesh anything.
    pub fn abandon(&mut self, magnet: &Id) -> Vec<Forward> {
        let window = self.want_window();
        let ids = self.missing(magnet, usize::MAX);
        self.interests.retain(|id, _| !ids.contains(id));
        ids.chunks(window).map(|w| Forward::Flood { except: NO_IFACE, bytes: cancel_frame(w) }).collect()
    }

    /// A link went away, so every interest it was the sole waiter for is dead.
    ///
    /// The other half of M11-K, and the half that covers a fetcher which cannot
    /// say goodbye — a phone out of radio range, a peer that crashed. Stateful
    /// bridges know when a link drops; this converts that knowledge into the same
    /// unwind an explicit cancel performs.
    pub fn forget_interests_on(&mut self, iface: Iface) -> Vec<Forward> {
        let mut orphaned: Vec<Id> = Vec::new();
        self.interests.retain(|id, entry| {
            entry.waiters.retain(|(i, _)| *i != iface);
            if entry.waiters.is_empty() {
                orphaned.push(*id);
                return false;
            }
            true
        });
        if orphaned.is_empty() {
            return Vec::new();
        }
        let window = self.want_window();
        orphaned.chunks(window).map(|w| Forward::Flood { except: iface, bytes: cancel_frame(w) }).collect()
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
            let until = self.interest_deadline(id, now);
            self.interests.insert(*id, Interest { waiters: vec![(iface, nbr)], until });
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

    /// How long this node is willing to keep looking for `id` (M11-P).
    ///
    /// **Bounded by the object, not by a fixed timer.** An adopted interest is a
    /// promise to keep hunting, and the honest duration of that promise is "as
    /// long as the thing could still turn up". Chunks are ordinary envelopes and
    /// die with the publisher's expiry, so an interest that outlives the manifest
    /// naming it is hunting for bytes nobody will serve — pure cost, and exactly
    /// the standing pull `fetch-abandoned` measures.
    ///
    /// The old fixed `INTEREST_LEASE_SECS` was the opposite failure: fifteen
    /// minutes is far *shorter* than the object lives, which made adopted
    /// interest an online-only mechanism inside a delay-tolerant protocol. A
    /// courier who takes a day to reach the next mesh had forgotten what it was
    /// carrying long before it arrived.
    ///
    /// Still bounded three ways, so the longer lease does not weaken M11-A: the
    /// deadline comes from a *signed* manifest whose expiry the asker does not
    /// control and the store already clamps to its horizon; `limits.interests`
    /// caps how many can exist at once; and M11-K's cancel retires one the moment
    /// its last waiter goes. Without cancel, lengthening this would have
    /// multiplied the very load M11-K removed.
    fn interest_deadline(&self, id: &Id, now: u32) -> u32 {
        let floor = now.saturating_add(INTEREST_LEASE_SECS);
        let ceiling = now.saturating_add(MAX_INTEREST_LEASE_SECS);
        // The manifest that makes this id legal to want is also what says how
        // long wanting it makes sense. No manifest expiry to read — an id named
        // by a tree we are still resolving — falls back to the old short lease.
        let by_object =
            self.named_by(id).and_then(|root| self.store.meta(&root).map(|s| s.expiry)).unwrap_or(floor);
        by_object.clamp(floor, ceiling)
    }

    /// Ask again for everything still outstanding (M11-P).
    ///
    /// The courier's arrival. An adopted interest that survived a journey is
    /// worth nothing until it is *spoken* somewhere new, and there is no other
    /// path that would: WANT is emitted when a neighbour asks, and by definition
    /// the neighbour who asked is not here any more. A sync loop calls this when
    /// it meets someone it has not asked yet.
    ///
    /// Bounded by the same window as any other request, so meeting a stranger
    /// costs one frame's worth of ids rather than the whole table.
    pub fn resume_interests(&mut self, now: u32) -> Vec<Forward> {
        self.interests.retain(|_, i| i.until > now);
        let ids: Vec<Id> = self.interests.keys().copied().collect();
        if ids.is_empty() {
            return Vec::new();
        }
        let window = self.want_window();
        ids.chunks(window)
            .map(|w| {
                let mut payload: Vec<u8> = w.iter().flatten().copied().collect();
                payload.push(DEFAULT_WANT_DEPTH.saturating_sub(1));
                Forward::Flood {
                    except: NO_IFACE,
                    bytes: Envelope::new(ty::WANT, ZERO_DEST, 0, payload).wire(),
                }
            })
            .collect()
    }

    /// The outstanding interests, as a blob to persist (M11-P).
    ///
    /// `[1][n:2]` then `n × ([id:16][until:4])`. The counterpart to
    /// `Node::prekey_ring`, and saved the same way: this is what makes a pending
    /// interest survive a restart — or a flight, which is the case that matters.
    ///
    /// **Waiters are deliberately not carried.** A waiter is an `(iface, nbr)`
    /// pair, and an interface index means nothing after a restart, still less on
    /// another continent. Carrying them would serialise a promise to send bytes
    /// down a link that no longer exists.
    ///
    /// Nothing is lost by dropping them, because the courier does not need to
    /// remember *who* wanted this — only *what* was wanted. It keeps asking, and
    /// what comes back is stored like any other envelope; when it meets the
    /// original asker again, that node asks once more and is answered from the
    /// cache. The demand is carried as the id, exactly as the file layer carries
    /// demand as the manifest.
    pub fn pending_interests(&self) -> Vec<u8> {
        let n = self.interests.len().min(u16::MAX as usize);
        let mut out = Vec::with_capacity(3 + n * 20);
        out.push(1);
        out.extend_from_slice(&(n as u16).to_be_bytes());
        for (id, i) in self.interests.iter().take(n) {
            out.extend_from_slice(id);
            out.extend_from_slice(&i.until.to_be_bytes());
        }
        out
    }

    /// Restore interests from [`Node::pending_interests`], **merging** them.
    ///
    /// Merging rather than replacing, the opposite of `restore_prekey_ring` and
    /// for the opposite reason: a prekey blob is the authority on which secrets
    /// still exist, whereas an interest is a *want*, and a node that has since
    /// acquired others has not stopped having them. A restored entry never
    /// overwrites a live one, since the live one has real waiters attached.
    ///
    /// Restored entries arrive with no waiters and are dropped if already expired
    /// or past the ceiling — a blob cannot buy a longer promise than the node
    /// would have made itself, or "restore" would be a way to hand a node an
    /// interest it never adopted and could not have. `limits.interests` still
    /// caps the total. Returns `false` and changes nothing if the blob is
    /// malformed.
    pub fn restore_pending_interests(&mut self, blob: &[u8], now: u32) -> bool {
        if blob.len() < 3 || blob[0] != 1 {
            return false;
        }
        let n = u16::from_be_bytes([blob[1], blob[2]]) as usize;
        if blob.len() != 3 + n * 20 {
            return false;
        }
        let ceiling = now.saturating_add(MAX_INTEREST_LEASE_SECS);
        for i in 0..n {
            let o = 3 + i * 20;
            let mut id = [0u8; 16];
            id.copy_from_slice(&blob[o..o + 16]);
            let until = u32::from_be_bytes([blob[o + 16], blob[o + 17], blob[o + 18], blob[o + 19]]);
            if until <= now || self.interests.contains_key(&id) {
                continue;
            }
            if self.interests.len() >= self.limits.interests {
                break;
            }
            self.interests.insert(id, Interest { waiters: Vec::new(), until: until.min(ceiling) });
        }
        true
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
