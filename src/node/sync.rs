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
        let rate = self.gossip_rate;
        for chunk in e.payload.chunks(16).take(MAX_IDS_PER_GOSSIP) {
            if chunk.len() != 16 {
                continue;
            }
            let mut id = [0u8; 16];
            id.copy_from_slice(chunk);
            // `store.wire` may read from the spill directory, so an unbounded WANT
            // buys disk reads as well as bandwidth.
            let Some(wire) = self.store.wire(&id) else { continue };
            let bucket = self.gossip.entry(iface).or_insert_with(|| congestion::TokenBucket::new(rate));
            if !bucket.allow(wire.len() as u32, now) {
                break; // this link has spent its share; it can ask again later
            }
            rx.forwards.push(Forward::Directed { iface, nbr, bytes: wire });
        }
        rx
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

    /// Receive-side fragmentation status: for each in-progress fountain
    /// reassembly, `(original id, independent fragments held, total needed)` —
    /// so a UI can show "receiving X/N".
    pub fn frag_progress(&self) -> Vec<(Id, u8, u8)> {
        self.frags
            .iter()
            .filter(|(_, f)| f.done.is_none())
            .map(|(id, f)| (*id, f.rows.len() as u8, f.count as u8))
            .collect()
    }

    // ---- datagram sessions (§ application layer, tag 0x04) ---------------
}
