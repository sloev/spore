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
use rand::rngs::OsRng;
use rand::RngCore;
use std::collections::HashMap;
use x25519_dalek::{PublicKey, StaticSecret};

/// Wire magic so another app ignores an SPDR payload it doesn't understand.
pub const MAGIC: &[u8; 4] = b"SPDR";
/// Profile version. Bumping it changes the signalling and key schedule; a peer on
/// a different version is rejected rather than mis-negotiated.
///
/// **2** — a candidate's medium is a length-prefixed name, not a byte code (and
/// the KDF binds the name). Bumped rather than finessed: v1 spelled the same
/// field differently, so a v1 peer would mis-parse every candidate. Cheap to do
/// now and not later — until #95 nothing could start a pipe at all, so there is
/// no deployed v1 signalling to strand.
pub const VERSION: u8 = 2;

/// A transport-capable medium, named by **convention rather than by code**.
///
/// This was a closed `#[repr(u8)]` enum, which was the wrong shape twice over.
/// `DESIGN.md`'s model already says the nutrient list is closed while the
/// *bridge* list stays open — a medium is the Direct plane's version of a bridge,
/// so enumerating them in the core made every new one an edit to `src/`, and an
/// allocation somebody had to hand out. A name needs neither: run SPORE Direct
/// over a medium nobody here has heard of and the core does not have to know.
///
/// It also removes a real failure mode. An unknown *code* had to be a decode
/// error; an unknown *name* is simply a medium this node is not willing to use,
/// so it is skipped like any other unusable candidate rather than poisoning the
/// offer it arrived in.
///
/// `est_bps`/`mtu` on a candidate describe the link; the name only says which
/// family it is, so both ends agree on what they chose and the KDF can bind it.
///
/// The conventional names are listed as associated constants and in
/// `docs/DIRECT.md`. Nothing enforces them — that is the point — but a medium
/// two implementations spell differently is two mediums, so stick to the list
/// where one exists, and namespace anything new (`acme.lora-p2p`).
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Medium(String);

impl Medium {
    /// One datagram carries exactly one record.
    pub const UDP: &'static str = "udp";
    /// Byte stream; records get a length prefix.
    pub const TCP: &'static str = "tcp";
    /// Bluetooth Low Energy.
    pub const BLE: &'static str = "ble";
    /// ESP-NOW, the connectionless ESP32 link.
    pub const ESP_NOW: &'static str = "esp-now";

