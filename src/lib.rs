//! SPORE v1 — Store-and-forward Planetary Opportunistic Relay Envelope.
//!
//! One portable core. Pure-Rust crypto (no libsodium/C), so this compiles
//! unchanged to native targets and to `wasm32-unknown-unknown` for the browser.
//! Transports are plugins that hand raw bytes to `Node::on_rx` and execute the
//! `Forward`s it returns; the router itself never changes across media.
//!
//! Section numbers below map to the one-page spec.

#![allow(clippy::needless_range_loop)]

use std::collections::{HashMap, HashSet};

use blake2::digest::{Update as _, VariableOutput};
use blake2::Blake2bVar;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rand::rngs::OsRng;
use rand::RngCore;
use sha2::{Digest, Sha256};

// ---------------------------------------------------------------------------
// §1 Identity & addressing
// ---------------------------------------------------------------------------

pub type Id = [u8; 16]; // first 16 bytes of SHA-256(envelope, hops zeroed)
pub type Addr = [u8; 8]; // first 8 bytes of SHA-256(pubkey) or SHA-256(topic)
pub type Iface = u16; // transport-assigned interface id
pub const NO_IFACE: Iface = u16::MAX;
pub const ZERO_DEST: Addr = [0u8; 8]; // public flood

pub const SEEN_MIN_SECS: u32 = 30 * 24 * 3600;
pub const PATH_FRESH_SECS: u32 = 3 * 3600;

/// How often a node mints a fresh prekey (§7). One day.
pub const PREKEY_PERIOD_SECS: u32 = 24 * 3600;

/// How long a prekey *secret* is kept after it was minted (§7). Seven days.
///
/// This is the seizure window, and it is only real because the secret is random
/// rather than derived from the identity seed: once swept it cannot be recomputed
/// from anything a restore has. Mail sealed to a prekey older than this is
/// permanently unreadable — deliberately. See S-022.
pub const PREKEY_LIFETIME_SECS: u32 = 7 * 24 * 3600;

/// Hard ceiling on ring entries, so a clock jump cannot grow it without bound.
pub const MAX_PREKEY_RING: usize = 16;

/// One entry in the prekey ring: an X25519 keypair and when it was minted.
///
/// Both halves are stored because [`seal_nonce`] mixes the *recipient's* public
/// key, so a secret can only be tried against the public half it belongs to.
struct Prekey {
    public: [u8; 32],
    secret: [u8; 32],
    /// Unix seconds when minted; `0` means "seed-derived bootstrap, age unknown".
    born: u32,
}

impl Prekey {
    fn from_secret(secret: [u8; 32], born: u32) -> Self {
        let public = *crypto_box::SecretKey::from(secret).public_key().as_bytes();
        Prekey { public, secret, born }
    }

    fn fresh(born: u32) -> Self {
        let mut s = [0u8; 32];
        OsRng.fill_bytes(&mut s);
        Prekey::from_secret(s, born)
    }
}

/// Trickle bounds for the link-local HELLO beacon (§5.4b), **in seconds**.
///
/// The spec says the interval doubles 5 → 80 *minutes*. The daemon used to pass
/// the bare numbers 5 and 80 into a timer whose base is `now()` in seconds, so it
/// beaconed 60× too fast — and beaconed the mesh-wide flood rather than the
/// link-local HELLO, giving ~45 floods an hour against a documented ceiling of one
/// (S-023). Named constants in the timer's own unit, so the mistake cannot recur
/// silently.
pub const HELLO_MIN_SECS: u32 = 5 * 60;
pub const HELLO_MAX_SECS: u32 = 80 * 60;

/// Floor between mesh-wide flooded ANNOUNCEs — the spec's "ANNOUNCE flood ≤ 1/h".
pub const ANNOUNCE_FLOOD_MIN_SECS: u32 = 3600;

/// Default per-envelope wire budget used by `Node::send` to decide when to
/// fragment. Transports over tighter media (LoRa ~200 B, ESP-NOW 250 B) lower
/// `Node::mtu`; the router itself is MTU-agnostic.
pub const DEFAULT_MTU: usize = 1400;
/// Default per-source flood quota (§10): the sustained bytes/second any one
/// originating address may have a node store and relay. Generous enough that
/// legitimate traffic never notices; a safety valve against amplification abuse.
/// Stamped (proof-of-work) mail bypasses it. Tune with `Node::set_source_quota`.
pub const DEFAULT_SOURCE_QUOTA: u32 = 1024 * 1024;

/// Most ids honoured from a single INV or WANT.
///
/// The payload length is a `u16`, so one packet can list ~4095 ids, and `on_want`
/// answers each one with a *whole stored envelope*. That is reflection with gain:
/// measured at **32x** with 400-byte payloads, and the ceiling is the largest
/// envelope over 16 bytes — around 4000x. INV/WANT are also consumed before dedup
/// and before the quota (§6: per-link, hops=0, never stored, never relayed), so an
/// attacker can replay one identical request forever, and on a radio link every
/// reply spends airtime that §10 caps at 10% by law.
///
/// Capping the ids honoured bounds the work one packet can buy. The requester
/// loses nothing real: INV/WANT are gossip, so an id not served now is offered
/// again on the next round.
pub const MAX_IDS_PER_GOSSIP: usize = 64;

/// Bytes per second of stored envelopes one interface may pull out of us with
/// WANT.
///
/// The per-id cap above bounds a single packet; this bounds a *stream* of them,
/// which is the other half of the same problem. Per-interface rather than
/// per-source, because a WANT carries no identity worth having — it is unsigned
/// per-link gossip — so the link it arrived on is the only thing we can attribute
/// it to.
pub const DEFAULT_GOSSIP_BUDGET: u32 = 32 * 1024;

// --- Bounds on state a peer can grow ---------------------------------------
//
// Every table below is keyed or filled by something that arrives from outside,
// and none of them were bounded: measured, 20,000 incomplete fountain sets and
// 20,000 dedup entries from unsigned traffic the quota admits unconditionally,
// plus 3,000 peer records from minted identities — none collected 10 million
// seconds later. Signature checks are not a bound here, because an Ed25519
// keypair is nearly free (the same lesson as `MAX_NEIGHBOURS`).
//
// The numbers are chosen to be generous for real meshes and finite for hostile
// ones. Every one of them degrades a capability rather than breaking it: a
// forgotten peer re-announces, a dropped partial object is re-fetched, an evicted
// dedup entry costs one duplicate relay.

/// Dedup entries retained (§5). Evicts nearest-to-expiry first, so the ids most
/// likely to still be in flight are the ones kept.
pub const MAX_SEEN: usize = 1 << 16;
/// Incomplete fountain sets held at once. Each holds real chunk bytes, so this is
/// the tightest of these bounds.
pub const MAX_PARTIAL_OBJECTS: usize = 256;
/// How long an incomplete fountain set is kept before it is collected. Fragments
/// of a live object keep arriving; a set that has heard nothing for this long is
/// either abandoned or was never real.
pub const PARTIAL_TIMEOUT_SECS: u32 = 300;
/// Peers we retain prekeys, names and busy bytes for.
pub const MAX_PEERS: usize = 4096;
/// File manifests retained.
pub const MAX_MANIFESTS: usize = 1024;
/// Receipt ids retained (§8).
pub const MAX_ACKED: usize = 8192;
/// Undrained inbound RPC requests / feed events. The application is expected to
/// drain these; the cap is what happens when it does not, and dropping the oldest
/// is better than growing until the process dies.
pub const MAX_INBOX: usize = 1024;
/// How often the time-based sweep runs. Hard caps apply on every ingest; this is
/// only for expiry, which does not need to be immediate.
const SWEEP_INTERVAL_SECS: u32 = 60;

/// An object too large to carry as one fountain set — returned by
/// [`Node::send`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TooLarge {
    /// Chunks the object would need at the MTU in force.
    pub needed: usize,
    /// Bytes per chunk at that MTU (`mtu - FRAG_OVERHEAD`).
    pub chunk: usize,
}

impl core::fmt::Display for TooLarge {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "object needs {} chunks of {} B but one fountain set holds {}; \
             use the file/manifest layer for objects this large",
            self.needed, self.chunk, MAX_FOUNTAIN_CHUNKS
        )
    }
}
impl std::error::Error for TooLarge {}

/// Datagrams are ephemeral: a short expiry keeps interactive session traffic
/// out of anyone's long-term store.
pub const SESSION_EXPIRY_SECS: u32 = 300;

/// First payload byte of a delivery receipt: `[0x06][orig_id:16]` (§8).
pub const RECEIPT_TAG: u8 = 0x06;

/// address = SHA-256(pubkey)[..8]
pub fn addr_of(pubkey: &[u8; 32]) -> Addr {
    let d = Sha256::digest(pubkey);
    let mut a = [0u8; 8];
    a.copy_from_slice(&d[..8]);
    a
}
/// topic hash = SHA-256(utf8)[..8]
pub fn topic_of(s: &str) -> Addr {
    let d = Sha256::digest(s.as_bytes());
    let mut a = [0u8; 8];
    a.copy_from_slice(&d[..8]);
    a
}

// ---------------------------------------------------------------------------
// §3 Fragmentation — rateless fountain code over GF(2)
// ---------------------------------------------------------------------------

/// Drop arbitrary entries until `map` holds at most `max`.
///
/// The victims are whichever the iterator yields first. That is unordered, and
/// deliberately not attacker-chosen: `std`'s hasher is seeded per map, so a peer
/// cannot arrange to be the one who survives. For these tables — prekeys, names,
/// busy bytes, manifests — any survivor set is equally correct, because a peer we
/// forgot re-announces and a manifest we forgot is re-fetched.
fn trim_map<K: Copy + Eq + std::hash::Hash, V>(map: &mut HashMap<K, V>, max: usize) {
    if map.len() <= max {
        return;
    }
    let excess = map.len() - max;
    let victims: Vec<K> = map.keys().take(excess).copied().collect();
    for k in victims {
        map.remove(&k);
    }
}

/// [`trim_map`] for a set.
fn trim_set<K: Copy + Eq + std::hash::Hash>(set: &mut HashSet<K>, max: usize) {
    if set.len() <= max {
        return;
    }
    let excess = set.len() - max;
    let victims: Vec<K> = set.iter().take(excess).copied().collect();
    for k in victims {
        set.remove(&k);
    }
}

// ---------------------------------------------------------------------------
// §4 Routing state
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct Path {
    iface: Iface,
    nbr: Option<Addr>,
    age: u32,
}
#[derive(Default)]
pub struct Paths {
    map: HashMap<Addr, Vec<Path>>,
}
impl Paths {
    fn learn(&mut self, a: Addr, iface: Iface, nbr: Option<Addr>, now: u32) {
        let v = self.map.entry(a).or_default();
        v.retain(|p| !(p.iface == iface && p.nbr == nbr));
        v.insert(0, Path { iface, nbr, age: now });
        v.truncate(3); // keep up to 3 candidates
    }
    fn fresh(&self, a: &Addr, now: u32) -> Option<Path> {
        self.map.get(a)?.iter().find(|p| now.saturating_sub(p.age) < PATH_FRESH_SECS).cloned()
    }
}

// ---------------------------------------------------------------------------
// §5 Forwarding — what the transport must execute
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub enum Forward {
    /// Send on all interfaces except `except` (on shared media the transport
    /// applies CSMA: jitter 1–5× airtime, cancel if the ID is overheard).
    Flood { except: Iface, bytes: Vec<u8> },
    /// Send only toward this neighbor/interface (learned path).
    Directed { iface: Iface, nbr: Option<Addr>, bytes: Vec<u8> },
}

#[derive(Default)]
pub struct Rx {
    pub delivered: Vec<Envelope>, // to the local app
    pub forwards: Vec<Forward>,   // to the transport
}

// ---------------------------------------------------------------------------
// The node: §5 router + §6 sync, glued together. Transports call `on_rx`.
// ---------------------------------------------------------------------------

pub struct Node {
    pub sk: SigningKey,
    pub addr: Addr,
    /// Prekey ring, oldest first. See [`Node::rotate_prekey`]. Never empty.
    ring: Vec<Prekey>,
    /// The prekey we currently advertise — the newest entry's public half. Kept
    /// as a field because it is public API and callers seal to it.
    pub prekey_pub: [u8; 32],
    pub petname: String,

    pub topics: HashSet<Addr>,
    addrs: HashSet<Addr>,

    seen: HashMap<Id, u32>, // id -> retain-until
    store: store::Store,
    paths: Paths,
    peer_prekeys: HashMap<Addr, [u8; 32]>,
    peer_busy: HashMap<Addr, u8>,
    peer_names: HashMap<Addr, String>, // the display name a peer announces (a hint, not identity)
    // §7 Double Ratchet sessions (PR0b), keyed by peer. In-memory only, like
    // the three maps above — rebuilt from the next ANNOUNCE exchange after a
    // restart rather than persisted, so there is no second place a session
    // secret lives to drift from the prekey ring's own accessor.
    sessions: HashMap<Addr, ratchet::Ratchet>,
    // The "offline window" (PR0 Part B): how long a prekey secret — and, via
    // session bootstrap, a ratchet session's skipped-key cache — survives
    // before deletion. Defaults to PREKEY_LIFETIME_SECS; runtime-configurable
    // via set_offline_window_secs so this is the one policy value both the
    // seal layer and the session layer read, rather than two consts that
    // could drift apart.
    prekey_lifetime_secs: u32,

