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

    /// Snoop a received frame: if signed, bind the sender's SPORE address to
    /// `under` and return it (handy as the `nbr` for `Node::on_rx`). Unsigned
    /// frames teach nothing — you can only bind an address you can verify.
    pub fn snoop(&mut self, frame: &[u8], under: U, now: u32) -> Option<Addr> {
        let (e, _) = Envelope::decode(frame).ok()?;
        if e.flags & fl::SIGNED == 0 {
            return None;
        }
        let a = match &e.src {
            Src::Full(pk) => addr_of(pk),
            Src::Short(a) => *a,
            Src::None => return None,
        };
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
