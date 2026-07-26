//! Self-distribution — the network carries its own genome.
//!
//! SPORE can hand out its own installer. A **bootstrap bundle** (the source, the
//! manual, prebuilt binaries — whatever regrows a node) is published as an
//! ordinary content-addressed, signed file on a well-known topic. Any node that
//! holds it can serve it, and a newcomer fetches it by magnet and verifies every
//! chunk against the signed manifest for free — so getting SPORE doesn't depend
//! on a download server that might not be there.
//!
//! A **seed vault** is a node that [`Node::pin`]s the bundle so memory pressure
//! never evicts it: a long-term custodian that keeps the genome alive.
//!
//! This is all a thin convention over the existing file machinery (`file`,
//! `publish_file`/`fetch`/`file_bytes`); there is no new envelope type.

use crate::*;

/// The well-known topic bootstrap bundles are published on. Every node agrees on
/// it, so `latest_bundle()` finds the newest genome without any directory.
pub const BOOTSTRAP_TOPIC: &str = "spore/bootstrap/v1";

/// The address of [`BOOTSTRAP_TOPIC`].
pub fn topic() -> Addr {
    topic_of(BOOTSTRAP_TOPIC)
}

impl Node {
    /// Publish a bootstrap bundle on the well-known topic. Content-addressed and
    /// signed like any file; returns the magnet (manifest ID) and the forwards
    /// to flood the small manifest. The data is pulled on demand, BitTorrent-style.
    pub fn publish_bundle(&mut self, name: &str, bytes: &[u8], now: u32) -> (Id, Vec<Forward>) {
        self.publish_file(name, bytes, topic(), now)
    }

    /// Every bootstrap bundle we know a manifest for, as `(magnet, name, complete)`,
    /// where `complete` means we hold all its chunks and can serve it.
    pub fn bundles(&self) -> Vec<(Id, String, bool)> {
        let t = topic();
        let mut out: Vec<(Id, String, bool)> = self
            .manifests
            .iter()
            .filter(|(magnet, _)| self.store.meta(magnet).map(|s| s.dest) == Some(t))
            .map(|(magnet, m)| (*magnet, m.name.clone(), self.has_file(magnet)))
            .collect();
        // Newest first, by the manifest envelope's expiry.
        out.sort_by_key(|(magnet, _, _)| {
            std::cmp::Reverse(self.store.meta(magnet).map(|s| s.expiry).unwrap_or(0))
        });
        out
    }

    /// The newest bootstrap bundle we hold a manifest for, `(magnet, name)`.
    pub fn latest_bundle(&self) -> Option<(Id, String)> {
        self.bundles().into_iter().next().map(|(magnet, name, _)| (magnet, name))
    }

    /// Pin a file (by magnet) so a seed vault never evicts it under memory
    /// pressure — the manifest and every chunk it holds are kept forever. Idempotent.
    pub fn pin(&mut self, magnet: &Id) {
        self.pinned.insert(*magnet);
    }

    /// Stop pinning `magnet`; it becomes evictable again once complete.
    pub fn unpin(&mut self, magnet: &Id) {
        self.pinned.remove(magnet);
    }

    /// Whether `magnet` is explicitly pinned.
    pub fn is_pinned(&self, magnet: &Id) -> bool {
        self.pinned.contains(magnet)
    }
}

#[cfg(test)]
mod tests {
    use crate::*;

    const NOW: u32 = 1_700_000_000;

    // Move the manifest + chunk envelopes of a just-published file from the
    // publisher into `rx` by feeding them to the router, the way a real transfer
    // would (manifest floods, chunks are pulled). Returns the magnet.
    fn transfer_latest(publisher: &Node, rx: &mut Node, magnet: &Id) {
        let mut manifest = Vec::new();
        let mut chunks: Vec<Vec<u8>> = Vec::new();
        for (_, wire) in publisher.store_wires() {
            let (e, _) = Envelope::decode(&wire).unwrap();
            match e.payload.first() {
                Some(&file::MANIFEST_TAG) if e.id() == *magnet => manifest = wire,
                Some(&file::CHUNK_TAG) => chunks.push(wire),
                _ => {}
            }
        }
        rx.on_rx(&manifest, 0, None, NOW);
        for c in &chunks {
            rx.on_rx(c, 0, None, NOW);
        }
    }

    #[test]
    fn bundle_publishes_verifies_and_is_discoverable() {
        let mut src = Node::new("origin", &[]);
        let genome: Vec<u8> = (0..5000u32).map(|i| (i.wrapping_mul(31)) as u8).collect();
        let (magnet, _f) = src.publish_bundle("spore-bootstrap.tar", &genome, NOW);

        // The publisher advertises it on the well-known bootstrap topic.
        assert_eq!(src.latest_bundle(), Some((magnet, "spore-bootstrap.tar".to_string())));

        // A fresh node that follows the bootstrap topic receives the manifest +
        // chunks and rebuilds + verifies it.
        let mut rx = Node::new("newcomer", &[bundle::BOOTSTRAP_TOPIC]);
        transfer_latest(&src, &mut rx, &magnet);
        assert_eq!(rx.latest_bundle(), Some((magnet, "spore-bootstrap.tar".to_string())));
        assert_eq!(rx.file_bytes(&magnet).as_deref(), Some(&genome[..]), "bundle reassembles");
    }

    #[test]
    fn latest_bundle_picks_the_newest() {
        let mut src = Node::new("origin", &[]);
        let (m_old, _) = src.publish_bundle("v1.tar", &vec![1u8; 2000], NOW);
        // A later expiry wins; publish_bundle stamps expiry = now + 7 days.
        let (m_new, _) = src.publish_bundle("v2.tar", &vec![2u8; 2000], NOW + 10);
        assert_ne!(m_old, m_new);
        assert_eq!(src.latest_bundle().map(|(m, _)| m), Some(m_new), "newest bundle wins");
    }

    #[test]
    fn a_seed_vault_pins_the_bundle_through_eviction() {
        let mut vault = Node::new("vault", &[]);
        let genome: Vec<u8> = (0..4000u32).map(|i| (i.wrapping_mul(7)) as u8).collect();
        let (magnet, _f) = vault.publish_bundle("genome.tar", &genome, NOW);
        assert!(vault.file_bytes(&magnet).is_some(), "vault holds a complete bundle");

        // Pin it, then shrink the store hard and flood junk to force eviction.
        vault.pin(&magnet);
        assert!(vault.is_pinned(&magnet));
        vault.set_store_budget(1000);
        for _ in 0..200 {
            let mut j = Node::new("j", &[]);
            let f = j.originate(ZERO_DEST, vec![0xEE; 240], NOW);
            let wire = match &f[0] {
                Forward::Flood { bytes, .. } | Forward::Directed { bytes, .. } => bytes.clone(),
            };
            vault.on_rx(&wire, 0, Some(j.addr), NOW);
        }
        // A pinned, *complete* bundle survives — unlike an ordinary completed file.
        assert_eq!(vault.file_bytes(&magnet).as_deref(), Some(&genome[..]), "pinned bundle kept");

        // Unpinning lets it be reclaimed under the same pressure.
        vault.unpin(&magnet);
        for _ in 0..200 {
            let mut j = Node::new("j", &[]);
            let f = j.originate(ZERO_DEST, vec![0xEE; 240], NOW);
            let wire = match &f[0] {
                Forward::Flood { bytes, .. } | Forward::Directed { bytes, .. } => bytes.clone(),
            };
            vault.on_rx(&wire, 0, Some(j.addr), NOW);
        }
        assert!(vault.file_bytes(&magnet).is_none(), "unpinned bundle is evictable again");
    }
}
