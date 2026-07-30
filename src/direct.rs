//! SPORE Direct — a negotiated, non-routed, end-to-end encrypted datagram pipe
//! between two SPORE identities over a **direct underlay** (UDP, TCP, BLE, …).
//!
//! Store-and-forward stays the right plane for asynchronous mesh delivery; it is
//! the wrong one for full-duplex, low-latency media (voice, telemetry, a live
//! terminal). Direct is that other plane: two peers agree — on the SPORE plane —
//! on a medium and an ephemeral key, then talk **directly** over it.
//!
//! This is an **application-level profile**. It adds nothing to the wire: the
//! envelope, store, hub, router and the frozen v1 contract are untouched. The
//! only thing that crosses the SPORE mesh is opaque `SPDR` signalling bytes,
//! carried by the app over the ordinary sealed+signed [`Node::send_direct`] path
//! (see `docs/DIRECT.md`), which is what binds an offer's ephemeral key to a real
//! SPORE identity. Everything here is pure: no sockets, no `Node` dependency, so
//! it is fully unit-testable and compiles everywhere the core does.
//!
//! Crypto reuses the primitives the core already vets: X25519 for the ephemeral
//! agreement ([`crate::ratchet::keypair`]), BLAKE2b as the KDF (the same
//! construction the ratchet uses, rather than pulling in a second hash), and
//! ChaCha20-Poly1305 for the record AEAD. The media keys never appear on the
//! wire, and both SPORE addresses plus the medium are bound into the KDF `info`,
//! so a record only opens for the exact pair that negotiated it.

use crate::Addr;
use blake2::digest::{Update as _, VariableOutput};
use blake2::Blake2bVar;
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use x25519_dalek::{PublicKey, StaticSecret};

/// Wire magic so another app ignores an SPDR payload it doesn't understand.
pub const MAGIC: &[u8; 4] = b"SPDR";
/// Profile version. Bumping it changes the signalling and key schedule; a peer on
/// a different version is rejected rather than mis-negotiated.
pub const VERSION: u8 = 1;

/// A transport-capable medium. `est_bps`/`mtu` in a candidate describe *this*
/// link; the enum only names the family so both sides agree on what they chose.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Medium {
    Udp = 0,
    Tcp = 1,
    Ble = 2,
    EspNow = 3,
}

impl Medium {
    fn from_u8(v: u8) -> Option<Medium> {
        Some(match v {
            0 => Medium::Udp,
            1 => Medium::Tcp,
            2 => Medium::Ble,
            3 => Medium::EspNow,
            _ => return None,
        })
    }
}

/// Record type — the first byte inside the AEAD-framed record. `Data` is the
/// general best-effort datagram; `Keepalive` holds a NAT mapping open; `Control`
/// is for in-pipe signalling an app layers on top. Ordered streams or RPC retries
/// live *above* this record so a lost media frame never head-of-line-blocks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum RecordType {
    Media = 0,
    Keepalive = 1,
    Control = 2,
    Data = 3,
    Stream = 4,
}

impl RecordType {
    fn from_u8(v: u8) -> Option<RecordType> {
        Some(match v {
            0 => RecordType::Media,
            1 => RecordType::Keepalive,
            2 => RecordType::Control,
            3 => RecordType::Data,
            4 => RecordType::Stream,
            _ => return None,
        })
    }
}

/// What the initiator needs from the pipe, used to filter the responder's usable
/// mediums. `mtu_needed` and `min_bps` reject a link that can't carry the app's
/// traffic; `max_latency_ms` is an optional hint used only for ranking.
#[derive(Clone, Copy, Debug)]
pub struct Need {
    pub min_bps: u32,
    pub mtu_needed: u16,
    pub max_latency_ms: Option<u16>,
}

/// One offered path: a medium this peer can be reached on, and its measured/est.
/// capacity. `locator` is the opaque address on that medium (an `ip:port`, a BLE
/// handle, …) the other side will actually connect to — never interpreted here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    pub medium: Medium,
    pub locator: Vec<u8>,
    pub est_bps: u32,
    pub mtu: u16,
    pub rtt_hint_ms: u16,
}

