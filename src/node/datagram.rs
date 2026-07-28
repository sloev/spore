//! Node — the UDP-like session layer: dial, dg_send, dg_recv, reliable.
//!
//! Split out of the 3977-line `lib.rs` (task #23): `impl Node` in a descendant
//! module of the crate root, so it keeps full access to `Node`'s private fields
//! with no visibility change. A pure move — wire format and public API identical.

use crate::*;

impl Node {
    /// Open a UDP-like session to `peer` on `port`. Returns `None` until we've
    /// heard the peer's prekey (from an ANNOUNCE). No handshake: identity is the
    /// address, so the "connection" is just soft local state.
    pub fn dial(&self, peer: Addr, port: u16) -> Option<session::Session> {
        Some(session::Session::new(self.addr, peer, port, self.peer_prekey(&peer)?))
    }

    /// Send one datagram on a session: seal the bytes to the peer's prekey, wrap
    /// them with a replay sequence, sign the envelope, and hand the transport the
    /// `Forward`s. Best-effort and unordered, exactly like UDP.
    pub fn dg_send(&mut self, s: &mut session::Session, data: &[u8], now: u32) -> Vec<Forward> {
        let seq = s.next_tx_seq();
        let sealed = seal(data, &s.peer_prekey());
        let mut payload = Vec::with_capacity(11 + sealed.len());
        payload.push(session::TAG_DGRAM);
        payload.extend_from_slice(&s.port().to_be_bytes());
        payload.extend_from_slice(&seq.to_be_bytes());
        payload.extend_from_slice(&sealed);

        let mut e = Envelope::new(ty::DATA, s.peer(), now + SESSION_EXPIRY_SECS, payload);
        // No path yet -> flood to discover it; the signed reply teaches the
        // reverse path, and subsequent datagrams go directed (§5.6).
        if self.paths.fresh(&s.peer(), now).is_none() {
            e.flags |= fl::FLOOD;
        }
        e.sign(&self.sk);
        // Dedup our own copy off the flood, but don't clog the store with
        // ephemeral session traffic.
        self.mark_seen(&e);
        self.forward_intents(&e, NO_IFACE, now)
    }

    /// Parse an inbound datagram envelope for session `s`: check the port,
    /// authenticate the sender (its key must hash to the session peer), verify
    /// the signature, reject replays, and decrypt. `None` if it isn't a valid,
    /// fresh datagram for this session.
    pub fn dg_recv(&self, s: &mut session::Session, e: &Envelope) -> Option<Vec<u8>> {
        if e.typ != ty::DATA || e.payload.len() < 11 || e.payload[0] != session::TAG_DGRAM {
            return None;
        }
        if u16::from_be_bytes([e.payload[1], e.payload[2]]) != s.port() {
            return None;
        }
        let Src::Full(pk) = &e.src else { return None };
        if addr_of(pk) != s.peer() || !e.verify() {
            return None;
        }
        let mut sb = [0u8; 8];
        sb.copy_from_slice(&e.payload[3..11]);
        let seq = u64::from_be_bytes(sb);
        let data = self.open(&e.payload[11..])?;
        if !s.accept_rx(seq) {
            return None; // replay or too old
        }
        Some(data)
    }

    /// Wrap a session in a simple QUIC-style reliable, ordered byte stream.
    pub fn reliable(&self, s: session::Session) -> session::Reliable {
        let max_frame = self.mtu.saturating_sub(200).max(1);
        session::Reliable::new(s, max_frame)
    }

    // ---- files: content-addressed objects (§ application layer) ----------
}
