//! Address resolution — the ARP/NDP of SPORE, shared by every bridge.

use crate::*;

/// A generic neighbour table mapping a SPORE address to the underlay address
/// `U` it was last heard from. `U` is whatever a medium names a peer with: a
/// `SocketAddr`, a 6-byte MAC, a Meshtastic `u32`, a Zigbee `u64`, a stateful
/// connection handle, or `()` for broadcast-only media (audio, raw LoRa, QR).
///
/// Learned passively by snooping the underlay source of *signed* frames — no
/// extra traffic — exactly like a learning switch or an ARP cache. Bridges
/// call `resolve` to turn a directed send into an underlay unicast, and fall
/// back to broadcast when the answer is unknown. Write it once; every bridge
/// reuses it, differing only in the concrete `U` and its send primitives.
pub struct Neighbors<U> {
    map: HashMap<Addr, Vec<(U, u32)>>, // spore addr -> [(underlay addr, last_seen)]
    ttl: u32,
    keep: usize,
    max: usize,
}

/// Most SPORE addresses one table will hold.
///
/// `keep` bounds the bindings held *per address*; this bounds how many addresses
/// there can be, which is the part an attacker chooses. Requiring a valid
/// signature to learn (see [`Neighbors::snoop`]) raises the price of a forged
/// binding from nothing to almost nothing: minting an identity is one keypair,
/// and Ed25519 keygen is fast, so a peer can still mint as many *genuinely
/// signed* addresses as it likes and grow this map for the whole TTL. Verified
/// is not the same as scarce. `congestion::Quotas` caps its tracked sources at
/// the same number for the same reason.
pub const MAX_NEIGHBOURS: usize = 4096;

impl<U: Clone + PartialEq> Neighbors<U> {
    /// `ttl` seconds before a learned binding goes stale.
    pub fn new(ttl: u32) -> Self {
        Neighbors { map: HashMap::new(), ttl, keep: 3, max: MAX_NEIGHBOURS }
    }

    /// Cap the number of addresses held. Mostly for tests and for bridges on
    /// memory-tight hardware; [`MAX_NEIGHBOURS`] is the default.
    pub fn with_max(mut self, max: usize) -> Self {
        self.max = max.max(1);
        self
    }

    /// Record that SPORE address `spore` is reachable at underlay `under`.
    /// Keeps up to a few freshest bindings per address (like keeping paths).
    pub fn learn(&mut self, spore: Addr, under: U, now: u32) {
        // Only a *new* address can grow the table, so only that path pays for
        // the check. Drop stale bindings first — that is free capacity and the
        // common case on a live link — and evict the least-recently-heard
        // address only if the table is still full.
        if !self.map.contains_key(&spore) && self.map.len() >= self.max {
            self.expire(now);
            if self.map.len() >= self.max {
                self.evict_stalest();
            }
        }
        let v = self.map.entry(spore).or_default();
        v.retain(|(u, _)| *u != under);
        v.insert(0, (under, now)); // freshest first
        v.truncate(self.keep);
    }

    /// Snoop a received frame: if it carries a signature that *verifies*, bind
    /// the sender's SPORE address to `under` and return it (handy as the `nbr`
    /// for `Node::on_rx`). Frames that teach nothing return `None`.
    ///
    /// The signature is checked, not merely the `SIGNED` flag. That flag is one
    /// bit chosen by whoever wrote the frame, so trusting it lets anyone bind
    /// any address they can name: copy a victim's *public* key into `src`, set
    /// the bit, attach 64 bytes of zeroes. Sealed payloads stay unreadable, but
    /// every directed send for that victim is then unicast to the forger instead
    /// of the victim — which is precisely the "make a node believe a false
    /// address" that the bridge layer promises cannot happen.
    ///
    /// `Src::Short` carries no key, so nothing here *can* verify it — an 8-byte
    /// address is a claim, not evidence. It teaches nothing, which is the rule
    /// the node's own path learning already follows. A bridge that loses a
    /// binding falls back to broadcast, which always reaches the peer; a bridge
    /// that learns a forged one does not.
    pub fn snoop(&mut self, frame: &[u8], under: U, now: u32) -> Option<Addr> {
        let (e, _) = Envelope::decode(frame).ok()?;
        // `verify` re-checks the SIGNED flag and matches Src::Full itself, so
        // this is the whole test: a signature by the key the address is made of.
        let Src::Full(pk) = &e.src else { return None };
        if !e.verify() {
            return None;
        }
        let a = addr_of(pk);
        self.learn(a, under, now);
        Some(a)
    }

    /// Resolve a SPORE address to a fresh underlay address, or `None` — in
    /// which case the bridge should broadcast (which always reaches it).
    pub fn resolve(&self, spore: &Addr, now: u32) -> Option<U> {
        self.map
            .get(spore)?
            .iter()
            .find(|(_, seen)| now.saturating_sub(*seen) < self.ttl)
            .map(|(u, _)| u.clone())
    }

    /// Drop a neighbour outright — e.g. when a *stateful* connection (a
    /// WebSocket, a BLE GATT link) closes and its handle is dead.
    pub fn forget(&mut self, spore: &Addr) {
        self.map.remove(spore);
    }

    /// Forget the address whose freshest binding is oldest. Losing a binding is
    /// safe — the bridge falls back to broadcast, which always reaches the peer —
    /// so under pressure the table degrades to "slower" rather than to "unbounded".
    fn evict_stalest(&mut self) {
        let stalest = self
            .map
            .iter()
            .min_by_key(|(_, v)| v.iter().map(|(_, seen)| *seen).max().unwrap_or(0))
            .map(|(k, _)| *k);
        if let Some(k) = stalest {
            self.map.remove(&k);
        }
    }

    /// Evict every stale binding.
    pub fn expire(&mut self, now: u32) {
        for v in self.map.values_mut() {
            v.retain(|(_, seen)| now.saturating_sub(*seen) < self.ttl);
        }
        self.map.retain(|_, v| !v.is_empty());
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_cannot_be_grown_without_bound() {
        // Signatures are checked before learning, but a signature only proves a
        // keypair — and keypairs are cheap. A peer minting identities must not be
        // able to grow this map forever.
        let now = 1_700_000_000;
        let mut n: Neighbors<u32> = Neighbors::new(3600).with_max(8);
        for i in 0..500u32 {
            let mut a = [0u8; 8];
            a[..4].copy_from_slice(&i.to_be_bytes());
            n.learn(a, i, now);
        }
        assert!(n.len() <= 8, "held {} bindings against a cap of 8", n.len());

        // The freshest arrival survives; losing older ones is safe, since an
        // unresolved address just falls back to broadcast.
        let mut last = [0u8; 8];
        last[..4].copy_from_slice(&499u32.to_be_bytes());
        assert_eq!(n.resolve(&last, now), Some(499));
    }

    #[test]
    fn stale_bindings_are_reclaimed_before_anything_live_is_evicted() {
        let now = 1_700_000_000;
        let mut n: Neighbors<u32> = Neighbors::new(100).with_max(4);
        for i in 0..4u32 {
            let mut a = [0u8; 8];
            a[..4].copy_from_slice(&i.to_be_bytes());
            n.learn(a, i, now);
        }
        // Long enough later that every existing binding is stale: the newcomer
        // should take reclaimed space rather than evicting a live peer.
        let later = now + 1_000;
        n.learn([9u8; 8], 99, later);
        assert_eq!(n.resolve(&[9u8; 8], later), Some(99));
        assert!(n.len() <= 4);
    }
}