/// The initiator's OFFER: who it is, the fresh pipe id, its need, its ephemeral
/// public key, and the paths it can be reached on.
#[derive(Clone, Debug)]
pub struct Offer {
    pub pipe_id: [u8; 16],
    pub from: Addr,
    pub to: Addr,
    pub need: Need,
    pub eph_pub: [u8; 32],
    pub candidates: Vec<Candidate>,
}

/// Why an OFFER was refused. Distinct reasons so the initiator can tell "try later"
/// (`Busy`) from "we will never agree" (`NoMedium`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Reject {
    NoMedium = 0,
    Throughput = 1,
    Busy = 2,
}

/// The responder's ANSWER: accept with a chosen path + its ephemeral key, or a
/// reason it declined.
#[derive(Clone, Debug)]
pub enum Answer {
    Ok { pipe_id: [u8; 16], eph_pub: [u8; 32], chosen: Candidate },
    Reject { pipe_id: [u8; 16], reason: Reject },
}

// ---- signalling codec (SPDR) --------------------------------------------------

const T_OFFER: u8 = 1;
const T_ANSWER: u8 = 2;

fn put_u16(v: &mut Vec<u8>, n: u16) {
    v.extend_from_slice(&n.to_be_bytes());
}
fn put_u32(v: &mut Vec<u8>, n: u32) {
    v.extend_from_slice(&n.to_be_bytes());
}
fn put_bytes(v: &mut Vec<u8>, b: &[u8]) {
    put_u16(v, b.len().min(u16::MAX as usize) as u16);
    v.extend_from_slice(&b[..b.len().min(u16::MAX as usize)]);
}

/// A cursor that never reads past the end — every getter returns `None` rather
/// than panicking, so a truncated or hostile SPDR payload is a clean rejection.
struct Rd<'a> {
    b: &'a [u8],
    o: usize,
}
impl<'a> Rd<'a> {
    fn new(b: &'a [u8]) -> Rd<'a> {
        Rd { b, o: 0 }
    }
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let s = self.b.get(self.o..self.o + n)?;
        self.o += n;
        Some(s)
    }
    fn u8(&mut self) -> Option<u8> {
        Some(self.take(1)?[0])
    }
    fn u16(&mut self) -> Option<u16> {
        Some(u16::from_be_bytes(self.take(2)?.try_into().ok()?))
    }
    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_be_bytes(self.take(4)?.try_into().ok()?))
    }
    fn arr8(&mut self) -> Option<Addr> {
        self.take(8)?.try_into().ok()
    }
    fn arr16(&mut self) -> Option<[u8; 16]> {
        self.take(16)?.try_into().ok()
    }
    fn arr32(&mut self) -> Option<[u8; 32]> {
        self.take(32)?.try_into().ok()
    }
    fn bytes(&mut self) -> Option<Vec<u8>> {
        let n = self.u16()? as usize;
        Some(self.take(n)?.to_vec())
    }
}

fn hdr(v: &mut Vec<u8>, typ: u8) {
    v.extend_from_slice(MAGIC);
    v.push(VERSION);
    v.push(typ);
}

fn check_hdr(b: &[u8], typ: u8) -> Option<Rd<'_>> {
    let mut r = Rd::new(b);
    if r.take(4)? != MAGIC || r.u8()? != VERSION || r.u8()? != typ {
        return None;
    }
    Some(r)
}

