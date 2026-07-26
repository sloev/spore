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
}

impl<U: Clone + PartialEq> Neighbors<U> {
    /// `ttl` seconds before a learned binding goes stale.
    pub fn new(ttl: u32) -> Self {
        Neighbors { map: HashMap::new(), ttl, keep: 3 }
    }

    /// Record that SPORE address `spore` is reachable at underlay `under`.
    /// Keeps up to a few freshest bindings per address (like keeping paths).
    pub fn learn(&mut self, spore: Addr, under: U, now: u32) {
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