    max_store_bytes: usize,
    seq: u64,
    frags: HashMap<Id, Fountain>,
    pub mtu: usize,
    manifests: HashMap<Id, file::Manifest>,
    pending: HashMap<Id, Pending>, // ACKREQ messages awaiting a receipt (§8)
    acked: HashSet<Id>,            // orig ids we've received receipts for
    rpc_pending: HashSet<u64>,     // request ids awaiting a response (L4)
    // Keyed by request id; the `Addr` is the response's *authenticated* sender
    // (its `Src::Full` key hashed), retained so a caller can check the reply
    // actually came from the service it asked — a flooded response is otherwise
    // forgeable by anyone who saw the request id.
    rpc_responses: HashMap<u64, (Addr, rpc::Response)>,
    rpc_inbox: Vec<(Addr, u64, rpc::Request)>, // requests delivered to a service
    feed_inbox: Vec<feed::Event>,              // feed events on subscribed topics (L5)
    quotas: congestion::Quotas,                // per-source flood quota (§10)
    pinned: HashSet<Id>,                       // magnets a seed-vault keeps forever
    gossip: HashMap<Iface, congestion::TokenBucket>, // per-link WANT service budget
    gossip_rate: u32,
    last_sweep: u32, // when expiry-based pruning last ran
}

struct Pending {
    wire: Vec<u8>,
    backoff: congestion::Backoff,
}

// The `Node` router's methods live in `src/node/` — split by concern (task
// #23). They are `impl Node` blocks in descendant modules of this crate root,
// so they keep full access to the private fields declared just above with no
// visibility change. A pure move: the wire format and public API are identical.
mod node;

// ---------------------------------------------------------------------------
// Sessions — a UDP-like bidirectional link, and a simple reliable stream on it.
//
// A datagram is an ordinary sealed+signed unicast envelope carrying an app tag,
// a port, and a replay sequence. It is best-effort and unordered, like UDP; the
// peer is a cryptographic address, so the "connection" survives roaming and NAT
// changes with no handshake. `Reliable` layers a small Go-Back-N ARQ on top for
// when you need an ordered byte stream (SSH, git) — reliability is endpoint
// state, never a network property, exactly as QUIC rides UDP.
// ---------------------------------------------------------------------------

pub mod session;

// ---------------------------------------------------------------------------
// §7 Double Ratchet — state-of-the-art forward-secret sessions. Each message
// gets a fresh key derived from a hash chain; a new DH public in the header
// turns the ratchet, mixing in fresh entropy. Compromise of current state
// reveals nothing older than the last ratchet turn, and out-of-order arrival
// (normal in SPORE) is handled by caching skipped message keys.
//
// KDF = BLAKE2b; AEAD = ChaCha20-Poly1305; DH = X25519. Message on the wire:
// [dh_pub:32][n:2][pn:2][ct] with ct = AEAD(mk, nonce=n, ad=header).
// ---------------------------------------------------------------------------

pub mod ratchet;

// ---------------------------------------------------------------------------
// §9 Mix mode — anonymity by nesting. An onion is sealed envelopes inside
// sealed envelopes: each layer is addressed to one mix, its payload sealed to
// that mix = 'O' ‖ the next full envelope. A mix opens its layer and re-injects
// the inner envelope as its own traffic. Payloads are padded to size classes so
// onion depth never shows on the wire.
// ---------------------------------------------------------------------------

pub mod mix;

// ---------------------------------------------------------------------------
// §5.4 Congestion control — four independent knobs. The router stays simple;
// these are primitives the originator and the bridges apply. (a) token bucket
// caps relayed airtime, (b) Trickle paces beacons, (c) backpressure scales
// sends by a peer's busy byte, (d) exponential backoff retries un-acked floods.
// ---------------------------------------------------------------------------

/// Fountain coding (§3) — see [`fountain`]. Extracted from this file unchanged;
/// the public names stay reachable at the crate root so nothing outside had to
/// move with it.
/// §7 crypto — see [`seal`]. Extracted from this file unchanged; the public names
/// stay at the crate root so no caller had to move with it.
/// §2 the envelope — see [`envelope`]. Extracted from this file unchanged; the
/// public names stay at the crate root so no caller had to move with it.
mod envelope;
pub use envelope::{fl, ty, Envelope, Err, Src, VER};

mod seal;
pub use seal::{open_sealed, prekey_keypair, seal, topic_open, topic_seal, SEALED_FILE_NAME};
// Crate-internal: the file layer seals chunks, the router derives the box nonce.
use seal::{chunk_open, chunk_seal, seal_nonce};

mod fountain;
use fountain::FRAG_OVERHEAD;
pub use fountain::{fragment, Fountain, MAX_FOUNTAIN_CHUNKS};

pub mod congestion;

// ---------------------------------------------------------------------------
// Files — content-addressed objects. A signed manifest indexes a set of
// ordinary chunk envelopes by their content IDs. Integrity is free (an
// envelope's ID *is* the hash of its bytes), and swarming is just WANT: any
// peer that holds a chunk can answer for it, since the chunk is named by
// content, not by who made it.
// ---------------------------------------------------------------------------

pub mod file;

// ---------------------------------------------------------------------------
// Custody — what a node holds, and where it holds it. Metadata stays resident;
// the bytes spill to disk past a memory budget, so what a node can carry is
// bounded by its disk rather than by its RAM.
// ---------------------------------------------------------------------------

mod store;

/// The storage nutrient: where the bytes go when they are not resident.
///
/// `FsSpill` is what a daemon, desktop or Android node uses. `SpillBackend` is
/// public so a runtime whose storage is *not* a filesystem — a browser tab, an
/// MCU — can supply its own; see `docs/DESIGN.md`'s "The spore and the soil".
pub use store::{FsSpill, SpillBackend};

// ---------------------------------------------------------------------------
// Page 2, rule 2 — KISS framing for byte streams (TCP, serial, RFCOMM, TNCs).
// ---------------------------------------------------------------------------

pub mod kiss;

// ---------------------------------------------------------------------------
// Page 2, rule 3 — text-channel armor (SMS, email, Usenet, paper, voice).
// ~S1.<base32(env)>.<base32(sha256(env)[..4])>~
// ---------------------------------------------------------------------------

pub mod armor;

/// Malformed-input robustness: every parser reachable from a stranger, fed
/// arbitrary and near-miss bytes. Test-only, and in `src/` rather than `tests/`
/// because the freeze guard treats all of `tests/` as frozen contract.
#[cfg(test)]
mod robustness;

// ---------------------------------------------------------------------------
// L4 Request/response — RPC as a convention (tags 0x02 request, 0x03 response).
// A request is a signed DATA to a service (address or topic); the reply is a
// signed DATA back to the requester, correlated by a nonce and routed along the
// reverse path the request taught. HTTP-shaped, but medium-independent.
// ---------------------------------------------------------------------------

pub mod rpc;

// ---------------------------------------------------------------------------
// L5 Feeds — pub/sub over topics (tag 0x05). Publish to a topic, subscribers
// following it receive every event; late joiners backfill from any peer's store
// via INV/WANT. Signed gossip minus JSON, relays, and always-on internet.
// ---------------------------------------------------------------------------

pub mod feed;

// §7 KEYROT — encrypted-topic key rotation (forward-secret ratchet + membership
// rekey), built on the `topic_seal`/`seal` primitives above.
pub mod invite;
pub mod topic;

// The network carries its own genome: publish/discover the bootstrap bundle
// (source, manual, binaries) over the mesh, and pin it as a seed vault.
pub mod bundle;

// Bridges — SPORE rides everything (spec Page 2). See `src/bridge/` for the
// per-medium modules; each only moves envelope bytes in and out of a `Node`.
pub mod bridge;

// SPORE Direct — a negotiated, non-routed, end-to-end encrypted datagram pipe for
// low-latency media between two identities. An application profile on top of the
// existing sealed+signed unicast path; no envelope/store/hub/wire changes.
pub mod direct;

// C ABI for the Python / Go / JS wrappers under `bindings/`.
pub mod ffi;