impl Offer {
    /// Encode this OFFER as SPDR bytes for the app to carry over the SPORE plane.
    pub fn encode(&self) -> Vec<u8> {
        let mut v = Vec::new();
        hdr(&mut v, T_OFFER);
        v.extend_from_slice(&self.pipe_id);
        v.extend_from_slice(&self.from);
        v.extend_from_slice(&self.to);
        put_u32(&mut v, self.need.min_bps);
        put_u16(&mut v, self.need.mtu_needed);
        put_u16(&mut v, self.need.max_latency_ms.unwrap_or(0));
        v.push(self.need.max_latency_ms.is_some() as u8);
        v.extend_from_slice(&self.eph_pub);
        v.push(self.candidates.len().min(255) as u8);
        for c in self.candidates.iter().take(255) {
            v.push(c.medium as u8);
            put_bytes(&mut v, &c.locator);
            put_u32(&mut v, c.est_bps);
            put_u16(&mut v, c.mtu);
            put_u16(&mut v, c.rtt_hint_ms);
        }
        v
    }

    /// Parse SPDR bytes back into an OFFER, or `None` if they aren't a v1 offer.
    pub fn decode(b: &[u8]) -> Option<Offer> {
        let mut r = check_hdr(b, T_OFFER)?;
        let pipe_id = r.arr16()?;
        let from = r.arr8()?;
        let to = r.arr8()?;
        let min_bps = r.u32()?;
        let mtu_needed = r.u16()?;
        let lat = r.u16()?;
        let has_lat = r.u8()? != 0;
        let eph_pub = r.arr32()?;
        let n = r.u8()? as usize;
        let mut candidates = Vec::with_capacity(n);
        for _ in 0..n {
            let medium = Medium::from_u8(r.u8()?)?;
            let locator = r.bytes()?;
            let est_bps = r.u32()?;
            let mtu = r.u16()?;
            let rtt_hint_ms = r.u16()?;
            candidates.push(Candidate { medium, locator, est_bps, mtu, rtt_hint_ms });
        }
        Some(Offer {
            pipe_id,
            from,
            to,
            need: Need { min_bps, mtu_needed, max_latency_ms: has_lat.then_some(lat) },
            eph_pub,
            candidates,
        })
    }
}

impl Answer {
    /// Encode this ANSWER as SPDR bytes.
    pub fn encode(&self) -> Vec<u8> {
        let mut v = Vec::new();
        hdr(&mut v, T_ANSWER);
        match self {
            Answer::Ok { pipe_id, eph_pub, chosen } => {
                v.extend_from_slice(pipe_id);
                v.push(1); // ok
                v.extend_from_slice(eph_pub);
                v.push(chosen.medium as u8);
                put_bytes(&mut v, &chosen.locator);
                put_u32(&mut v, chosen.est_bps);
                put_u16(&mut v, chosen.mtu);
                put_u16(&mut v, chosen.rtt_hint_ms);
            }
            Answer::Reject { pipe_id, reason } => {
                v.extend_from_slice(pipe_id);
                v.push(0); // reject
                v.push(*reason as u8);
            }
        }
        v
    }

    /// Parse SPDR bytes back into an ANSWER, or `None` if malformed.
    pub fn decode(b: &[u8]) -> Option<Answer> {
        let mut r = check_hdr(b, T_ANSWER)?;
        let pipe_id = r.arr16()?;
        match r.u8()? {
            1 => {
                let eph_pub = r.arr32()?;
                let medium = Medium::from_u8(r.u8()?)?;
                let locator = r.bytes()?;
                let est_bps = r.u32()?;
                let mtu = r.u16()?;
                let rtt_hint_ms = r.u16()?;
                Some(Answer::Ok {
                    pipe_id,
                    eph_pub,
                    chosen: Candidate { medium, locator, est_bps, mtu, rtt_hint_ms },
                })
            }
            0 => {
                let reason = match r.u8()? {
                    0 => Reject::NoMedium,
                    1 => Reject::Throughput,
                    2 => Reject::Busy,
                    _ => return None,
                };
                Some(Answer::Reject { pipe_id, reason })
            }
            _ => None,
        }
    }
}

// ---- medium selection ---------------------------------------------------------