    pub fn new(name: impl Into<String>) -> Medium {
        Medium(name.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The conventional UDP medium — the one every runtime here can open.
    pub fn udp() -> Medium {
        Medium::new(Medium::UDP)
    }

    /// The conventional TCP medium.
    pub fn tcp() -> Medium {
        Medium::new(Medium::TCP)
    }
}

impl std::fmt::Display for Medium {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl PartialEq<str> for Medium {
    fn eq(&self, other: &str) -> bool {
        self.0 == other
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
            put_bytes(&mut v, c.medium.as_str().as_bytes());
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
            let medium = Medium::new(String::from_utf8(r.bytes()?).ok()?);
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
                put_bytes(&mut v, chosen.medium.as_str().as_bytes());
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
                let medium = Medium::new(String::from_utf8(r.bytes()?).ok()?);
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

/// The responder's decision, taken *before* any port exists.
///
/// Deciding which medium wins is the core's job; opening a socket for it is the
/// runtime's. [`accept`] and [`Pipe::answer_with`] are the two halves, and this
/// is what is held across the gap between them.
///
/// [`Pipe::answer`] fuses the two, which is only usable when the responder is
/// willing to use exactly one medium — it takes the port before it knows which
/// candidate won, so a node offering several would have to guess. Anything
/// choosing among mediums wants this pair instead.
pub struct Accepted {
    /// The candidate that won. The runtime opens a port for *this* medium.
    pub chosen: Candidate,
    pipe_id: [u8; 16],
    initiator: Addr,
    offer_eph: [u8; 32],
}

impl Accepted {
    /// The pipe this decision belongs to.
    pub fn pipe_id(&self) -> [u8; 16] {
        self.pipe_id
    }
}

/// Build an OFFER. Free-standing because nothing about proposing a pipe needs a
/// port — [`Pipe::offer`]'s type parameter was only ever incidental, and
/// [`Signalling`] would otherwise have to name an arbitrary transport to call it.
fn build_offer(
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

/// Responder side, step 1: choose a candidate without opening anything.
///
/// `Err` is the encoded reject ANSWER to send back — there is nothing to open in
/// that case, which is exactly why the reject path does not need a port either.
pub fn accept(offer: &Offer, willing: &[Medium]) -> Result<Accepted, Vec<u8>> {
    match choose(offer, willing) {
        Ok(c) => Ok(Accepted {
            chosen: c.clone(),
            pipe_id: offer.pipe_id,
            initiator: offer.from,
            offer_eph: offer.eph_pub,
        }),
        Err(reason) => Err(Answer::Reject { pipe_id: offer.pipe_id, reason }.encode()),
    }
}

/// True if `payload` is SPDR signalling rather than ordinary application bytes.
///
/// A node receives OFFER/ANSWER as the plaintext of an ordinary sealed DM, so
/// something has to tell the two apart before the app sees a message that is not
/// for it. The magic is checked, not guessed at — an app payload that happens to
/// start with these four bytes still has to carry a version and a known type.
pub fn is_signal(payload: &[u8]) -> bool {
    payload.len() >= 6
        && &payload[..4] == MAGIC
        && payload[4] == VERSION
        && matches!(payload[5], T_OFFER | T_ANSWER)
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
    medium: &Medium,
) -> ([u8; 32], [u8; 32]) {
    let mut h = Blake2bVar::new(64).unwrap();
    h.update(shared);
    h.update(b"spore-direct-v1");
    h.update(pipe_id);
    h.update(initiator);
    h.update(responder);
    h.update(medium.as_str().as_bytes());
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

// Real socket adapters. Both use `std::net`, which does not exist on `wasm32`, so
// they are gated off that target — the negotiation core above still compiles there.
#[cfg(not(target_arch = "wasm32"))]
mod udp;
#[cfg(not(target_arch = "wasm32"))]
pub use udp::UdpPort;
#[cfg(not(target_arch = "wasm32"))]
mod tcp;
#[cfg(not(target_arch = "wasm32"))]
pub use tcp::TcpPort;

// P-Direct-NAT step 2: reflexive locators, and the echo that answers them.
#[cfg(not(target_arch = "wasm32"))]
pub mod stun;

// The UDP runner both native runtimes drive. Shared so the daemon and Android
// cannot drift into two different negotiations of the same protocol.
#[cfg(not(target_arch = "wasm32"))]
pub mod runner;
#[cfg(not(target_arch = "wasm32"))]
pub use runner::{Event, Outbound, UdpRunner};

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
        build_offer(from, to, pipe_id, need, candidates)
    }

    /// Responder side: given an OFFER and the mediums we're willing to use, pick a
    /// candidate, derive keys, and open our end over `port` (built for the chosen
    /// medium). Returns the ANSWER bytes to send back and our live [`Pipe`]. On no
    /// fit, returns the ANSWER bytes carrying the reject and no pipe.
    /// Convenience for a responder willing to use exactly **one** medium: decide
    /// and open in a single call.
    ///
    /// `port` is taken before the choice is made, so it has to already be the
    /// right kind of port. A responder offering more than one medium cannot know
    /// that in advance — it wants [`accept`] then [`Pipe::answer_with`], which
    /// put the runtime's socket work in the gap between the two.
    pub fn answer(offer: &Offer, me: Addr, willing: &[Medium], port: P) -> (Vec<u8>, Option<Pipe<P>>) {
        match accept(offer, willing) {
            Ok(acc) => {
                let (bytes, pipe) = Self::answer_with(acc, me, port);
                (bytes, Some(pipe))
            }
            Err(reject) => (reject, None),
        }
    }

    /// Responder side, step 2: derive keys over the port the runtime opened for
    /// [`Accepted::chosen`]. Returns the ANSWER bytes to carry back over the mesh
    /// and the live pipe.
    ///
    /// Infallible by construction: the choosing already happened in [`accept`], so
    /// there is no "no medium fits" outcome left to represent here.
    pub fn answer_with(acc: Accepted, me: Addr, port: P) -> (Vec<u8>, Pipe<P>) {
        let (eph_sec, eph_pub) = crate::ratchet::keypair();
        let shared = dh(&eph_sec, &acc.offer_eph);
        // We are the responder: initiator = the offer's sender, responder = me.
        let (init_tx, init_rx) = derive(&shared, &acc.pipe_id, &acc.initiator, &me, &acc.chosen.medium);
        let pipe = Pipe {
            tx_key: init_rx, // responder transmits on the initiator's rx key
            rx_key: init_tx,
            pipe_id: acc.pipe_id,
            tx_seq: 0,
            port,
        };
        let answer = Answer::Ok { pipe_id: acc.pipe_id, eph_pub, chosen: acc.chosen };
        (answer.encode(), pipe)
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
        let (init_tx, init_rx) =
            derive(&shared, &pending.pipe_id, &pending.from, &pending.to, &chosen.medium);
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

// ---- mesh signalling ----------------------------------------------------------

/// What an inbound SPDR payload means, and what the runtime has to do about it.
///
/// Every variant that needs a socket names the medium to open and hands back the
/// state to finish with. The core decides; the runtime opens. Nothing here holds
/// a port, and nothing here sees a byte that flows over one.
pub enum Signal {
    /// An OFFER this node can serve. Open a port for `accepted.chosen`, call
    /// [`Pipe::answer_with`], and carry the ANSWER bytes it returns back to
    /// `peer` over the mesh.
    Offer { peer: Addr, accepted: Accepted },
    /// An OFFER this node cannot serve. Carry `reply` back to `peer`. Nothing is
    /// opened and nothing is left half-built — the reject path needs no port,
    /// which is the whole reason choosing happens before opening.
    Decline { peer: Addr, reply: Vec<u8> },
    /// The ANSWER to an offer this node made. Open a port for `chosen`, then
    /// [`Pipe::finish`] with `pending` and `answer`.
    Answered { peer: Addr, pending: Pending, answer: Answer, chosen: Candidate },
    /// The peer refused, and why. Nothing to open.
    Refused { peer: Addr, pipe_id: [u8; 16], reason: Reject },
    /// Not SPDR, not addressed here, or an ANSWER for a pipe this node never
    /// offered. The payload is ordinary application bytes; hand it to the app.
    NotSignal,
}

/// The half-open Direct negotiations this node is party to.
///
/// This is the piece that was missing: `direct` could already negotiate and the
/// adapters could already carry bytes, but nothing tied SPDR to
/// [`Node::send_direct`](crate::Node::send_direct), so no app could start a pipe
/// at all — which is why NAT traversal had never actually been hit in practice.
///
/// Pure, like the rest of this module: no sockets and no clock of its own. Time
/// arrives as a parameter; ports are the runtime's to open.
pub struct Signalling {
    me: Addr,
    pending: HashMap<[u8; 16], (Pending, u32)>,
}

impl Signalling {
    pub fn new(me: Addr) -> Signalling {
        Signalling { me, pending: HashMap::new() }
    }

    /// Start a negotiation. Returns the SPDR bytes to carry to `to` over
    /// `Node::send_direct`, and the id of the pipe they propose.
    pub fn offer(
        &mut self,
        to: Addr,
        need: Need,
        candidates: Vec<Candidate>,
        now: u32,
    ) -> (Vec<u8>, [u8; 16]) {
        let mut pipe_id = [0u8; 16];
        OsRng.fill_bytes(&mut pipe_id);
        let (bytes, pending) = build_offer(self.me, to, pipe_id, need, candidates);
        self.pending.insert(pipe_id, (pending, now));
        (bytes, pipe_id)
    }

    /// Interpret the plaintext of a delivered DM from `peer`.
    ///
    /// `willing` is this runtime's declared set — the mediums it can actually
    /// open, not the ones it would like to. A runtime that can open nothing
    /// passes an empty slice and every offer is declined honestly, rather than
    /// accepted and then quietly failing to connect.
    pub fn on_signal(&mut self, peer: Addr, payload: &[u8], willing: &[Medium]) -> Signal {
        if !is_signal(payload) {
            return Signal::NotSignal;
        }
        if let Some(offer) = Offer::decode(payload) {
            // An offer addressed to someone else is not ours to answer, even if
            // the mesh happened to hand it to us.
            if offer.to != self.me {
                return Signal::NotSignal;
            }
            return match accept(&offer, willing) {
                Ok(accepted) => Signal::Offer { peer, accepted },
                Err(reply) => Signal::Decline { peer, reply },
            };
        }
        if let Some(answer) = Answer::decode(payload) {
            let pipe_id = match &answer {
                Answer::Ok { pipe_id, .. } | Answer::Reject { pipe_id, .. } => *pipe_id,
            };
            // An answer for a pipe we never offered is dropped: without our own
            // ephemeral secret there is nothing to derive, so nothing to act on.
            let Some((pending, _)) = self.pending.remove(&pipe_id) else {
                return Signal::NotSignal;
            };
            return match answer {
                Answer::Ok { ref chosen, .. } => {
                    let chosen = chosen.clone();
                    Signal::Answered { peer, pending, answer, chosen }
                }
                Answer::Reject { reason, .. } => Signal::Refused { peer, pipe_id, reason },
            };
        }
        Signal::NotSignal
    }

    /// Forget offers that were never answered. Returns how many were dropped.
    ///
    /// Unbounded without this: a daemon that offers a pipe to an unreachable peer
    /// holds that ephemeral secret forever. `ttl` wants to be short — a NAT
    /// binding dies in 30s–5min, so the candidates in an older offer are stale
    /// anyway and the negotiation is better restarted than resumed.
    pub fn expire(&mut self, now: u32, ttl: u32) -> usize {
        let before = self.pending.len();
        self.pending.retain(|_, (_, made)| now.saturating_sub(*made) < ttl);
        before - self.pending.len()
    }

    /// How many offers are waiting for an answer.
    pub fn pending(&self) -> usize {
        self.pending.len()
    }

    /// This node's address — the one an ANSWER is derived against.
    pub fn me(&self) -> Addr {
        self.me
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
                    medium: Medium::udp(),
                    locator: b"10.0.0.1:7000".to_vec(),
                    est_bps: 1_000_000,
                    mtu: 1200,
                    rtt_hint_ms: 20,
                },
                Candidate {
                    medium: Medium::new(Medium::BLE),
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
                medium: Medium::tcp(),
                locator: b"host:9".to_vec(),
                est_bps: 9,
                mtu: 1400,
                rtt_hint_ms: 5,
            },
        };
        match Answer::decode(&ok.encode()).unwrap() {
            Answer::Ok { chosen, .. } => assert_eq!(chosen.medium, Medium::tcp()),
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

    /// The behaviour the byte-code enum could not have: a medium this build has
    /// never heard of is *skipped*, not fatal.
    ///
    /// Under the old `Medium::from_u8`, an unrecognised code propagated `None`
    /// out of `Offer::decode` and took the whole offer with it — so a peer
    /// advertising one new path alongside three usable ones got nothing at all,
    /// and adding any medium anywhere broke every older build.
    #[test]
    fn an_unknown_medium_is_ignored_rather_than_poisoning_the_offer() {
        let mut o = sample_offer();
        o.candidates.insert(
            0,
            Candidate {
                medium: Medium::new("acme.lora-p2p"),
                locator: b"whatever-this-means".to_vec(),
                est_bps: 9_000_000,
                mtu: 1500,
                rtt_hint_ms: 1, // the best-looking candidate, so ranking would pick it
                                // if it were ever considered
            },
        );

        // It survives a round trip: the codec carries a name it cannot interpret.
        let decoded = Offer::decode(&o.encode()).expect("an unknown medium must still decode");
        assert_eq!(decoded.candidates.len(), o.candidates.len(), "no candidate is dropped on the wire");
        assert_eq!(decoded.candidates[0].medium, Medium::new("acme.lora-p2p"));

        // And it is simply not chosen, because nobody declared willingness for it.
        let c = choose(&decoded, &[Medium::udp()]).expect("the usable candidate still wins");
        assert_eq!(c.medium, Medium::udp(), "an unknown medium never outranks one we can open");

        // An offer of *only* unknown mediums is an honest NoMedium, not a parse
        // failure — the peer hears a reason instead of silence.
        let only_alien = Offer { candidates: vec![decoded.candidates[0].clone()], ..decoded.clone() };
        let round_tripped = Offer::decode(&only_alien.encode()).expect("still decodes");
        assert_eq!(choose(&round_tripped, &[Medium::udp()]), Err(Reject::NoMedium));
    }

    #[test]
    fn a_medium_is_a_name_and_the_key_schedule_binds_it() {
        // Names are conventions, not codes: anything round trips.
        for name in ["udp", "esp-now", "acme.lora-p2p", ""] {
            let mut o = sample_offer();
            o.candidates[0].medium = Medium::new(name);
            let d = Offer::decode(&o.encode()).expect("any name decodes");
            assert_eq!(d.candidates[0].medium, Medium::new(name));
        }

        // Two mediums spelled differently are two mediums, and the KDF says so:
        // the same shared secret and pipe under a different medium name derives
        // different keys, so a record cannot be replayed onto another medium.
        let shared = [7u8; 32];
        let (a, _) = derive(&shared, &[1u8; 16], &[2u8; 8], &[3u8; 8], &Medium::udp());
        let (b, _) = derive(&shared, &[1u8; 16], &[2u8; 8], &[3u8; 8], &Medium::tcp());
        assert_ne!(a, b, "the medium name is bound into the key schedule");
    }

    #[test]
    fn choose_filters_by_throughput_and_mtu_then_ranks_by_latency() {
        let o = sample_offer();
        // Willing on both; UDP meets 5kbps/64B and has the lower rtt -> chosen.
        let c = choose(&o, &[Medium::udp(), Medium::new(Medium::BLE)]).unwrap();
        assert_eq!(c.medium, Medium::udp());

        // Only BLE on offer meets a tiny need, so it wins when UDP isn't willing.
        let c = choose(&o, &[Medium::new(Medium::BLE)]).unwrap();
        assert_eq!(c.medium, Medium::new(Medium::BLE));

        // No overlap at all -> NoMedium.
        assert_eq!(choose(&o, &[Medium::new(Medium::ESP_NOW)]), Err(Reject::NoMedium));

        // Overlap exists but the need is too big for any -> Throughput.
        let mut greedy = o.clone();
        greedy.need.mtu_needed = 5000;
        assert_eq!(choose(&greedy, &[Medium::udp(), Medium::new(Medium::BLE)]), Err(Reject::Throughput));
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
                medium: Medium::udp(),
                locator: b"127.0.0.1:0".to_vec(),
                est_bps: 1_000_000,
                mtu: 1200,
                rtt_hint_ms: 10,
            }],
        );

        let offer = Offer::decode(&offer_bytes).unwrap();
        let (answer_bytes, resp_pipe) = Pipe::answer(&offer, resp_addr, &[Medium::udp()], resp_port);
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
            vec![Candidate { medium: Medium::udp(), locator: vec![], est_bps: 10, mtu: 100, rtt_hint_ms: 1 }],
        );
        let offer = Offer::decode(&ob).unwrap();
        let (ans, resp) = Pipe::answer(&offer, to, &[Medium::udp()], rp);
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
            vec![Candidate { medium: Medium::udp(), locator: vec![], est_bps: 10, mtu: 100, rtt_hint_ms: 1 }],
        );
        // An answer that names a *different* pipe id must be refused, not finished.
        let wrong = Answer::Ok {
            pipe_id: [0xBBu8; 16],
            eph_pub: [1u8; 32],
            chosen: Candidate {
                medium: Medium::udp(),
                locator: vec![],
                est_bps: 10,
                mtu: 100,
                rtt_hint_ms: 1,
            },
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
            vec![Candidate { medium: Medium::udp(), locator: vec![], est_bps: 10, mtu: 100, rtt_hint_ms: 1 }],
        );
        let offer = Offer::decode(&ob).unwrap();
        let (ans, resp) = Pipe::answer(&offer, [2u8; 8], &[Medium::udp()], rp);
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