// Browser node ABI (wasm32) for the JS transports under `web/`.
#[cfg(target_arch = "wasm32")]
pub mod wasm;

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn keypair() -> SigningKey {
        let mut s = [0u8; 32];
        OsRng.fill_bytes(&mut s);
        SigningKey::from_bytes(&s)
    }

    #[test]
    fn identity_restores_from_seed() {
        // A node's whole identity is its 32-byte seed: persist it and rebuild the
        // same address and keys (the browser's local-storage persistence relies on
        // exactly this round-trip).
        let a = Node::new("persist", &["news"]);
        let seed = a.seed();
        let b = Node::from_seed("persist", &["news"], &seed);
        assert_eq!(a.addr, b.addr, "same seed must reproduce the same address");
        assert_eq!(a.sk.to_bytes(), b.sk.to_bytes(), "signing key restored");
        assert_eq!(a.prekey_pub, b.prekey_pub, "prekey is deterministic from the seed");
        // A signature made by the restored node verifies against the original addr.
        let mut e = Envelope::new(ty::DATA, ZERO_DEST, 1_700_000_000, b"hi".to_vec());
        e.sign(&b.sk);
        assert!(e.verify());
    }

    #[test]
    fn distinct_seeds_give_distinct_identities() {
        let a = Node::from_seed("x", &[], &[1u8; 32]);
        let b = Node::from_seed("x", &[], &[2u8; 32]);
        assert_ne!(a.addr, b.addr);
        assert_ne!(a.prekey_pub, b.prekey_pub);
        // `new` is just `from_seed` with a random seed → fresh nodes differ.
        assert_ne!(Node::new("x", &[]).addr, Node::new("x", &[]).addr);
    }

    #[test]
    fn envelope_roundtrip_and_sig() {
        let sk = keypair();
        let mut e = Envelope::new(ty::DATA, topic_of("test"), 1_700_000_000, b"hello".to_vec());
        e.sign(&sk);
        let wire = e.wire();
        let (d, n) = Envelope::decode(&wire).unwrap();
        assert_eq!(n, wire.len());
        assert_eq!(d.payload, b"hello");
        assert!(d.verify());
    }

    #[test]
    fn id_is_stable_under_hop_decrement() {
        let sk = keypair();
        let mut e = Envelope::new(ty::DATA, ZERO_DEST, 1_700_000_000, b"x".to_vec());
        e.sign(&sk);
        let id1 = e.id();
        e.hops -= 1; // relays do this
        assert_eq!(id1, e.id(), "id must not change when hops decrements");
    }

    #[test]
    fn tampering_breaks_signature() {
        let sk = keypair();
        let mut e = Envelope::new(ty::DATA, ZERO_DEST, 1_700_000_000, b"pay".to_vec());
        e.sign(&sk);
        let mut wire = e.wire();
        let plen_pos = 16 + 32; // header + full src
        wire[plen_pos + 2] ^= 0xff; // flip a payload byte
        let (d, _) = Envelope::decode(&wire).unwrap();
        assert!(!d.verify());
    }

    #[test]
    fn fountain_decodes_from_a_lossy_subset() {
        // Build a signed envelope big enough to need many chunks.
        let sk = keypair();
        let mut e = Envelope::new(ty::DATA, ZERO_DEST, 1_700_000_000, vec![0xABu8; 4000]);
        e.sign(&sk);
        let wire = e.wire();
        let id = e.id();
        let cs = 200usize;
        let count = wire.len().div_ceil(cs);

        // Emit data + plenty of repair, drop ~40% with a deterministic LCG.
        let indices: Vec<u8> = (0..(count as u8).saturating_add(60)).collect();
        let frags = fragment(&wire, cs, 16, e.expiry, ZERO_DEST, id, &indices);

        let mut f = Fountain::new();
        let mut rng: u64 = 0x1234_5678;
        let mut fed = 0;
        let mut recovered = None;
        for fr in &frags {
            rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1);
            if (rng >> 33) % 100 < 40 {
                continue; // 40% loss
            }
            fed += 1;
            let idx = fr.payload[16];
            let cnt = fr.payload[17];
            let chunk = fr.payload[18..].to_vec();
            if let Some(w) = f.add(&id, idx, cnt, chunk) {
                recovered = Some(w);
                break;
            }
        }
        let w = recovered.expect("should reassemble");
        assert_eq!(w, wire, "reassembled bytes must equal original");
        let (d, _) = Envelope::decode(&w).unwrap();
        assert!(d.verify(), "reassembled signature must verify");
        assert!(fed >= count, "need at least `count` independent chunks");
        assert!(fed <= count + 10, "fountain overhead should be small");
    }

    #[test]
    fn seal_open_roundtrip() {
        let bob = Node::new("bob", &[]);
        let msg = b"for bob only";
        let sealed = seal(msg, &bob.prekey_pub);
        assert_eq!(bob.open(&sealed).as_deref(), Some(&msg[..]));
        let mallory = Node::new("m", &[]);
        assert!(mallory.open(&sealed).is_none());
    }

    #[test]
    fn kiss_roundtrip_with_escapes() {
        let frame = vec![0x01, 0xC0, 0x02, 0xDB, 0x03];
        let stream = kiss::encode(&frame);
        let out = kiss::decode(&stream);
        assert_eq!(out, vec![frame]);
    }

    #[test]
    fn armor_roundtrip() {
        let sk = keypair();
        let mut e = Envelope::new(ty::DATA, ZERO_DEST, 1_700_000_000, b"armor me".to_vec());
        e.sign(&sk);
        let wire = e.wire();
        let text = format!("noise before {} noise after", armor::wrap(&wire));
        assert_eq!(armor::unwrap(&text).unwrap(), wire);
    }

    #[test]
    fn send_small_is_a_single_envelope() {
        let now = 1_700_000_000;
        let mut a = Node::new("a", &["news"]);
        let f = a.send(topic_of("news"), b"hi".to_vec(), now).unwrap();
        assert_eq!(f.len(), 1, "a small payload must not fragment");
    }

    #[test]
    fn send_large_object_fragments_and_reassembles() {
        let now = 1_700_000_000;
        let mut a = Node::new("a", &["news"]);
        let mut b = Node::new("b", &["news"]);

        let payload = vec![0x5Au8; 5000]; // well over one MTU
        let forwards = a.send(topic_of("news"), payload.clone(), now).unwrap();
        assert!(forwards.len() > 1, "a large payload must fragment into many sends");

        // Flood every fragment across the A—B link.
        let mut delivered = Vec::new();
        for f in &forwards {
            if let Forward::Flood { bytes, .. } = f {
                let rx = b.on_rx(bytes, 0, Some(a.addr), now);
                delivered.extend(rx.delivered);
            }
        }

        // The app sees exactly one reassembled object — never a raw fragment.
        assert_eq!(delivered.len(), 1, "one delivery, no raw fragments leaked to the app");
        assert_eq!(delivered[0].payload, payload, "reassembled payload matches");
        assert!(delivered[0].verify(), "reassembled signature verifies");
    }

    #[test]
    fn relays_forward_fragments_without_reassembling() {
        // C follows no relevant topic and is not the dest, so it must relay the
        // fragments onward but never buffer/reassemble the object itself.
        let now = 1_700_000_000;
        let mut a = Node::new("a", &["news"]);
        let mut c = Node::new("c", &[]); // relay: no matching topic
        let forwards = a.send(topic_of("news"), vec![0x11u8; 5000], now).unwrap();

        let mut relayed = 0;
        for f in &forwards {
            if let Forward::Flood { bytes, .. } = f {
                let rx = c.on_rx(bytes, 0, Some(a.addr), now);
                assert!(rx.delivered.is_empty(), "a relay delivers nothing to its app");
                relayed += rx.forwards.len();
            }
        }
        assert!(relayed > 1, "a relay must forward the fragments onward");
    }

    #[test]
    fn two_nodes_sync_over_a_link() {
        let now = 1_700_000_000;
        let mut a = Node::new("a", &["news"]);
        let mut b = Node::new("b", &["news"]);
        a.originate(topic_of("news"), b"headline".to_vec(), now);
        // B pulls: A sends INV -> B replies WANT -> A sends the envelope.
        let inv = a.build_inv(&b.topics);
        let want_rx = b.on_rx(&inv, 0, Some(a.addr), now);
        assert_eq!(want_rx.forwards.len(), 1);
        if let Forward::Directed { bytes, .. } = &want_rx.forwards[0] {
            let get_rx = a.on_rx(bytes, 0, Some(b.addr), now); // A answers WANT
            for f in get_rx.forwards {
                if let Forward::Directed { bytes, .. } = f {
                    b.on_rx(&bytes, 0, Some(a.addr), now);
                }
            }
        }
        assert_eq!(b.store_len(), 1, "B should now hold A's message");
    }

    fn fwd_bytes(f: &Forward) -> Vec<u8> {
        match f {
            Forward::Flood { bytes, .. } => bytes.clone(),
            Forward::Directed { bytes, .. } => bytes.clone(),
        }
    }

    /// Wire two fresh nodes together: exchange ANNOUNCEs so each learns the
    /// other's prekey and a path back.
    fn meet(a: &mut Node, b: &mut Node, now: u32) {
        for f in a.build_announce(now) {
            b.on_rx(&fwd_bytes(&f), 0, Some(a.addr), now);
        }
        for f in b.build_announce(now) {
            a.on_rx(&fwd_bytes(&f), 0, Some(b.addr), now);
        }
    }

    #[test]
    fn in_progress_file_chunks_are_pinned_under_memory_pressure() {
        let now = 1_700_000_000;
        let mut src = Node::new("src", &[]);
        let file: Vec<u8> = (0..4000u32).map(|i| (i.wrapping_mul(7)) as u8).collect();
        let (magnet, _mf) = src.publish_file("f.bin", &file, ZERO_DEST, now);

        // Pull the manifest wire and the chunk wires straight out of src's store.
        let mut manifest_wire = Vec::new();
        let mut chunk_wires: Vec<Vec<u8>> = Vec::new();
        for (_, w) in src.store_wires() {
            let (e, _) = Envelope::decode(&w).unwrap();
            match e.payload.first() {
                Some(&file::MANIFEST_TAG) => manifest_wire = w,
                Some(&file::CHUNK_TAG) => chunk_wires.push(w),
                _ => {}
            }
        }
        assert!(chunk_wires.len() >= 3, "multi-chunk file for the test");

        let chunk0_id = Envelope::decode(&chunk_wires[0]).unwrap().0.id();
        let in_store = |n: &Node, id: &Id| n.store_wires().iter().any(|(k, _)| k == id);

        // Receiver with a tiny store budget learns the manifest and one chunk,
        // leaving the file in-progress.
        let mut rx = Node::new("rx", &[]);
        rx.set_store_budget(2000);
        rx.on_rx(&manifest_wire, 0, None, now);
        rx.on_rx(&chunk_wires[0], 0, None, now);

        // Hammer the store with junk floods from many sources to force eviction.
        // The junk envelopes are smaller than the chunk, so a size-first eviction
        // policy would drop the chunk first — pinning is what protects it.
        for _ in 0..150 {
            let mut j = Node::new("j", &[]);
            let f = j.originate(ZERO_DEST, vec![0xEE; 240], now);
            rx.on_rx(&fwd_bytes(&f[0]), 0, Some(j.addr), now);
        }
        assert!(in_store(&rx, &chunk0_id), "the in-progress chunk was pinned through the storm");

        // With room to hold the whole file, delivering the rest reassembles it.
        rx.set_store_budget(10 * 1024 * 1024);
        for w in &chunk_wires[1..] {
            rx.on_rx(w, 0, None, now);
        }
        assert_eq!(rx.file_bytes(&magnet).as_deref(), Some(&file[..]), "pinned chunks reassemble");
    }

    #[test]
    fn source_quota_throttles_a_flooder() {
        let now = 1_700_000_000;
        let mut src = Node::new("S", &[]);
        let mut relay = Node::new("R", &[]);
        relay.set_source_quota(500); // tiny sustained budget for this source

        // S sprays 40 distinct public floods at the same instant. The token
        // bucket lets a burst through, then R stops storing and relaying S's
        // traffic — the flooder is capped well below the 40 it sent.
        let mut forwarded = 0;
        for i in 0..40u32 {
            let f = src.originate(ZERO_DEST, vec![i as u8; 200], now);
            let rx = relay.on_rx(&fwd_bytes(&f[0]), 0, Some(src.addr), now);
            if !rx.forwards.is_empty() {
                forwarded += 1;
            }
        }
        assert!(forwarded >= 1, "the first envelopes fit the burst");
        assert!(forwarded < 40, "an over-quota flooder is throttled, got {forwarded}");
    }

    #[test]
    fn datagram_session_roundtrip_and_replay() {
        let now = 1_700_000_000;
        let mut a = Node::new("a", &[]);
        let mut b = Node::new("b", &[]);
        meet(&mut a, &mut b, now);

        let mut sa = a.dial(b.addr, 22).expect("a knows b's prekey");
        let mut sb = b.dial(a.addr, 22).expect("b knows a's prekey");

        let fwds = a.dg_send(&mut sa, b"ping", now);
        let (e, _) = Envelope::decode(&fwd_bytes(&fwds[0])).unwrap();

        assert_eq!(b.dg_recv(&mut sb, &e).as_deref(), Some(&b"ping"[..]), "peer decrypts datagram");
        assert!(b.dg_recv(&mut sb, &e).is_none(), "a replayed datagram is rejected");

        // A different node cannot open it (sealed to B's prekey).
        let mut m = Node::new("m", &[]);
        meet(&mut a, &mut m, now);
        let mut sm = m.dial(a.addr, 22).unwrap();
        assert!(m.dg_recv(&mut sm, &e).is_none(), "wrong recipient can't read it");
    }

    #[test]
    fn reliable_stream_recovers_over_30pct_loss() {
        let t0 = 1_700_000_000;
        let mut a = Node::new("a", &[]);
        let mut b = Node::new("b", &[]);
        meet(&mut a, &mut b, t0);

        let mut ra = a.reliable(a.dial(b.addr, 22).unwrap());
        let mut rb = b.reliable(b.dial(a.addr, 22).unwrap());

        let payload: Vec<u8> = (0..4000u32).map(|i| (i.wrapping_mul(7)) as u8).collect();

        let mut to_b: Vec<Vec<u8>> = Vec::new();
        let mut to_a: Vec<Vec<u8>> = Vec::new();
        let mut now = t0;
        for f in ra.write(&mut a, &payload, now) {
            to_b.push(fwd_bytes(&f));
        }

        let mut rng: u64 = 0xDEAD_BEEF;
        let drop30 = |rng: &mut u64| {
            *rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1);
            (*rng >> 33) % 100 < 30
        };

        let mut got = Vec::new();
        for _ in 0..5000 {
            now += 1;
            for w in std::mem::take(&mut to_b) {
                if drop30(&mut rng) {
                    continue;
                }
                let (e, _) = Envelope::decode(&w).unwrap();
                for f in rb.deliver(&mut b, &e, now) {
                    to_a.push(fwd_bytes(&f));
                }
            }
            got.extend(rb.read());
            for w in std::mem::take(&mut to_a) {
                if drop30(&mut rng) {
                    continue;
                }
                let (e, _) = Envelope::decode(&w).unwrap();
                for f in ra.deliver(&mut a, &e, now) {
                    to_b.push(fwd_bytes(&f));
                }
            }
            for f in ra.poll(&mut a, now) {
                to_b.push(fwd_bytes(&f));
            }
            for f in rb.poll(&mut b, now) {
                to_a.push(fwd_bytes(&f));
            }
            if got.len() >= payload.len() && to_a.is_empty() && to_b.is_empty() {
                break;
            }
        }
        assert_eq!(got, payload, "reliable stream reassembles in order despite 30% loss");
    }

    #[test]
    fn publish_fetch_and_verify_file() {
        let now = 1_700_000_000;
        let mut a = Node::new("a", &[]);
        let mut b = Node::new("b", &[]);
        meet(&mut a, &mut b, now);

        let data: Vec<u8> = (0..9000u32).map(|i| (i.wrapping_mul(31)) as u8).collect();
        let (magnet, mf) = a.publish_file("field-notes.txt", &data, ZERO_DEST, now);

        // The small manifest floods; B absorbs it but holds no data yet.
        for f in &mf {
            b.on_rx(&fwd_bytes(f), 0, Some(a.addr), now);
        }
        assert!(!b.has_file(&magnet), "B knows the manifest but not the chunks");
        assert!(b.file_bytes(&magnet).is_none());

        // B pulls the chunks it lacks; A answers from its store by content ID.
        let want = b.fetch(&magnet);
        assert_eq!(want.len(), 1, "one WANT covering the missing chunks");
        for f in &want {
            let rx = a.on_rx(&fwd_bytes(f), 0, Some(b.addr), now);
            for cf in rx.forwards {
                b.on_rx(&fwd_bytes(&cf), 0, Some(a.addr), now);
            }
        }

        assert!(b.has_file(&magnet), "B now holds every chunk");
        assert_eq!(
            b.file_bytes(&magnet).as_deref(),
            Some(&data[..]),
            "file reassembles and is content-verified against the signed manifest"
        );
    }

    #[test]
    fn kiss_stream_reassembles_across_reads() {
        let sk = keypair();
        let mut e = Envelope::new(ty::DATA, ZERO_DEST, 1_700_000_000, b"streamed".to_vec());
        e.sign(&sk);
        let f1 = bridge::KissStream::frame(&e.wire());
        let f2 = bridge::KissStream::frame(&e.wire());
        let mut wire = f1.clone();
        wire.extend_from_slice(&f2);

        // Split the byte stream at an awkward point and feed it in two reads.
        let mut ks = bridge::KissStream::new();
        let mut frames = ks.push(&wire[..f1.len() - 3]);
        frames.extend(ks.push(&wire[f1.len() - 3..]));
        assert_eq!(frames.len(), 2, "both frames recovered across the split");
        assert_eq!(frames[0], e.wire());
        assert_eq!(frames[1], e.wire());
    }

    #[test]
    fn bag_inv_want_push_moves_an_envelope() {
        let now = 1_700_000_000;
        let mut a = Node::new("a", &["news"]);
        a.originate(topic_of("news"), b"bag me".to_vec(), now);

        // A advertises what it holds.
        let (_f, ids) = bridge::bag(&mut a, bridge::Bag::Inv, 0, now);
        assert_eq!(ids.len(), 16, "one stored envelope");

        // A serves those IDs; B pushes the result into its own store.
        let (_f, envs) = bridge::bag(&mut a, bridge::Bag::Want(ids), 0, now);
        let mut b = Node::new("b", &["news"]);
        let (_f, resp) = bridge::bag(&mut b, bridge::Bag::Push(envs), 0, now);
        assert!(resp.is_empty());
        assert_eq!(b.store_len(), 1, "the envelope crossed the bag bridge");
    }

    #[test]
    fn folder_store_round_trip() {
        let now = 1_700_000_000;
        let dir = std::env::temp_dir().join(format!("spore-folder-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        let mut a = Node::new("a", &["news"]);
        a.originate(topic_of("news"), b"note in a folder".to_vec(), now);
        let wrote = bridge::store::export_all(&dir, &a).unwrap();
        assert!(wrote >= 1, "wrote the envelope as <hexid>.spore");

        let mut b = Node::new("b", &["news"]);
        let rx = bridge::store::import(&dir, &mut b, 0, now).unwrap();
        assert_eq!(b.store_len(), wrote, "reading the folder = receiving");
        assert!(rx.delivered.iter().any(|e| e.payload == b"note in a folder"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn double_ratchet_bidirectional_and_out_of_order() {
        let now = 1_700_000_000; // fixed clock; this test is about ordering, not expiry
        let (a_sec, a_pub) = ratchet::keypair();
        let (b_sec, b_pub) = ratchet::keypair();
        let mut alice = ratchet::Ratchet::init_alice(a_sec, b_pub, PREKEY_LIFETIME_SECS);
        let mut bob = ratchet::Ratchet::init_bob(b_sec, b_pub, a_pub, PREKEY_LIFETIME_SECS);

        // Alice -> Bob (bootstraps Bob's receiving chain).
        let m1 = alice.encrypt(b"hello bob");
        assert_eq!(bob.decrypt(&m1, now).as_deref(), Some(&b"hello bob"[..]));

        // Bob -> Alice (turns the ratchet in both directions).
        let r1 = bob.encrypt(b"hi alice");
        assert_eq!(alice.decrypt(&r1, now).as_deref(), Some(&b"hi alice"[..]));

        // Alice -> Bob, delivered out of order: the later one first.
        let m2 = alice.encrypt(b"one");
        let m3 = alice.encrypt(b"two");
        assert_eq!(bob.decrypt(&m3, now).as_deref(), Some(&b"two"[..]), "arrives first");
        assert_eq!(bob.decrypt(&m2, now).as_deref(), Some(&b"one"[..]), "skipped key recovers it");

        // A replay of an already-consumed message is rejected.
        assert!(bob.decrypt(&m1, now).is_none(), "replay rejected");
    }

    #[test]
    fn onion_routes_through_mixes_and_hides_the_secret() {
        let now = 1_700_000_000;
        let exp = now + 3600;
        let m1 = Node::new("m1", &["mix"]);
        let m2 = Node::new("m2", &["mix"]);
        let r = Node::new("r", &[]);

        // Innermost: public dest (recipient anonymity), payload sealed to R.
        let secret = b"burn the ledgers at dawn";
        let mut inner = Envelope::new(ty::DATA, ZERO_DEST, exp, seal(secret, &r.prekey_pub));
        inner.flags |= fl::ENCRYPTED;

        let hops = [(m1.addr, m1.prekey_pub), (m2.addr, m2.prekey_pub)];
        let onion = mix::onion_wrap(&inner, &hops, exp).expect("wrap");

        // Outer layer is addressed to M1 and leaks nothing.
        assert_eq!(onion.dest, m1.addr);
        assert!(!onion.wire().windows(secret.len()).any(|w| w == secret), "no plaintext on the wire");
        assert!(m2.onion_peel(&onion).is_none(), "only the addressed mix can peel");

        // M1 peels -> layer for M2; M2 peels -> the innermost; R opens it.
        let l2 = m1.onion_peel(&onion).expect("m1 peels");
        let (e2, _) = Envelope::decode(&l2).unwrap();
        assert_eq!(e2.dest, m2.addr);

        let l3 = m2.onion_peel(&e2).expect("m2 peels");
        let (e3, _) = Envelope::decode(&l3).unwrap();
        assert_eq!(e3.dest, ZERO_DEST, "innermost is public for recipient anonymity");

        assert_eq!(r.open(&e3.payload).as_deref(), Some(&secret[..]), "only R recovers the secret");
        assert!(m1.open(&e3.payload).is_none(), "a mix cannot read the payload");
    }

    #[test]
    fn a_forged_signature_cannot_bind_a_victims_address() {
        // The attack the signature check exists to stop. Nothing secret is
        // needed: a public key is public, and the SIGNED flag is one bit.
        let now = 1_700_000_000;
        let mut nbrs: bridge::Neighbors<u32> = bridge::Neighbors::new(3600);
        let victim = Node::new("victim", &[]);

        let mut forged = Envelope::new(ty::DATA, ZERO_DEST, now + 3600, b"x".to_vec());
        forged.src = Src::Full(victim.sk.verifying_key().to_bytes());
        forged.flags |= fl::SIGNED;
        forged.sig = Some([0u8; 64]);
        assert!(!forged.verify(), "the forgery is not actually signed");

        assert_eq!(nbrs.snoop(&forged.wire(), 666u32, now), None, "must teach nothing");
        assert_eq!(
            nbrs.resolve(&victim.addr, now),
            None,
            "a directed send must not be unicast to the forger"
        );
    }

    #[test]
    fn a_short_source_is_a_claim_not_evidence() {
        // SRC8 carries an 8-byte address and no key, so there is nothing to
        // verify against — anyone can name anyone. It must not bind.
        let now = 1_700_000_000;
        let mut nbrs: bridge::Neighbors<u32> = bridge::Neighbors::new(3600);
        let victim = Node::new("victim", &[]);

        let mut short = Envelope::new(ty::DATA, ZERO_DEST, now + 3600, b"y".to_vec());
        short.src = Src::Short(victim.addr);
        short.flags |= fl::SIGNED | fl::SRC8;
        short.sig = Some([0u8; 64]);

        assert_eq!(nbrs.snoop(&short.wire(), 777u32, now), None);
        assert_eq!(nbrs.resolve(&victim.addr, now), None);
    }

    #[test]
    fn path_learning_also_refuses_a_forged_signature() {
        // The same forgery aimed at the node's own path table: it would bind the
        // victim to whichever interface the forgery arrived on.
        let now = 1_700_000_000;
        let mut n = Node::new("n", &[]);
        let victim = Node::new("victim", &[]);

        let mut forged = Envelope::new(ty::DATA, ZERO_DEST, now + 3600, b"z".to_vec());
        forged.src = Src::Full(victim.sk.verifying_key().to_bytes());
        forged.flags |= fl::SIGNED;
        forged.sig = Some([0u8; 64]);

        n.on_rx(&forged.wire(), 7, None, now);
        assert!(n.paths.fresh(&victim.addr, now).is_none(), "no path learned from a forgery");

        // ...while a genuinely signed envelope from the same node still teaches.
        let mut real = Envelope::new(ty::DATA, ZERO_DEST, now + 3600, b"z".to_vec());
        real.sign(&victim.sk);
        n.on_rx(&real.wire(), 7, None, now);
        assert!(n.paths.fresh(&victim.addr, now).is_some(), "a real signature still learns");
    }

    #[test]
    fn a_forged_source_cannot_spend_a_victims_quota() {
        // Attacker sprays unstamped junk that merely *names* the victim. If that
        // charged the victim's bucket, the victim's own mail would stop being
        // relayed — a denial of service against a third party, bought with junk.
        let now = 1_700_000_000;
        let mut relay = Node::new("relay", &[]);
        relay.set_source_quota(300);
        let mut victim = Node::new("victim", &[]);

        for i in 0..40u8 {
            let mut junk = Envelope::new(ty::DATA, ZERO_DEST, now + 3600, vec![i; 60]);
            junk.src = Src::Short(victim.addr);
            junk.flags |= fl::SIGNED | fl::SRC8; // src is only parsed when SIGNED
            junk.sig = Some([0u8; 64]);
            relay.on_rx(&junk.wire(), 1, None, now);
        }

        // The victim's genuinely signed mail must still be relayed.
        let fwds = victim.originate(ZERO_DEST, b"let me through".to_vec(), now);
        let wire = fwd_bytes(&fwds[0]);
        let rx = relay.on_rx(&wire, 2, None, now);
        assert!(!rx.forwards.is_empty(), "a forgery must not consume the victim's budget");
    }

    #[test]
    fn a_free_stamp_does_not_buy_a_quota_exemption() {
        // A stamp is leading zero bits of a hash, so class 1 costs ~2 tries and
        // half of all envelopes have it by accident. If `stamp > 0` exempted mail
        // from the quota, §10 would bound nothing at all.
        let now = 1_700_000_000;
        let mut q = congestion::Quotas::new(100);
        let src = [7u8; 8];

        // Spend past the bucket's burst with class-1 mail. If class 1 were exempt
        // every one of these would pass, however many were sent.
        let mut admitted = 0;
        for _ in 0..100 {
            if q.admit(src, 100, 1, now) {
                admitted += 1;
            }
        }
        assert!(admitted < 100, "class 1 is not proof of work and must be charged");
        assert!(!q.admit(src, 100, 1, now), "and once the budget is spent it is refused");

        // ...while a genuinely mined stamp still passes freely, on the same
        // exhausted bucket, which is what "priority is bought" means.
        assert!(q.admit(src, 100, congestion::STAMP_QUOTA_BYPASS_BITS, now), "real work still buys it");
        assert!(q.admit(src, 5000, 255, now), "and the highest class is never throttled");
    }

    #[test]
    fn a_want_cannot_be_used_as_an_amplifier() {
        // One unsigned WANT once pulled a full stored envelope for every id it
        // listed — measured at 32x gain, and replayable forever because INV/WANT
        // are consumed before dedup and before the quota (§6). On a radio link
        // every reply also spends airtime that §10 caps at 10% by law.
        let now = 1_700_000_000;
        let mut victim = Node::new("victim", &[]);
        let mut ids = Vec::new();
        for i in 0..400u32 {
            let f = victim.originate(ZERO_DEST, vec![(i % 251) as u8; 400], now);
            if let Some(Forward::Flood { bytes, .. } | Forward::Directed { bytes, .. }) = f.into_iter().next()
            {
                ids.push(Envelope::decode(&bytes).unwrap().0.id());
            }
        }
        let mut payload = Vec::new();
        for id in &ids {
            payload.extend_from_slice(id);
        }
        let want = Envelope::new(ty::WANT, ZERO_DEST, 0, payload).wire();

        let rx = victim.on_rx(&want, 1, None, now);
        assert!(
            rx.forwards.len() <= MAX_IDS_PER_GOSSIP,
            "one packet must not buy more than {MAX_IDS_PER_GOSSIP} envelopes, got {}",
            rx.forwards.len()
        );

        // The per-link budget is what bounds a *stream* of these. Replaying the
        // same request must not keep paying out.
        let mut served = 0;
        for _ in 0..20 {
            served += victim.on_rx(&want, 1, None, now).forwards.len();
        }
        assert_eq!(served, 0, "a spent link budget must not refill within the same second");

        // ...while a different link still gets service, and time still refills.
        assert!(!victim.on_rx(&want, 2, None, now).forwards.is_empty(), "per-link, not global");
        assert!(!victim.on_rx(&want, 1, None, now + 60).forwards.is_empty(), "refills over time");
    }

    #[test]
    fn ordinary_gossip_still_works() {
        // The bound must not break the mechanism it protects: a modest WANT from a
        // real peer is answered in full.
        let now = 1_700_000_000;
        let mut a = Node::new("a", &[]);
        let mut ids = Vec::new();
        for i in 0..5u8 {
            let f = a.originate(ZERO_DEST, vec![i; 100], now);
            if let Some(Forward::Flood { bytes, .. } | Forward::Directed { bytes, .. }) = f.into_iter().next()
            {
                ids.push(Envelope::decode(&bytes).unwrap().0.id());
            }
        }
        let mut payload = Vec::new();
        for id in &ids {
            payload.extend_from_slice(id);
        }
        let want = Envelope::new(ty::WANT, ZERO_DEST, 0, payload).wire();
        assert_eq!(a.on_rx(&want, 1, None, now).forwards.len(), 5, "every id we hold is served");

        // An INV offering ids we lack still produces a WANT asking for them.
        let mut inv_payload = Vec::new();
        for i in 0..5u8 {
            inv_payload.extend_from_slice(&[i; 16]);
        }
        let inv = Envelope::new(ty::INV, ZERO_DEST, 0, inv_payload).wire();
        let rx = a.on_rx(&inv, 1, None, now);
        assert_eq!(rx.forwards.len(), 1, "one WANT asking for what we lack");
    }

    #[test]
    fn incomplete_fountain_sets_cannot_accumulate() {
        // Measured before this bound: 20,000 incomplete sets held, each carrying
        // real chunk bytes, and still held 10 million seconds later. Cheapest
        // possible attack — unsigned fragments are Src::None, which the quota
        // admits unconditionally, so this cost the sender nothing.
        let now = 1_700_000_000;
        let mut v = Node::new("victim", &[]);
        for i in 0..3_000u32 {
            let mut payload = vec![0u8; 18];
            payload[..4].copy_from_slice(&i.to_be_bytes()); // a distinct orig_id each
            payload[16] = 0; // idx
            payload[17] = 4; // claims 4 chunks, sends 1 — never solvable
            payload.extend_from_slice(&[0xAA; 200]);
            let mut e = Envelope::new(ty::DATA, ZERO_DEST, now + 3600, payload);
            e.flags |= fl::FRAGMENT | fl::FLOOD;
            v.on_rx(&e.wire(), 1, None, now);
        }
        assert!(
            v.frags.len() <= MAX_PARTIAL_OBJECTS + 1,
            "held {} partial objects against a cap of {MAX_PARTIAL_OBJECTS}",
            v.frags.len()
        );

        // And an abandoned set is collected on time, not kept forever.
        let live = v.frags.len();
        assert!(live > 0);
        v.on_rx(
            &Envelope::new(ty::DATA, ZERO_DEST, now + PARTIAL_TIMEOUT_SECS + 100, b"tick".to_vec()).wire(),
            1,
            None,
            now + PARTIAL_TIMEOUT_SECS + 1,
        );
        assert_eq!(v.frags.len(), 0, "sets past PARTIAL_TIMEOUT_SECS are collected");
    }

    #[test]
    fn every_peer_grown_table_has_a_ceiling() {
        // The caps are large enough that driving each one through `on_rx` would be
        // a slow test for no extra confidence — the interesting property is that
        // `enforce_bounds` brings an over-full table back down, whatever filled it.
        let now = 1_700_000_000;
        let mut n = Node::new("n", &[]);

        for i in 0..(MAX_PEERS + 500) as u32 {
            let mut a = [0u8; 8];
            a[..4].copy_from_slice(&i.to_be_bytes());
            n.peer_prekeys.insert(a, [0u8; 32]);
            n.peer_busy.insert(a, 0);
            n.peer_names.insert(a, "x".into());
            // §7 ratchet sessions (PR0b) are peer-keyed exactly like the three
            // tables above, and must be bounded the same way.
            n.sessions
                .insert(a, ratchet::Ratchet::init_bob([0u8; 32], [0u8; 32], [0u8; 32], PREKEY_LIFETIME_SECS));
        }
        for i in 0..(MAX_ACKED + 500) as u32 {
            let mut id = [0u8; 16];
            id[..4].copy_from_slice(&i.to_be_bytes());
            n.acked.insert(id);
        }
        for i in 0..(MAX_SEEN + 500) as u32 {
            let mut id = [0u8; 16];
            id[..4].copy_from_slice(&i.to_be_bytes());
            n.seen.insert(id, now + 3600);
        }
        for _ in 0..(MAX_INBOX + 500) {
            n.feed_inbox.push(feed::Event { topic: ZERO_DEST, from: None, data: vec![1] });
        }

        n.enforce_bounds(now);

        assert!(n.peer_prekeys.len() <= MAX_PEERS, "prekeys {}", n.peer_prekeys.len());
        assert!(n.peer_busy.len() <= MAX_PEERS);
        assert!(n.peer_names.len() <= MAX_PEERS);
        assert!(n.sessions.len() <= MAX_PEERS, "sessions {}", n.sessions.len());
        assert!(n.acked.len() <= MAX_ACKED, "acked {}", n.acked.len());
        assert!(n.seen.len() <= MAX_SEEN, "seen {}", n.seen.len());
        assert_eq!(n.feed_inbox.len(), MAX_INBOX, "inbox trimmed to the cap");
    }

    #[test]
    fn dedup_evicts_the_nearest_to_expiring_first() {
        // Which entry is dropped matters: an id still in flight that we forget
        // costs a duplicate relay, so the ones kept should be the ones with the
        // most life left.
        let now = 1_700_000_000;
        let mut n = Node::new("n", &[]);
        for i in 0..(MAX_SEEN + 100) as u32 {
            let mut id = [0u8; 16];
            id[..4].copy_from_slice(&i.to_be_bytes());
            // The first 100 expire soonest.
            let until = if i < 100 { now + 1 } else { now + 100_000 };
            n.seen.insert(id, until);
        }
        n.enforce_bounds(now);
        assert!(n.seen.len() <= MAX_SEEN);
        let mut short_lived_kept = 0;
        for i in 0..100u32 {
            let mut id = [0u8; 16];
            id[..4].copy_from_slice(&i.to_be_bytes());
            if n.seen.contains_key(&id) {
                short_lived_kept += 1;
            }
        }
        assert!(short_lived_kept < 50, "should have dropped the soon-to-expire ones first");
    }

    #[test]
    fn neighbors_snoop_resolve_and_expire() {
        let now = 1_700_000_000;
        // U here is a stand-in underlay address (e.g. a Meshtastic node number).
        let mut nbrs: bridge::Neighbors<u32> = bridge::Neighbors::new(3600);

        // A signed ANNOUNCE is observed arriving from underlay address 42.
        let mut a = Node::new("a", &[]);
        let bytes = fwd_bytes(&a.build_announce(now)[0]);
        assert_eq!(nbrs.snoop(&bytes, 42u32, now), Some(a.addr), "snoop learns the sender");
        assert_eq!(nbrs.resolve(&a.addr, now), Some(42), "directed sends now resolve to a unicast");

        // Unsigned frames must not populate the table (can't verify the source).
        let unsigned = Envelope::new(ty::DATA, ZERO_DEST, now + 3600, b"x".to_vec()).wire();
        assert_eq!(nbrs.snoop(&unsigned, 99u32, now), None);

        // Nor may a frame that merely *claims* to be signed. Setting the flag is
        // free, so the signature itself has to be checked — see the two forgery
        // tests below, which is where that is proven.
        let mut liar = Envelope::new(ty::DATA, ZERO_DEST, now + 3600, b"x".to_vec());
        liar.src = Src::Full(a.sk.verifying_key().to_bytes());
        liar.flags |= fl::SIGNED;
        liar.sig = Some([0u8; 64]);
        assert_eq!(nbrs.snoop(&liar.wire(), 98u32, now), None);
        assert_eq!(nbrs.resolve(&a.addr, now), Some(42), "the real binding is untouched");

        // Unknown address -> None -> the bridge would broadcast.
        assert_eq!(nbrs.resolve(&[9u8; 8], now), None);

        // Stale bindings expire.
        nbrs.expire(now + 4000);
        assert_eq!(nbrs.resolve(&a.addr, now + 4000), None, "binding aged out");
    }

    #[test]
    fn ackreq_gets_a_receipt_back() {
        let now = 1_700_000_000;
        let mut a = Node::new("a", &[]);
        let mut b = Node::new("b", &[]);
        meet(&mut a, &mut b, now); // learn prekeys + a path each way

        // A -> B with ACKREQ. A has a path to B, so it's directed.
        let fwds = a.originate_ackreq(b.addr, b"confirm receipt".to_vec(), now);
        let id = Envelope::decode(&fwd_bytes(&fwds[0])).unwrap().0.id();
        assert!(!a.acked(&id));

        // B receives it, delivers, and floods a signed receipt back.
        let brx = b.on_rx(&fwd_bytes(&fwds[0]), 0, Some(a.addr), now);
        assert!(brx.delivered.iter().any(|e| e.payload == b"confirm receipt"));
        let receipt = brx.forwards.first().map(|f| match f {
            Forward::Flood { bytes, .. } | Forward::Directed { bytes, .. } => bytes.clone(),
        });
        let receipt = receipt.expect("B emitted a receipt");

        // A receives the receipt and marks the message delivered.
        a.on_rx(&receipt, 0, Some(b.addr), now);
        assert!(a.acked(&id), "A learned its ACKREQ message was delivered");
    }

    #[test]
    fn a_file_that_fits_one_manifest_keeps_the_pre_tree_encoding() {
        let now = 1_700_000_000;
        let mut n = Node::new("n", &[]);
        let body = vec![3u8; n.max_flat_file_bytes()];
        let (magnet, _) = n.publish_file("small.bin", &body, ZERO_DEST, now);

        let wire = n.get_wire(&magnet).expect("manifest stored");
        assert!(wire.len() <= n.mtu, "a flat manifest fits one frame");
        let (e, _) = Envelope::decode(&wire).unwrap();
        assert_eq!(
            e.payload.first(),
            Some(&file::MANIFEST_TAG),
            "small files must stay byte-compatible with nodes that predate trees"
        );
        assert_eq!(file::Manifest::decode(&e.payload).unwrap().depth, 0);
        assert_eq!(n.file_bytes(&magnet).as_deref(), Some(&body[..]));
    }

    #[test]
    fn a_file_past_one_manifest_grows_a_tree_but_keeps_one_magnet() {
        let now = 1_700_000_000;
        let mut n = Node::new("n", &[]);
        // A small MTU buys deep trees for the price of a small file.
        n.mtu = 200;
        let body: Vec<u8> = (0..30_000u32).map(|i| i.wrapping_mul(31) as u8).collect();
        assert!(body.len() > n.max_flat_file_bytes() * 8, "big enough to need levels");

        let (magnet, forwards) = n.publish_file("deep.bin", &body, ZERO_DEST, now);
        let wire = n.get_wire(&magnet).expect("root stored");
        assert!(wire.len() <= n.mtu, "the root is one frame however big the file is");
        assert_eq!(forwards.len(), 1, "one magnet floods — the file itself is pulled");

        let (e, _) = Envelope::decode(&wire).unwrap();
        assert_eq!(e.payload.first(), Some(&file::TREE_TAG));
        let root = file::Manifest::decode(&e.payload).unwrap();
        assert!(root.depth >= 2, "30 KB at a 200-byte MTU needs levels, got {}", root.depth);
        assert_eq!(root.total_len, body.len() as u64);

        assert!(n.has_file(&magnet));
        assert_eq!(n.file_bytes(&magnet).as_deref(), Some(&body[..]));
    }

    #[test]
    fn a_tree_resolves_top_down_through_windowed_fetches() {
        let now = 1_700_000_000;
        let mut a = Node::new("a", &[]);
        let mut b = Node::new("b", &[]);
        a.mtu = 200;
        b.mtu = 200;
        meet(&mut a, &mut b, now);

        let body: Vec<u8> = (0..12_000u32).map(|i| i.wrapping_mul(17) as u8).collect();
        let (magnet, forwards) = a.publish_file("deep.bin", &body, ZERO_DEST, now);

        // Only the root reaches B. It names sub-manifests, not chunks, so B
        // cannot yet name a single byte of the file.
        for f in forwards {
            b.on_rx(&fwd_bytes(&f), 0, Some(a.addr), now);
        }
        assert!(!b.has_file(&magnet), "B holds the map's first page only");

        let mut rounds = 0;
        while !b.has_file(&magnet) && rounds < 500 {
            let want = b.fetch(&magnet);
            assert!(!want.is_empty(), "B still needs parts but asked for nothing");
            for f in &want {
                let wire = fwd_bytes(f);
                assert!(wire.len() <= b.mtu, "a WANT must fit the link it rides");
                for answer in a.on_rx(&wire, 0, Some(b.addr), now).forwards {
                    b.on_rx(&fwd_bytes(&answer), 0, Some(a.addr), now);
                }
            }
            rounds += 1;
        }

        assert!(b.has_file(&magnet), "B never completed the file");
        assert_eq!(b.file_bytes(&magnet).as_deref(), Some(&body[..]));
        assert!(rounds > 1, "a tree this size cannot arrive in one frame");
    }

    #[test]
    fn a_tree_node_at_an_impossible_depth_is_refused() {
        let m = file::Manifest {
            file_id: [1u8; 16],
            chunk_size: 100,
            count: 0,
            total_len: 0,
            name: String::new(),
            chunk_ids: vec![],
            depth: 1,
            sealed_hdr: Vec::new(),
        };
        let mut enc = m.encode();
        assert_eq!(enc[0], file::TREE_TAG);

        enc[1] = 0; // leaf depth under the interior tag
        assert!(file::Manifest::decode(&enc).is_none());
        enc[1] = file::MAX_DEPTH + 1; // deeper than we are willing to walk
        assert!(file::Manifest::decode(&enc).is_none());
        enc[1] = file::MAX_DEPTH;
        assert!(file::Manifest::decode(&enc).is_some());
    }

    #[test]
    fn a_manifest_claiming_an_absurd_size_cannot_exhaust_memory() {
        let now = 1_700_000_000;
        let mut n = Node::new("n", &[]);
        let (magnet, _) = n.publish_file("t.bin", b"tiny", ZERO_DEST, now);

        // total_len arrives off the wire. Reassembly must fail on the bytes it
        // cannot find, not on the allocator it was asked to trust.
        n.manifests.get_mut(&magnet).unwrap().total_len = u64::MAX;
        assert!(n.file_bytes(&magnet).is_none());
        assert!(!n.has_file(&magnet) || n.write_file_to(&magnet, &mut Vec::new()).is_none());
    }

    /// A scratch directory that cleans itself up.
    struct TmpDir(std::path::PathBuf);
    impl TmpDir {
        fn new(tag: &str) -> TmpDir {
            let mut p = std::env::temp_dir();
            p.push(format!("spore-test-{tag}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&p);
            std::fs::create_dir_all(&p).unwrap();
            TmpDir(p)
        }
    }
    impl Drop for TmpDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn the_store_spills_to_disk_and_still_serves_what_it_spilled() {
        let now = 1_700_000_000;
        let dir = TmpDir::new("spill");
        let mut a = Node::new("a", &[]);
        a.set_store_budget(64 * 1024 * 1024);
        a.set_mem_budget(16 * 1024); // tiny, so almost everything spills
        a.set_spill_dir(&dir.0, now).expect("spill dir");

        let body: Vec<u8> = (0..400_000u32).map(|i| i.wrapping_mul(13) as u8).collect();
        let (magnet, _) = a.publish_file("big.bin", &body, ZERO_DEST, now);

        // The file is held, but almost none of it is resident.
        assert!(a.has_file(&magnet), "a spilled file is still held");
        assert!(a.store_bytes() > body.len(), "the store accounts for all of it");
        let on_disk = std::fs::read_dir(&dir.0).unwrap().count();
        assert!(on_disk > 100, "most of it went to disk, got {on_disk} files");

        // …and it reads back byte for byte, from wherever the bytes are.
        assert_eq!(a.file_bytes(&magnet).as_deref(), Some(&body[..]));

        // A spilled envelope answers a WANT exactly as a resident one does.
        let ids = a.missing(&magnet, 4);
        assert!(ids.is_empty(), "we hold every part");
        let stored = a.stored_ids();
        let mut id = [0u8; 16];
        id.copy_from_slice(&stored[..16]);
        assert!(a.get_wire(&id).is_some(), "a spilled envelope still serves");
    }

    #[test]
    fn a_restart_adopts_what_was_spilled_and_resumes_the_transfer() {
        let now = 1_700_000_000;
        let dir = TmpDir::new("adopt");
        let body: Vec<u8> = (0..200_000u32).map(|i| i.wrapping_mul(7) as u8).collect();

        let magnet = {
            let mut a = Node::new("a", &[]);
            a.set_store_budget(64 * 1024 * 1024);
            a.set_mem_budget(8 * 1024);
            a.set_spill_dir(&dir.0, now).unwrap();
            let (magnet, _) = a.publish_file("big.bin", &body, ZERO_DEST, now);
            assert!(a.has_file(&magnet));
            magnet
        }; // the node goes away — power cut, app killed, container reclaimed

        // A fresh node pointed at the same directory picks up where it left off.
        let mut b = Node::new("b", &[]);
        b.set_store_budget(64 * 1024 * 1024);
        b.set_mem_budget(8 * 1024);
        let adopted = b.set_spill_dir(&dir.0, now).expect("adopt");
        assert!(adopted > 100, "adopted {adopted} envelopes");
        assert!(b.has_file(&magnet), "the manifest was re-learned along with the chunks");
        assert_eq!(b.file_bytes(&magnet).as_deref(), Some(&body[..]));
    }

    #[test]
    fn an_absurdly_large_spilled_file_is_not_read_into_memory() {
        let now = 1_700_000_000;
        let dir = TmpDir::new("huge");
        // A file whose *name* is a plausible id but whose body is far too big to
        // be an envelope. Adoption must judge it by size before reading it.
        let name = "a".repeat(32) + ".spore";
        std::fs::write(dir.0.join(&name), vec![0u8; 4 * 1024 * 1024]).unwrap();

        let mut n = Node::new("n", &[]);
        assert_eq!(n.set_spill_dir(&dir.0, now).unwrap(), 0, "oversized file must not be adopted");
        assert!(!dir.0.join(&name).exists(), "and it is cleaned up rather than retried forever");
    }

    #[test]
    fn a_spilled_file_whose_name_lies_about_its_content_is_discarded() {
        let now = 1_700_000_000;
        let dir = TmpDir::new("forged");

        // A plausible-looking file whose bytes are not what its name claims.
        let mut e = Envelope::new(ty::DATA, ZERO_DEST, now + 86400, b"not what it says".to_vec());
        e.flags |= fl::FLOOD;
        let real = e.id();
        let mut lie = real;
        lie[0] ^= 0xff;
        let name: String = lie.iter().map(|b| format!("{b:02x}")).collect::<String>() + ".spore";
        std::fs::write(dir.0.join(&name), e.wire()).unwrap();
        std::fs::write(dir.0.join("not-even-hex.spore"), b"garbage").unwrap();

        let mut n = Node::new("n", &[]);
        let adopted = n.set_spill_dir(&dir.0, now).unwrap();
        assert_eq!(adopted, 0, "an id is the hash of its bytes — neither file matched");
        assert!(!n.has(&lie) && !n.has(&real));
        assert!(!dir.0.join(&name).exists(), "and the bad file is cleaned up");
    }

    #[test]
    fn a_sealed_file_is_encrypted_one_chunk_at_a_time() {
        let now = 1_700_000_000;
        let mut a = Node::new("a", &[]);
        let mut b = Node::new("b", &[]);
        meet(&mut a, &mut b, now);

        // Past what one sealed manifest can list, so this is a tree as well.
        let body: Vec<u8> = (0..120_000u32).map(|i| i.wrapping_mul(29) as u8).collect();
        let (magnet, _) = a.publish_file_sealed("plans.pdf", &body, b.addr, now).expect("prekey known");

        let root = a.manifests.get(&magnet).unwrap();
        assert_eq!(root.total_len, body.len() as u64, "sealing does not change the length");
        assert_eq!(root.name, SEALED_FILE_NAME, "the advertised name says nothing");
        assert!(!root.sealed_hdr.is_empty(), "the key travels sealed in the root");
        assert!(root.depth > 0, "big enough to be a tree as well as sealed");

        // Sample the plaintext rather than cross-product every window against
        // every wire — a few probes catch a plaintext chunk just as well.
        let probes: Vec<&[u8]> = (0..8).map(|i| &body[i * 9_000..i * 9_000 + 32]).collect();
        for (_, w) in a.store_wires() {
            assert!(w.len() <= a.mtu, "a sealed chunk must still fit the link");
            assert!(!w.windows(9).any(|x| x == b"plans.pdf"), "the file name leaked");
            for p in &probes {
                assert!(!w.windows(32).any(|x| x == *p), "file contents leaked");
            }
        }

        for (_, w) in a.store_wires() {
            b.on_rx(&w, 0, Some(a.addr), now);
        }

        let mut out = Vec::new();
        let (name, n) = b.open_file_to(&magnet, &mut out).expect("B can open it");
        assert_eq!(name, "plans.pdf", "the real name comes out of the sealed header");
        assert_eq!(n, body.len() as u64);
        assert_eq!(out, body);

        // The raw path refuses rather than handing back ciphertext that would
        // look like a short file.
        assert!(b.write_file_to(&magnet, &mut Vec::new()).is_none());

        // The property that makes streaming possible: any one chunk decrypts on
        // its own, with no other chunk present.
        let held = b.manifests.get(&magnet).unwrap();
        let (key, _) = b.open_sealed_header(held).expect("sealed to B");
        let mut leaves = Vec::new();
        b.walk_tree(held, &mut |id, depth, _| {
            if depth == 0 {
                leaves.push(*id);
            }
            true
        });
        let wire = b.get_wire(&leaves[3]).expect("a middle chunk");
        let (ce, _) = Envelope::decode(&wire).unwrap();
        let idx = u32::from_be_bytes([ce.payload[17], ce.payload[18], ce.payload[19], ce.payload[20]]);
        let plain = chunk_open(&ce.payload[21..], &key, idx).expect("one chunk opens alone");
        let start = idx as usize * held.chunk_size as usize;
        assert_eq!(plain, body[start..start + plain.len()]);

        // A relay carries every part and can read none of it.
        let mut c = Node::new("c", &[]);
        for (_, w) in a.store_wires() {
            c.on_rx(&w, 0, Some(a.addr), now);
        }
        let (root_env, _) = Envelope::decode(&a.get_wire(&magnet).unwrap()).unwrap();
        c.absorb_manifest(&root_env).expect("the root is signed, so anyone may parse it");
        assert!(c.has_file(&magnet), "the relay holds every part");
        assert!(c.open_file(&magnet).is_none(), "…and can read none of it");
    }

    #[test]
    fn a_file_sealed_the_old_whole_blob_way_still_opens() {
        let now = 1_700_000_000;
        let mut a = Node::new("a", &[]);
        let mut b = Node::new("b", &[]);
        meet(&mut a, &mut b, now);

        // Reproduce what a node published before chunks were sealed one by one:
        // name and bytes sealed as a single blob, then chunked, under the
        // placeholder name.
        let body: Vec<u8> = (0..2_000u32).map(|i| (i % 251) as u8).collect();
        let pk = a.peer_prekey(&b.addr).expect("prekey known");
        let mut inner = Vec::new();
        inner.extend_from_slice(&9u16.to_be_bytes());
        inner.extend_from_slice(b"plans.pdf");
        inner.extend_from_slice(&body);
        let (magnet, _) = a.publish_file(SEALED_FILE_NAME, &seal(&inner, &pk), b.addr, now);

        for (_, w) in a.store_wires() {
            b.on_rx(&w, 0, Some(a.addr), now);
        }
        let (name, got) = b.open_file(&magnet).expect("the legacy form still opens");
        assert_eq!(name, "plans.pdf");
        assert_eq!(got, body);
    }

    #[test]
    fn writing_a_file_out_streams_it_without_a_whole_second_copy() {
        let now = 1_700_000_000;
        let mut n = Node::new("n", &[]);
        n.mtu = 200;
        let body: Vec<u8> = (0..9_000u32).map(|i| (i % 253) as u8).collect();
        let (magnet, _) = n.publish_file("s.bin", &body, ZERO_DEST, now);

        let mut out = Vec::new();
        assert_eq!(n.write_file_to(&magnet, &mut out), Some(body.len() as u64));
        assert_eq!(out, body);

        // A node holding the root but no chunks reports nothing, rather than
        // handing back a short file that looks complete.
        let mut fresh = Node::new("fresh", &[]);
        fresh.mtu = 200;
        let (root, _) = Envelope::decode(&n.get_wire(&magnet).unwrap()).unwrap();
        fresh.absorb_manifest(&root).expect("the root is signed");
        assert!(fresh.write_file_to(&magnet, &mut Vec::new()).is_none());
    }

    #[test]
    fn sealed_files_hide_their_contents_and_their_name() {
        let now = 1_700_000_000;
        let mut a = Node::new("a", &[]);
        let mut b = Node::new("b", &[]);
        meet(&mut a, &mut b, now);

        let body: Vec<u8> = (0..5000u32).map(|i| (i % 251) as u8).collect();
        let (magnet, _f) = a.publish_file_sealed("plans.pdf", &body, b.addr, now).expect("prekey known");

        // Nothing on the wire reveals the name or the contents — a relay only
        // ever sees "some sealed file".
        for (_, w) in a.store_wires() {
            assert!(!w.windows(9).any(|x| x == b"plans.pdf"), "the file name leaked");
            assert!(!w.windows(16).any(|x| body.windows(16).any(|y| x == y)), "file contents leaked");
        }

        // Move everything A holds to B, as a sync or a chunk fetch would.
        for (_, w) in a.store_wires() {
            b.on_rx(&w, 0, Some(a.addr), now);
        }
        let (name, got) = b.open_file(&magnet).expect("B has every chunk and can open it");
        assert_eq!(name, "plans.pdf", "the real name is recovered from inside the seal");
        assert_eq!(got, body);

        // A third party relays the same chunks but can never read them.
        let mut c = Node::new("c", &[]);
        for (_, w) in a.store_wires() {
            c.on_rx(&w, 0, Some(a.addr), now);
        }
        assert!(c.open_file(&magnet).is_none(), "a relay must not be able to open it");

        // Progress is reportable while a transfer is still in flight.
        let files = b.files();
        let row = files.iter().find(|(m, ..)| *m == magnet).expect("listed");
        assert_eq!(row.3, row.4, "B holds every chunk");
    }

    #[test]
    fn announced_names_reach_peers_as_a_hint() {
        let now = 1_700_000_000;
        let mut a = Node::new("Jo's phone", &[]);
        let mut b = Node::new("basecamp", &[]);
        meet(&mut a, &mut b, now);

        // Each side learns what the other calls itself — so a UI can suggest a
        // petname instead of making anyone read out hex.
        assert_eq!(a.peer_name(&b.addr), Some("basecamp"));
        assert_eq!(b.peer_name(&a.addr), Some("Jo's phone"));
        // We never invent a name for someone we haven't heard.
        assert_eq!(a.peer_name(&Node::new("x", &[]).addr), None);

        // A node that follows topics still announces a readable name (the name
        // sits after the topic list on the wire).
        let mut t = Node::new("weatherbox", &["news", "weather"]);
        let mut c = Node::new("c", &[]);
        for f in t.build_announce(now) {
            c.on_rx(&fwd_bytes(&f), 0, Some(t.addr), now);
        }
        assert_eq!(c.peer_name(&t.addr), Some("weatherbox"));
    }

    #[test]
    fn send_direct_seals_to_the_peer_and_reports_delivery() {
        let now = 1_700_000_000;
        let mut a = Node::new("a", &[]);
        let mut b = Node::new("b", &[]);
        meet(&mut a, &mut b, now);

        // The ANNOUNCE exchange makes each side a known peer, with a prekey.
        let peers = a.peers(now);
        assert_eq!(peers.len(), 1, "exactly one peer, and not ourselves");
        assert_eq!(peers[0].0, b.addr);
        assert!(peers[0].2, "prekey learned from the ANNOUNCE");

        // A direct message is sealed to that prekey and asks for a receipt.
        let (id, fwds, encrypted) = a.send_direct(b.addr, b"meet at the north pier", now);
        assert!(encrypted, "prekey known => sealed");
        let wire = fwd_bytes(&fwds[0]);
        let (e, _) = Envelope::decode(&wire).unwrap();
        assert!(e.flags & fl::ENCRYPTED != 0, "marked ENCRYPTED");
        assert!(e.flags & fl::ACKREQ != 0, "marked ACKREQ");
        assert!(!e.payload.windows(4).any(|w| w == b"meet"), "the plaintext must never appear on the wire");

        // B opens it — via the ratchet session if PR0b ratcheted this one
        // (whichever of a/b sorted lower did), via the prekey ring otherwise —
        // and answers with a receipt.
        assert!(!a.acked(&id));
        let brx = b.on_rx(&wire, 0, Some(a.addr), now);
        let opened =
            brx.delivered.iter().find_map(|d| b.open_dm(a.addr, &d.payload, d.flags & fl::RATCHET != 0, now));
        assert_eq!(opened.expect("B decrypts"), b"meet at the north pier");
        let receipt = brx.forwards.first().map(fwd_bytes).expect("B emitted a receipt");
        a.on_rx(&receipt, 0, Some(b.addr), now);
        assert!(a.acked(&id), "the UI can show it delivered");

        // To a stranger we have no prekey for, it still sends — as cleartext.
        let c = Node::new("c", &[]);
        let (_, _, enc2) = a.send_direct(c.addr, b"hello stranger", now);
        assert!(!enc2, "no prekey yet => signed cleartext, not a silent failure");
    }

    // -- §7 Double Ratchet sessions (PR0b) ----------------------------------

    #[test]
    fn ratchet_session_roles_are_deterministic_regardless_of_announce_order() {
        // Whichever address sorts lower is always Alice for a pair, decided
        // independently by each side — so the order the two ANNOUNCEs happen
        // to be processed in must not matter. Run it both ways.
        let now = 1_700_000_000;

        let mut a1 = Node::new("a", &[]);
        let mut b1 = Node::new("b", &[]);
        for f in a1.build_announce(now) {
            b1.on_rx(&fwd_bytes(&f), 0, Some(a1.addr), now);
        }
        for f in b1.build_announce(now) {
            a1.on_rx(&fwd_bytes(&f), 0, Some(b1.addr), now);
        }

        let mut a2 = Node::from_seed("a", &[], &a1.seed());
        let mut b2 = Node::from_seed("b", &[], &b1.seed());
        // Same two identities, opposite processing order.
        for f in b2.build_announce(now) {
            a2.on_rx(&fwd_bytes(&f), 0, Some(b2.addr), now);
        }
        for f in a2.build_announce(now) {
            b2.on_rx(&fwd_bytes(&f), 0, Some(a2.addr), now);
        }

        // Whichever side is "Alice" (lower address) can ratchet-encrypt
        // immediately in both runs; whichever is "Bob" cannot yet. If order
        // flipped who plays which role, one of these two pairs would disagree
        // with the other about which side that is.
        let a_is_alice_1 = a1.addr < b1.addr;
        let a_is_alice_2 = a2.addr < b2.addr;
        assert_eq!(a_is_alice_1, a_is_alice_2, "identity, not order, decides the role");

        let (_, fwds1, enc1) = a1.send_direct(b1.addr, b"hello", now);
        let (_, fwds2, enc2) = a2.send_direct(b2.addr, b"hello", now);
        assert_eq!(enc1, enc2);
        let ratcheted1 = Envelope::decode(&fwd_bytes(&fwds1[0])).unwrap().0.flags & fl::RATCHET != 0;
        let ratcheted2 = Envelope::decode(&fwd_bytes(&fwds2[0])).unwrap().0.flags & fl::RATCHET != 0;
        assert_eq!(ratcheted1, ratcheted2, "same identities, same role, regardless of ANNOUNCE order");
        assert_eq!(ratcheted1, a_is_alice_1, "ratcheted from message one iff this side is Alice");
    }

    #[test]
    fn ratchet_session_is_independent_of_a_later_prekey_rotation() {
        // A session, once bootstrapped, is never re-seeded from a later
        // ANNOUNCE — its own ratchet evolution is what's trusted from then on.
        // Prove it survives the *unrelated* identity-layer prekey rotating.
        let now = 1_700_000_000;
        let mut a = Node::new("a", &[]);
        let mut b = Node::new("b", &[]);
        meet(&mut a, &mut b, now);

        // Get both sides past their first (necessarily plain-sealed, for
        // whichever side is "Bob") message, so both can ratchet from here.
        let (_, fwds, _) = a.send_direct(b.addr, b"first", now);
        let e = Envelope::decode(&fwd_bytes(&fwds[0])).unwrap().0;
        let brx = b.on_rx(&fwd_bytes(&fwds[0]), 0, Some(a.addr), now);
        assert!(brx
            .delivered
            .iter()
            .any(|d| b.open_dm(a.addr, &d.payload, e.flags & fl::RATCHET != 0, now).is_some()));

        // B rotates its identity prekey — a completely unrelated event.
        b.rotate_prekey(now + 100);
        assert_ne!(b.peer_prekey(&a.addr), None); // sanity: a's own view is untouched by b's rotation

        // The session between a and b must still work, using the ratchet's own
        // evolving state rather than either side's (possibly stale) prekey.
        let (_, fwds2, enc2) = b.send_direct(a.addr, b"after rotation", now + 100);
        assert!(enc2);
        let e2 = Envelope::decode(&fwd_bytes(&fwds2[0])).unwrap().0;
        let arx = a.on_rx(&fwd_bytes(&fwds2[0]), 0, Some(b.addr), now + 100);
        let opened = arx
            .delivered
            .iter()
            .find_map(|d| a.open_dm(b.addr, &d.payload, e2.flags & fl::RATCHET != 0, now + 100));
        assert_eq!(opened.as_deref(), Some(&b"after rotation"[..]));
    }

    #[test]
    fn open_dm_falls_back_to_the_prekey_ring_with_no_session() {
        // No ANNOUNCE exchange has happened, so no session exists for the
        // claimed sender — `open_dm` with `ratcheted=false` must still open a
        // plain seal via the prekey ring, exactly like `open` always has. This
        // is what keeps a peer who never gets a session (or restarted and
        // lost one) interoperating rather than silently locked out.
        let now = 1_700_000_000;
        let mut b = Node::new("b", &[]);
        let sealed = seal(b"no session here", &b.prekey_pub);
        let claimed_sender = Node::new("stranger", &[]).addr;
        let opened = b.open_dm(claimed_sender, &sealed, false, now);
        assert_eq!(opened.as_deref(), Some(&b"no session here"[..]));

        // And a RATCHET-flagged message with no matching session simply
        // doesn't open — there is nothing to fall back to for that branch.
        assert_eq!(b.open_dm(claimed_sender, &sealed, true, now), None);
    }

    #[test]
    fn offline_window_is_clamped_and_drives_both_the_ring_and_new_sessions() {
        // PR0 Part B: one knob, read by the seal layer (prekey sweep) and
        // handed to any ratchet session bootstrapped from here on — not two
        // consts that could drift apart.
        let mut n = Node::new("n", &[]);
        assert_eq!(n.offline_window_secs(), PREKEY_LIFETIME_SECS, "7 days by default");

        // Floor: can't go below the daily rotation cadence.
        n.set_offline_window_secs(1);
        assert_eq!(n.offline_window_secs(), PREKEY_PERIOD_SECS);
        // Ceiling: a sanity bound, not an unbounded number.
        n.set_offline_window_secs(u32::MAX);
        assert_eq!(n.offline_window_secs(), 365 * 86_400);

        // A shortened window actually shortens how long a swept-past-lifetime
        // prekey survives.
        let t0 = 1_700_000_000;
        let short = 3 * 86_400; // 3 days
        n.set_offline_window_secs(short);
        n.rotate_prekey(t0);
        assert_eq!(n.prekey_count(), 2);
        n.sweep_prekeys(t0 + short + 1);
        assert_eq!(n.prekey_count(), 1, "the boot entry aged out under the shortened window");

        // A session bootstrapped after the change wires the new window into
        // Ratchet::init_alice/init_bob without erroring, end to end through
        // the real ANNOUNCE/send_direct/open_dm path — the skip-TTL decay
        // behaviour itself is exercised at the primitive level by
        // ratchet.rs's own (now-parameterized) skipped-key tests.
        let mut a = Node::new("a", &[]);
        meet(&mut a, &mut n, t0);
        let (_, fwds, _) = a.send_direct(n.addr, b"hi", t0);
        let e = Envelope::decode(&fwd_bytes(&fwds[0])).unwrap().0;
        let nrx = n.on_rx(&fwd_bytes(&fwds[0]), 0, Some(a.addr), t0);
        assert!(nrx
            .delivered
            .iter()
            .any(|d| n.open_dm(a.addr, &d.payload, e.flags & fl::RATCHET != 0, t0).is_some()));
    }

    // -- §7 prekey ring (S-022) --------------------------------------------

    #[test]
    fn a_rotation_keeps_old_mail_readable_until_the_secret_expires() {
        let day = PREKEY_PERIOD_SECS;
        let t0 = 1_700_000_000;
        let mut bob = Node::new("bob", &[]);

        // Sealed to the prekey Bob advertises today.
        let old_pub = bob.prekey_pub;
        let old_mail = seal(b"north pier midnight", &old_pub);
        assert_eq!(bob.open(&old_mail).as_deref(), Some(&b"north pier midnight"[..]));

        // A day later he rotates. A sender who only heard the old ANNOUNCE still
        // reaches him — that is why `open` tries the whole live ring.
        bob.rotate_prekey(t0 + day);
        assert_ne!(bob.prekey_pub, old_pub, "he advertises the new one");
        assert_eq!(bob.prekey_count(), 2);
        assert_eq!(bob.open(&old_mail).as_deref(), Some(&b"north pier midnight"[..]));
        let new_mail = seal(b"and the new one", &bob.prekey_pub);
        assert_eq!(bob.open(&new_mail).as_deref(), Some(&b"and the new one"[..]));

        // Past the lifetime the old secret is gone, and with it the old mail.
        bob.sweep_prekeys(t0 + day + PREKEY_LIFETIME_SECS + 1);
        assert!(bob.open(&old_mail).is_none(), "THE forward-secrecy property");
        assert_eq!(bob.open(&new_mail).as_deref(), Some(&b"and the new one"[..]));
    }

    /// The failure mode of the old design, stated as a test: a seed-derived prekey
    /// is re-derivable, so "deleting" it means nothing. A rotated one is random,
    /// so restoring from the seed cannot bring it back.
    #[test]
    fn a_seed_restore_does_not_resurrect_a_swept_prekey() {
        let t0 = 1_700_000_000;
        let mut a = Node::from_seed("a", &[], &[9u8; 32]);
        a.rotate_prekey(t0);
        let rotated_pub = a.prekey_pub;
        let mail = seal(b"only the ring can read this", &rotated_pub);
        assert!(a.open(&mail).is_some());

        // Persist both halves and come back: still readable.
        let seed = a.seed();
        let ring = a.prekey_ring();
        let mut back = Node::from_seed("a", &[], &seed);
        assert!(back.restore_prekey_ring(&ring), "a well-formed ring restores");
        assert_eq!(back.addr, a.addr, "same identity");
        assert_eq!(back.prekey_pub, rotated_pub, "and the same advertised prekey");
        assert!(back.open(&mail).is_some());

        // Sweep the rotated secret away, persist *that*, and it stays gone.
        back.sweep_prekeys(t0 + PREKEY_LIFETIME_SECS + 1);
        back.rotate_prekey(t0 + PREKEY_LIFETIME_SECS + 2);
        back.sweep_prekeys(t0 + PREKEY_LIFETIME_SECS + 2);
        assert!(back.open(&mail).is_none(), "swept");
        let swept_ring = back.prekey_ring();
        let mut again = Node::from_seed("a", &[], &seed);
        assert!(again.restore_prekey_ring(&swept_ring));
        assert!(again.open(&mail).is_none(), "a restore must not resurrect it");

        // And the seed alone gives only the bootstrap key — no rotated secret.
        let seed_only = Node::from_seed("a", &[], &seed);
        assert_eq!(seed_only.prekey_count(), 1);
        assert!(seed_only.open(&mail).is_none(), "the seed is not the ring");
    }

    #[test]
    fn an_announce_moves_a_peers_seal_target_to_the_newest_prekey() {
        let now = 1_700_000_000;
        let mut bob = Node::new("bob", &[]);
        let mut alice = Node::new("alice", &[]);

        let a1 = bob.build_announce(now);
        let Forward::Flood { bytes, .. } = &a1[0] else { panic!() };
        alice.on_rx(bytes, 0, Some(bob.addr), now);
        assert_eq!(alice.peer_prekey(&bob.addr), Some(bob.prekey_pub));

        bob.rotate_prekey(now + PREKEY_PERIOD_SECS);
        let a2 = bob.build_announce(now + PREKEY_PERIOD_SECS);
        let Forward::Flood { bytes, .. } = &a2[0] else { panic!() };
        alice.on_rx(bytes, 0, Some(bob.addr), now + PREKEY_PERIOD_SECS);
        assert_eq!(alice.peer_prekey(&bob.addr), Some(bob.prekey_pub), "follows the rotation");

        // And a message sealed to what Alice now knows still opens.
        let sealed = seal(b"ack", &alice.peer_prekey(&bob.addr).unwrap());
        assert_eq!(bob.open(&sealed).as_deref(), Some(&b"ack"[..]));
    }

    #[test]
    fn the_ring_is_bounded_and_never_empty() {
        let mut n = Node::new("n", &[]);
        // Rotate far more often than the lifetime would ever allow, so nothing
        // expires and only the hard cap can hold the ring down.
        for i in 0..(MAX_PREKEY_RING as u32 * 4) {
            n.rotate_prekey(1_700_000_000 + i);
        }
        assert_eq!(n.prekey_count(), MAX_PREKEY_RING, "capped");
        // The newest always survives a sweep, however far in the future it runs.
        n.sweep_prekeys(u32::MAX);
        assert_eq!(n.prekey_count(), 1, "everything but the current key expired");
        let m = seal(b"still reachable", &n.prekey_pub);
        assert_eq!(n.open(&m).as_deref(), Some(&b"still reachable"[..]));
        // Sweeping again changes nothing.
        let before = n.prekey_pub;
        n.sweep_prekeys(u32::MAX);
        assert_eq!(n.prekey_count(), 1);
        assert_eq!(n.prekey_pub, before, "idempotent");
    }

    #[test]
    fn prekey_health_reports_unknowable_age_and_due_rotation_honestly() {
        let t0 = 1_700_000_000;
        let mut n = Node::new("n", &[]);

        // A fresh node: one unstamped bootstrap entry. Its age is unknowable, and
        // since it has never rotated, the next rotation is due right now.
        let (count, oldest_age, next_mint_in) = n.prekey_health(t0);
        assert_eq!(count, 1);
        assert_eq!(oldest_age, None, "bootstrap entry's true age is unknowable");
        assert_eq!(next_mint_in, 0, "never rotated => due now, not a made-up wait");

        // After a rotation the bootstrap entry gets stamped (identity.rs:
        // `rotate_prekey` backfills `born` on any zero entry) and a fresh newest
        // lands beside it, so both halves of the readout become concrete.
        n.rotate_prekey(t0);
        let (count2, oldest_age2, next_mint_in2) = n.prekey_health(t0 + 100);
        assert_eq!(count2, 2);
        assert_eq!(oldest_age2, Some(100), "the now-stamped bootstrap entry ages normally");
        assert_eq!(next_mint_in2, PREKEY_PERIOD_SECS - 100, "counts down to the next rotation");

        // Past the scheduled rotation, it reports zero rather than an underflowed
        // giant number — the UI should say "due", not lie with an enormous count.
        let (_, _, overdue) = n.prekey_health(t0 + PREKEY_PERIOD_SECS + 500);
        assert_eq!(overdue, 0);
    }

    #[test]
    fn rotation_happens_by_operating_not_by_being_asked() {
        let t0 = 1_700_000_000;
        let mut a = Node::new("a", &[]);
        let boot = a.prekey_pub;
        let mut b = Node::new("b", &[]);

        // Ordinary traffic drives the sweep, which drives rotation.
        let fwd = b.build_announce(t0);
        let Forward::Flood { bytes, .. } = &fwd[0] else { panic!() };
        a.on_rx(bytes, 0, Some(b.addr), t0);
        assert_ne!(a.prekey_pub, boot, "the bootstrap key is replaced on first sweep");

        let after_first = a.prekey_pub;
        let fwd = b.build_announce(t0 + 60);
        let Forward::Flood { bytes, .. } = &fwd[0] else { panic!() };
        a.on_rx(bytes, 0, Some(b.addr), t0 + 60);
        assert_eq!(a.prekey_pub, after_first, "and not again a minute later");
    }

    #[test]
    fn restore_prekey_ring_rejects_hostile_blobs_without_panicking() {
        let mut n = Node::new("n", &[]);
        n.rotate_prekey(1_700_000_000);
        let good = n.prekey_ring();
        let pub_before = n.prekey_pub;

        let mut v = Node::new("v", &[]);
        assert!(!v.restore_prekey_ring(&[]), "empty");
        assert!(!v.restore_prekey_ring(&[1]), "truncated header");
        assert!(!v.restore_prekey_ring(&[9, 1]), "unknown version");
        assert!(!v.restore_prekey_ring(&[1, 0]), "zero entries");
        assert!(!v.restore_prekey_ring(&[1, 255]), "absurd count");
        for i in 0..good.len() {
            assert!(!v.restore_prekey_ring(&good[..i]), "truncated at {i}");
        }
        // A blob whose public half does not match its secret must be refused, or a
        // node would advertise a key it cannot open.
        let mut lying = good.clone();
        lying[2] ^= 0xff;
        assert!(!v.restore_prekey_ring(&lying), "public/secret mismatch");
        // Every single-byte corruption: reject or accept, never panic.
        for i in 0..good.len() {
            let mut bad = good.clone();
            bad[i] ^= 0xff;
            let _ = v.restore_prekey_ring(&bad);
        }
        // A rejected restore left the victim untouched.
        let mut fresh = Node::new("f", &[]);
        let untouched = fresh.prekey_pub;
        assert!(!fresh.restore_prekey_ring(&lying));
        assert_eq!(fresh.prekey_pub, untouched, "a bad blob changes nothing");
        assert!(fresh.restore_prekey_ring(&good), "the intact one still works");
        assert_eq!(fresh.prekey_pub, pub_before);
    }

    /// S-023. Two properties the daemon got wrong for as long as it existed: the
    /// beacon's Trickle interval was in the wrong unit, and the frame it beaconed
    /// was the mesh-wide flood rather than §4's link-local HELLO.
    #[test]
    fn hello_is_link_local_and_the_beacon_cadence_is_in_seconds() {
        // The spec says the HELLO interval doubles 5 -> 80 *minutes*. These are
        // seconds, because that is the timer's base — the bug was passing 5 and 80.
        assert_eq!(HELLO_MIN_SECS, 300, "5 minutes, in the timer's own unit");
        assert_eq!(HELLO_MAX_SECS, 4800, "80 minutes");
        assert_eq!(ANNOUNCE_FLOOD_MIN_SECS, 3600, "the spec's 'ANNOUNCE flood <= 1/h'");

        let now = 1_700_000_000;
        let mut a = Node::new("a", &["news"]);

        // A HELLO stops at the first hop: hops == 0, so §5 rule 5 drops it there.
        let hello = a.build_hello(now);
        assert!(!hello.is_empty(), "a HELLO goes out on every interface");
        let Forward::Flood { bytes, .. } = &hello[0] else { panic!("HELLO floods locally") };
        let (he, _) = Envelope::decode(bytes).expect("decode HELLO");
        assert_eq!(he.hops, 0, "a HELLO must not be relayed");
        assert_eq!(he.typ, ty::ANNOUNCE);
        assert!(he.verify(), "still signed — it teaches our prekey");

        // A neighbour learns from it and does not pass it on.
        let mut b = Node::new("b", &[]);
        let rx = b.on_rx(bytes, 0, Some(a.addr), now);
        assert!(b.peer_prekey(&a.addr).is_some(), "the neighbour learned our prekey");
        assert!(rx.forwards.is_empty(), "hops == 0 => the mesh never sees it");

        // The flooded form is the expensive one, and is the one worth storing.
        let flood = a.build_announce(now);
        let Forward::Flood { bytes: fb, .. } = &flood[0] else { panic!("ANNOUNCE floods") };
        let (fe, _) = Envelope::decode(fb).expect("decode ANNOUNCE");
        assert_eq!(fe.hops, 16, "mesh-wide");
        let mut c = Node::new("c", &[]);
        assert!(!c.on_rx(fb, 0, Some(a.addr), now).forwards.is_empty(), "and it is relayed");
    }

    #[test]
    fn congestion_primitives() {
        use congestion::*;

        // (d) Backoff: fires with growing gaps, then exhausts after MAX.
        let mut bo = Backoff::new(0);
        assert!(!bo.due(0));
        assert!(bo.due(30));
        bo.fired(30);
        assert!(!bo.due(31));
        assert!(bo.due(60), "next retry ~30 s later");
        for t in 0..10 {
            if bo.due(t * 100000) {
                bo.fired(t * 100000);
            }
        }
        assert!(bo.exhausted(), "gives up after MAX tries");

        // (b) Trickle: doubles while quiet, resets on novelty.
        let mut tr = Trickle::new(0, 5, 80);
        assert!(tr.due(5));
        tr.fired(5);
        assert_eq!(tr.interval(), 10);
        tr.fired(15);
        assert_eq!(tr.interval(), 20);
        tr.reset(100);
        assert_eq!(tr.interval(), 5, "novelty snaps back to the fast interval");

        // (a) Token bucket: caps sustained throughput, refills over time.
        let mut tb = TokenBucket::new(100);
        assert!(tb.allow(80, 0));
        assert!(!tb.allow(80, 0), "same second: out of budget");
        assert!(tb.allow(80, 5), "refilled after 5 s");

        // (c) Backpressure: idle admits all; busy drops unstamped; *mined* mail
        // rides. A low class is not mined — class 3 is eight hashes' work — so it
        // is throttled like anything else; only STAMP_QUOTA_BYPASS_BITS buys the pass.
        assert!(admit(0, 0, 200));
        assert!(!admit(255, 0, 100));
        assert!(!admit(255, 3, 100), "a nearly-free stamp must not dodge backpressure");
        assert!(admit(255, congestion::STAMP_QUOTA_BYPASS_BITS, 100), "real work rides");
    }

    #[test]
    fn encrypted_topic_roundtrip() {
        let psk = [7u8; 32];
        let ct = topic_seal(b"members only", &psk);
        assert_ne!(&ct[24..], b"members only", "ciphertext is not plaintext");
        assert_eq!(topic_open(&ct, &psk).as_deref(), Some(&b"members only"[..]));
        assert!(topic_open(&ct, &[9u8; 32]).is_none(), "wrong key can't open it");
    }

    #[test]
    fn announce_carries_busy_byte() {
        let now = 1_700_000_000;
        let mut a = Node::new("a", &["news"]);
        let mut b = Node::new("b", &[]);
        for f in a.build_announce(now) {
            b.on_rx(&fwd_bytes(&f), 0, Some(a.addr), now);
        }
        assert_eq!(b.peer_busy(&a.addr), Some(a.busy()), "B learned A's busy byte");
    }

    #[test]
    fn mix_batch_holds_then_releases() {
        let mut q = mix::Batch::new(3);
        q.add(b"a".to_vec(), 0, 5);
        q.add(b"b".to_vec(), 0, 5);
        assert!(q.ready(100).is_empty(), "fewer than the batch minimum: hold");
        q.add(b"c".to_vec(), 0, 5);
        assert_eq!(q.ready(3).len(), 0, "batch full but not due yet");
        assert_eq!(q.ready(10).len(), 3, "batch full and due: release all");
    }

    #[test]
    fn csma_damps_a_flood() {
        let now = 1000;
        let mut csma = bridge::Csma::new();
        let id1 = [1u8; 16];
        let id2 = [2u8; 16];
        csma.schedule(id1, now, 5, false); // flood
        csma.schedule(id2, now, 5, false);
        // id1 is overheard twice while we wait -> we cancel our copy.
        csma.overheard(&id1);
        csma.overheard(&id1);
        let send = csma.ready(now + 5);
        assert_eq!(send, vec![id2], "only the un-overheard flood is transmitted");
    }

    #[test]
    fn crc_tail_detects_corruption() {
        let framed = bridge::crc_append(b"envelope bytes");
        assert_eq!(bridge::crc_check(&framed), Some(&b"envelope bytes"[..]));
        let mut bad = framed.clone();
        bad[0] ^= 0xff;
        assert!(bridge::crc_check(&bad).is_none(), "a flipped bit is caught");
    }

    #[test]
    fn folder_sync_publishes_and_materialises() {
        let now = 1_700_000_000;
        let base = std::env::temp_dir().join(format!("spore-sync-{}", std::process::id()));
        let src = base.join("src");
        let out = base.join("out");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("a.txt"), b"first file body").unwrap();
        std::fs::write(src.join("b.bin"), vec![0x42u8; 5000]).unwrap();

        let topic = topic_of("myfolder");
        let mut a = Node::new("a", &["myfolder"]);
        let mut b = Node::new("b", &["myfolder"]);

        // A publishes the directory; the manifests flood to B, which absorbs them.
        let mf = bridge::foldersync::publish_dir(&mut a, &src, topic, now).unwrap();
        for f in &mf {
            b.on_rx(&fwd_bytes(f), 0, Some(a.addr), now);
        }
        // B pulls the chunks it lacks; A serves them from its store.
        for w in b.fetch_all() {
            let arx = a.on_rx(&fwd_bytes(&w), 0, Some(b.addr), now);
            for cf in arx.forwards {
                b.on_rx(&fwd_bytes(&cf), 0, Some(a.addr), now);
            }
        }
        let wrote = bridge::foldersync::materialize(&b, &out).unwrap();
        assert_eq!(wrote, 2, "both files materialised");
        assert_eq!(std::fs::read(out.join("a.txt")).unwrap(), b"first file body");
        assert_eq!(std::fs::read(out.join("b.bin")).unwrap(), vec![0x42u8; 5000]);

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn rpc_request_gets_a_response() {
        let now = 1_700_000_000;
        let mut client = Node::new("c", &[]);
        let mut server = Node::new("s", &[]);
        meet(&mut client, &mut server, now); // learn prekeys + paths both ways

        let (id, fwds) = client.request(
            server.addr,
            rpc::Request { method: "GET".into(), path: "/temp".into(), body: vec![] },
            now,
        );
        // Server receives the request and queues it.
        server.on_rx(&fwd_bytes(&fwds[0]), 0, Some(client.addr), now);
        let reqs = server.poll_requests();
        assert_eq!(reqs.len(), 1);
        let (from, rid, req) = reqs.into_iter().next().unwrap();
        assert_eq!((req.method.as_str(), req.path.as_str()), ("GET", "/temp"));
        assert_eq!(rid, id);

        // Server answers; the reply routes back to the client.
        let rf = server.respond(from, rid, rpc::Response { status: 200, body: b"21C".to_vec() }, now);
        client.on_rx(&fwd_bytes(&rf[0]), 0, Some(server.addr), now);

        let resp = client.take_response(id).expect("response arrived");
        assert_eq!(resp.status, 200);
        assert_eq!(resp.body, b"21C");
    }

    // A large reply (a profile with an avatar is tens of KB) is too big for one
    // envelope, so the responder sends the RESPONSE payload through the
    // fountain-fragmenting `send` path rather than `respond`. This pins that the
    // reassembled whole re-enters the RPC demux and its *authenticated* sender is
    // retained — the check the app relies on so a flooded reply can't forge one.
    #[test]
    fn fragmented_response_reassembles_with_authenticated_sender() {
        let now = 1_700_000_000;
        let mut client = Node::new("c", &[]);
        let mut server = Node::new("s", &[]);
        meet(&mut client, &mut server, now);

        let (id, fwds) = client.request(
            server.addr,
            rpc::Request { method: "GET".into(), path: "/profile".into(), body: vec![] },
            now,
        );
        for f in &fwds {
            server.on_rx(&fwd_bytes(f), 0, Some(client.addr), now);
        }
        let (from, rid, _) = server.poll_requests().into_iter().next().unwrap();
        assert_eq!(rid, id);

        // The responder builds the RESPONSE payload by hand and fragments it with
        // `send` — the same bytes and path the JNI layer uses. 20 KB forces a
        // multi-fragment fountain set.
        let body = vec![0xABu8; 20_000];
        let payload = rpc::encode_response(rid, &rpc::Response { status: 200, body: body.clone() });
        let rf = server.send(from, payload.clone(), now).expect("fits a fountain set");
        assert!(rf.len() > 1, "a 20 KB reply must fragment");

        for f in &rf {
            client.on_rx(&fwd_bytes(f), 0, Some(server.addr), now);
        }
        let (sender, resp) = client.take_response_from(id).expect("reassembled reply");
        assert_eq!(sender, server.addr, "authenticated sender is the server");
        assert_eq!(resp.status, 200);
        assert_eq!(resp.body, body, "20 KB body survived fragmentation");
    }

    #[test]
    fn feed_publish_reaches_subscribers() {
        let now = 1_700_000_000;
        let mut publisher = Node::new("p", &[]);
        let mut sub = Node::new("s", &[]);
        sub.subscribe("alerts");

        let fwds = publisher.publish("alerts", b"the tide is turning".to_vec(), now);
        sub.on_rx(&fwd_bytes(&fwds[0]), 0, Some(publisher.addr), now);

        let events = sub.poll_feed();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].topic, topic_of("alerts"));
        assert_eq!(events[0].data, b"the tide is turning");
        assert!(sub.poll_feed().is_empty(), "poll drains the inbox");
    }

    #[test]
    fn meshtastic_frame_roundtrip() {
        use bridge::meshtastic;
        let sk = keypair();
        let mut e = Envelope::new(ty::DATA, ZERO_DEST, 1_700_000_000, b"over the mesh".to_vec());
        e.sign(&sk);

        // Wrap as a broadcast Meshtastic packet, then read it back.
        let frame = meshtastic::encode(&e.wire(), 0x1234_abcd, meshtastic::BROADCAST, 0x00c0_ffee);
        let (from, port, payload) = meshtastic::decode(&frame).expect("valid frame");
        assert_eq!(from, 0x1234_abcd, "sender node number survives");
        assert_eq!(port, meshtastic::PORT_PRIVATE_APP, "tagged as SPORE's private app");
        assert_eq!(payload, e.wire(), "the SPORE envelope round-trips intact");
        assert!(Envelope::decode(&payload).unwrap().0.verify(), "and still verifies");

        // A frame on some other portnum is not ours.
        let other = meshtastic::encode(&e.wire(), 1, meshtastic::BROADCAST, 1);
        // (portnum is fixed to PRIVATE_APP by encode; a real foreign app would
        // carry a different portnum — decode still parses, the runner filters.)
        assert_eq!(meshtastic::decode(&other).unwrap().1, meshtastic::PORT_PRIVATE_APP);
    }
}