/// Choose the best offered candidate this responder can actually serve.
///
/// Keeps only candidates whose medium the responder is willing to use *and* that
/// meet the initiator's `min_bps` and `mtu_needed`, then ranks by latency hint
/// (lower first), breaking ties by capacity (higher first). Returns the chosen
/// candidate or the reason none fit: `Throughput` when candidates existed but all
/// were too slow/small, `NoMedium` when none overlapped the responder's set.
pub fn choose<'a>(offer: &'a Offer, willing: &[Medium]) -> Result<&'a Candidate, Reject> {
    let overlap: Vec<&Candidate> = offer.candidates.iter().filter(|c| willing.contains(&c.medium)).collect();
    if overlap.is_empty() {
        return Err(Reject::NoMedium);
    }
    let mut fit: Vec<&Candidate> = overlap
        .into_iter()
        .filter(|c| c.est_bps >= offer.need.min_bps && c.mtu >= offer.need.mtu_needed)
        .collect();
    if fit.is_empty() {
        return Err(Reject::Throughput);
    }
    fit.sort_by(|a, b| a.rtt_hint_ms.cmp(&b.rtt_hint_ms).then(b.est_bps.cmp(&a.est_bps)));
    Ok(fit[0])
}

// ---- key schedule -------------------------------------------------------------

fn dh(sec: &[u8; 32], pubk: &[u8; 32]) -> [u8; 32] {
    StaticSecret::from(*sec).diffie_hellman(&PublicKey::from(*pubk)).to_bytes()
}

/// Derive the pair's directional keys from the ephemeral DH, binding both SPORE
/// addresses, the pipe id and the medium into the KDF so a record only opens for
/// the exact pair, pipe and link that negotiated it. Returns
/// `(initiator_tx, initiator_rx)`; the responder uses the same two swapped.
fn derive(
    shared: &[u8; 32],
    pipe_id: &[u8; 16],
    initiator: &Addr,
    responder: &Addr,
    medium: Medium,
) -> ([u8; 32], [u8; 32]) {
    let mut h = Blake2bVar::new(64).unwrap();
    h.update(shared);
    h.update(b"spore-direct-v1");
    h.update(pipe_id);
    h.update(initiator);
    h.update(responder);
    h.update(&[medium as u8]);
    let mut out = [0u8; 64];
    h.finalize_variable(&mut out).unwrap();
    let mut a = [0u8; 32];
    let mut b = [0u8; 32];
    a.copy_from_slice(&out[..32]);
    b.copy_from_slice(&out[32..]);
    (a, b)
}

fn nonce(seq: u16) -> [u8; 12] {
    let mut n = [0u8; 12];
    n[10..].copy_from_slice(&seq.to_be_bytes());
    n
}

// ---- transport abstraction ----------------------------------------------------

/// One direction-agnostic direct link: whatever the chosen medium's adapter
/// provides. The pipe drives it by polling — no callbacks — the same model the
/// JNI bridges use. Adapters (UDP, TCP, BLE) implement this; the crate ships an
/// in-memory [`Loopback`] for tests and for wiring two in-process peers.
pub trait DatagramPort {
    /// Largest record body this link carries in one shot.
    fn mtu(&self) -> usize;
    /// Best-effort send of one framed record. An error is the link's problem to
    /// report; the pipe treats a send as fire-and-forget.
    fn send(&mut self, frame: &[u8]) -> std::io::Result<()>;
    /// One received frame if the link has one buffered, else `None`.
    fn try_recv(&mut self) -> Option<Vec<u8>>;
}

// ---- the pipe ----------------------------------------------------------------

/// An open direct pipe: the two directional keys, per-direction sequence numbers,
/// and the transport it rides. Best-effort datagram — a dropped or reordered
/// record is the app's to handle (or not, for media). Keys are dropped with the
/// pipe.
pub struct Pipe<P: DatagramPort> {
    tx_key: [u8; 32],
    rx_key: [u8; 32],
    pipe_id: [u8; 16],
    tx_seq: u16,
    port: P,
}

/// The initiator's half-open state between sending an OFFER and receiving the
/// ANSWER. Holds the ephemeral secret so the keys can be derived once the
/// responder's ephemeral public key comes back.
pub struct Pending {
    pipe_id: [u8; 16],
    from: Addr,
    to: Addr,
    eph_sec: [u8; 32],
}

impl<P: DatagramPort> Pipe<P> {
    /// Build the initiator's OFFER. Returns the SPDR bytes to carry to the peer
    /// over the SPORE plane, plus the [`Pending`] state to finish with the answer.
    pub fn offer(
        from: Addr,
        to: Addr,
        pipe_id: [u8; 16],
        need: Need,
        candidates: Vec<Candidate>,
    ) -> (Vec<u8>, Pending) {
        let (eph_sec, eph_pub) = crate::ratchet::keypair();
        let offer = Offer { pipe_id, from, to, need, eph_pub, candidates };
        (offer.encode(), Pending { pipe_id, from, to, eph_sec })
    }

    /// Responder side: given an OFFER and the mediums we're willing to use, pick a
    /// candidate, derive keys, and open our end over `port` (built for the chosen
    /// medium). Returns the ANSWER bytes to send back and our live [`Pipe`]. On no
    /// fit, returns the ANSWER bytes carrying the reject and no pipe.
    pub fn answer(offer: &Offer, me: Addr, willing: &[Medium], port: P) -> (Vec<u8>, Option<Pipe<P>>) {
        let chosen = match choose(offer, willing) {
            Ok(c) => c.clone(),
            Err(reason) => {
                return (Answer::Reject { pipe_id: offer.pipe_id, reason }.encode(), None);
            }
        };
        let (eph_sec, eph_pub) = crate::ratchet::keypair();
        let shared = dh(&eph_sec, &offer.eph_pub);
        // We are the responder: initiator = offer.from, responder = me.
        let (init_tx, init_rx) = derive(&shared, &offer.pipe_id, &offer.from, &me, chosen.medium);
        let pipe = Pipe {
            tx_key: init_rx, // responder transmits on the initiator's rx key
            rx_key: init_tx,
            pipe_id: offer.pipe_id,
            tx_seq: 0,
            port,
        };
        let answer = Answer::Ok { pipe_id: offer.pipe_id, eph_pub, chosen };
        (answer.encode(), Some(pipe))
    }

    /// Initiator side: consume the responder's ANSWER, derive keys, and open our
    /// end over `port` (built for the answer's chosen medium). `None` if the answer
    /// was a reject, was for a different pipe, or was malformed.
    pub fn finish(pending: Pending, answer: &Answer, port: P) -> Option<Pipe<P>> {
        let (eph_pub, chosen) = match answer {
            Answer::Ok { pipe_id, eph_pub, chosen } if *pipe_id == pending.pipe_id => (eph_pub, chosen),
            _ => return None,
        };
        let shared = dh(&pending.eph_sec, eph_pub);
        let (init_tx, init_rx) = derive(&shared, &pending.pipe_id, &pending.from, &pending.to, chosen.medium);
        Some(Pipe { tx_key: init_tx, rx_key: init_rx, pipe_id: pending.pipe_id, tx_seq: 0, port })
    }

    /// Seal `payload` as one record and hand it to the transport. The record's
    /// header (version, type, seq, pipe-id prefix) is authenticated as AAD, so a
    /// flipped type or seq fails the peer's MAC.
    pub fn send(&mut self, typ: RecordType, payload: &[u8]) -> std::io::Result<()> {
        let seq = self.tx_seq;
        self.tx_seq = self.tx_seq.wrapping_add(1);
        let mut header = Vec::with_capacity(8);
        header.push(VERSION);
        header.push(typ as u8);
        put_u16(&mut header, seq);
        header.extend_from_slice(&self.pipe_id[..4]);
        let ct = ChaCha20Poly1305::new(Key::from_slice(&self.tx_key))
            .encrypt(Nonce::from_slice(&nonce(seq)), Payload { msg: payload, aad: &header })
            .map_err(|_| std::io::Error::other("seal failed"))?;
        let mut frame = header;
        frame.extend_from_slice(&ct);
        self.port.send(&frame)
    }

    /// Drain one received record, or `None` if the transport has nothing (or the
    /// frame failed to authenticate — a forged or corrupt record is dropped, not
    /// surfaced). Returns the record type and its plaintext.
    pub fn poll(&mut self) -> Option<(RecordType, Vec<u8>)> {
        loop {
            let frame = self.port.try_recv()?;
            if let Some(rec) = self.open(&frame) {
                return Some(rec);
            }
            // Bad frame: skip it and try the next, rather than tearing the pipe
            // down on a single corrupt or spoofed datagram.
        }
    }

    fn open(&self, frame: &[u8]) -> Option<(RecordType, Vec<u8>)> {
        if frame.len() < 8 {
            return None;
        }
        let (header, ct) = frame.split_at(8);
        if header[0] != VERSION {
            return None;
        }
        let typ = RecordType::from_u8(header[1])?;
        if header[4..8] != self.pipe_id[..4] {
            return None; // not our pipe
        }
        let seq = u16::from_be_bytes([header[2], header[3]]);
        let pt = ChaCha20Poly1305::new(Key::from_slice(&self.rx_key))
            .decrypt(Nonce::from_slice(&nonce(seq)), Payload { msg: ct, aad: header })
            .ok()?;
        Some((typ, pt))
    }

    /// The negotiated pipe id — a stable handle for logging or app-level demux.
    pub fn pipe_id(&self) -> [u8; 16] {
        self.pipe_id
    }
    /// The underlying transport, e.g. to read its MTU.
    pub fn port(&self) -> &P {
        &self.port
    }
}

// ---- in-memory loopback transport (tests + two in-process peers) --------------

/// A pair of connected in-memory ports: whatever one `send`s the other `try_recv`s.
/// Not a network — a deterministic stand-in so the whole negotiate→seal→open path
/// is exercised in a unit test with no sockets, and so two peers can run in one
/// process. Use [`Loopback::pair`].
pub struct Loopback {
    tx: std::sync::mpsc::Sender<Vec<u8>>,
    rx: std::sync::mpsc::Receiver<Vec<u8>>,
    mtu: usize,
}

impl Loopback {
    /// Two ends of one link. A frame sent on the first is received on the second.
    pub fn pair(mtu: usize) -> (Loopback, Loopback) {
        let (a_tx, a_rx) = std::sync::mpsc::channel();
        let (b_tx, b_rx) = std::sync::mpsc::channel();
        (Loopback { tx: a_tx, rx: b_rx, mtu }, Loopback { tx: b_tx, rx: a_rx, mtu })
    }
}

impl DatagramPort for Loopback {
    fn mtu(&self) -> usize {
        self.mtu
    }
    fn send(&mut self, frame: &[u8]) -> std::io::Result<()> {
        self.tx.send(frame.to_vec()).map_err(|_| std::io::Error::other("loopback closed"))
    }
    fn try_recv(&mut self) -> Option<Vec<u8>> {
        self.rx.try_recv().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_offer() -> Offer {
        Offer {
            pipe_id: [7u8; 16],
            from: [1u8; 8],
            to: [2u8; 8],
            need: Need { min_bps: 5_000, mtu_needed: 64, max_latency_ms: Some(150) },
            eph_pub: [9u8; 32],
            candidates: vec![
                Candidate {
                    medium: Medium::Udp,
                    locator: b"10.0.0.1:7000".to_vec(),
                    est_bps: 1_000_000,
                    mtu: 1200,
                    rtt_hint_ms: 20,
                },
                Candidate {
                    medium: Medium::Ble,
                    locator: b"aa:bb".to_vec(),
                    est_bps: 100_000,
                    mtu: 200,
                    rtt_hint_ms: 40,
                },
            ],
        }
    }

    #[test]
    fn offer_round_trips_through_spdr() {
        let o = sample_offer();
        let back = Offer::decode(&o.encode()).expect("decode");
        assert_eq!(back.pipe_id, o.pipe_id);
        assert_eq!(back.from, o.from);
        assert_eq!(back.need.min_bps, o.need.min_bps);
        assert_eq!(back.need.max_latency_ms, Some(150));
        assert_eq!(back.candidates, o.candidates);
    }

    #[test]
    fn answer_round_trips_both_variants() {
        let ok = Answer::Ok {
            pipe_id: [3u8; 16],
            eph_pub: [4u8; 32],
            chosen: Candidate {
                medium: Medium::Tcp,
                locator: b"host:9".to_vec(),
                est_bps: 9,
                mtu: 1400,
                rtt_hint_ms: 5,
            },
        };
        match Answer::decode(&ok.encode()).unwrap() {
            Answer::Ok { chosen, .. } => assert_eq!(chosen.medium, Medium::Tcp),
            _ => panic!("expected ok"),
        }
        let rej = Answer::Reject { pipe_id: [3u8; 16], reason: Reject::Busy };
        match Answer::decode(&rej.encode()).unwrap() {
            Answer::Reject { reason, .. } => assert_eq!(reason, Reject::Busy),
            _ => panic!("expected reject"),
        }
    }

    #[test]
    fn a_truncated_or_alien_payload_decodes_to_none() {
        assert!(Offer::decode(b"nope").is_none());
        assert!(Offer::decode(b"SPDR\x01\x02short").is_none()); // wrong type byte
        let o = sample_offer().encode();
        assert!(Offer::decode(&o[..o.len() - 3]).is_none(), "truncated tail rejected");
    }

    #[test]
    fn choose_filters_by_throughput_and_mtu_then_ranks_by_latency() {
        let o = sample_offer();
        // Willing on both; UDP meets 5kbps/64B and has the lower rtt -> chosen.
        let c = choose(&o, &[Medium::Udp, Medium::Ble]).unwrap();
        assert_eq!(c.medium, Medium::Udp);

        // Only BLE on offer meets a tiny need, so it wins when UDP isn't willing.
        let c = choose(&o, &[Medium::Ble]).unwrap();
        assert_eq!(c.medium, Medium::Ble);

        // No overlap at all -> NoMedium.
        assert_eq!(choose(&o, &[Medium::EspNow]), Err(Reject::NoMedium));

        // Overlap exists but the need is too big for any -> Throughput.
        let mut greedy = o.clone();
        greedy.need.mtu_needed = 5000;
        assert_eq!(choose(&greedy, &[Medium::Udp, Medium::Ble]), Err(Reject::Throughput));
    }

    #[test]
    fn negotiated_pipe_carries_data_both_ways() {
        let (init_port, resp_port) = Loopback::pair(1200);
        let init_addr = [0xA1u8; 8];
        let resp_addr = [0xB2u8; 8];

        let (offer_bytes, pending) = Pipe::<Loopback>::offer(
            init_addr,
            resp_addr,
            [42u8; 16],
            Need { min_bps: 5_000, mtu_needed: 64, max_latency_ms: None },
            vec![Candidate {
                medium: Medium::Udp,
                locator: b"127.0.0.1:0".to_vec(),
                est_bps: 1_000_000,
                mtu: 1200,
                rtt_hint_ms: 10,
            }],
        );

        let offer = Offer::decode(&offer_bytes).unwrap();
        let (answer_bytes, resp_pipe) = Pipe::answer(&offer, resp_addr, &[Medium::Udp], resp_port);
        let mut resp_pipe = resp_pipe.unwrap();
        let answer = Answer::decode(&answer_bytes).unwrap();
        let mut init_pipe = Pipe::finish(pending, &answer, init_port).unwrap();

        init_pipe.send(RecordType::Data, b"north pier at midnight").unwrap();
        assert_eq!(resp_pipe.poll().unwrap(), (RecordType::Data, b"north pier at midnight".to_vec()));

        resp_pipe.send(RecordType::Media, b"copy that").unwrap();
        assert_eq!(init_pipe.poll().unwrap(), (RecordType::Media, b"copy that".to_vec()));
    }

    // A whole negotiation over a fresh loopback, returning both live ends.
    fn negotiate(from: Addr, to: Addr) -> (Pipe<Loopback>, Pipe<Loopback>) {
        let (ip, rp) = Loopback::pair(1200);
        let (ob, pending) = Pipe::<Loopback>::offer(
            from,
            to,
            [1u8; 16], // same pipe id on purpose: keys must still differ by addr/eph
            Need { min_bps: 1, mtu_needed: 1, max_latency_ms: None },
            vec![Candidate { medium: Medium::Udp, locator: vec![], est_bps: 10, mtu: 100, rtt_hint_ms: 1 }],
        );
        let offer = Offer::decode(&ob).unwrap();
        let (ans, resp) = Pipe::answer(&offer, to, &[Medium::Udp], rp);
        let init = Pipe::finish(pending, &Answer::decode(&ans).unwrap(), ip).unwrap();
        (init, resp.unwrap())
    }

    #[test]
    fn a_record_only_opens_on_the_pipe_that_negotiated_it() {
        // The record's pipe-id prefix matches both pipes (same id), so this exercises
        // the *key* binding — addresses + ephemeral DH — not just the id demux.
        let (mut a_init, mut a_resp) = negotiate([1u8; 8], [2u8; 8]);
        let (_b_init, b_resp) = negotiate([3u8; 8], [4u8; 8]);

        a_init.send(RecordType::Data, b"north pier").unwrap();
        let frame = a_resp.port.try_recv().unwrap();
        assert!(a_resp.open(&frame).is_some(), "the true peer opens it");
        assert!(b_resp.open(&frame).is_none(), "an unrelated pipe's keys cannot open it");
    }

    #[test]
    fn an_answer_for_a_different_pipe_id_does_not_finish() {
        let (_ip, rp) = Loopback::pair(1200);
        let (_ob, pending) = Pipe::<Loopback>::offer(
            [1u8; 8],
            [2u8; 8],
            [0xAAu8; 16],
            Need { min_bps: 1, mtu_needed: 1, max_latency_ms: None },
            vec![Candidate { medium: Medium::Udp, locator: vec![], est_bps: 10, mtu: 100, rtt_hint_ms: 1 }],
        );
        // An answer that names a *different* pipe id must be refused, not finished.
        let wrong = Answer::Ok {
            pipe_id: [0xBBu8; 16],
            eph_pub: [1u8; 32],
            chosen: Candidate { medium: Medium::Udp, locator: vec![], est_bps: 10, mtu: 100, rtt_hint_ms: 1 },
        };
        assert!(Pipe::finish(pending, &wrong, rp).is_none());
    }

    #[test]
    fn a_flipped_header_byte_fails_the_mac() {
        let (ip, rp) = Loopback::pair(1200);
        let (ob, pending) = Pipe::<Loopback>::offer(
            [1u8; 8],
            [2u8; 8],
            [5u8; 16],
            Need { min_bps: 1, mtu_needed: 1, max_latency_ms: None },
            vec![Candidate { medium: Medium::Udp, locator: vec![], est_bps: 10, mtu: 100, rtt_hint_ms: 1 }],
        );
        let offer = Offer::decode(&ob).unwrap();
        let (ans, resp) = Pipe::answer(&offer, [2u8; 8], &[Medium::Udp], rp);
        let mut resp = resp.unwrap();
        let mut init = Pipe::finish(pending, &Answer::decode(&ans).unwrap(), ip).unwrap();

        // Seal a record through the real pipe, capture the frame off the link, and
        // flip its authenticated type byte before opening it.
        init.send(RecordType::Data, b"hi").unwrap();
        let mut frame = resp.port.try_recv().unwrap();
        let clean = resp.open(&frame);
        assert!(clean.is_some(), "the untampered frame opens");
        frame[1] ^= 0x01; // flip the type byte, which is authenticated as AAD
        assert!(resp.open(&frame).is_none(), "AAD tamper must fail the MAC");
    }
}
