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

/// Default per-envelope wire budget used by `Node::send` to decide when to
/// fragment. Transports over tighter media (LoRa ~200 B, ESP-NOW 250 B) lower
/// `Node::mtu`; the router itself is MTU-agnostic.
pub const DEFAULT_MTU: usize = 1400;
/// Default per-source flood quota (§10): the sustained bytes/second any one
/// originating address may have a node store and relay. Generous enough that
/// legitimate traffic never notices; a safety valve against amplification abuse.
/// Stamped (proof-of-work) mail bypasses it. Tune with `Node::set_source_quota`.
pub const DEFAULT_SOURCE_QUOTA: u32 = 1024 * 1024;
/// Bytes a fragment envelope adds around its chunk: 16 header + 2 plen +
/// 16 orig_id + 1 index + 1 count. `chunk = mtu - FRAG_OVERHEAD`.
const FRAG_OVERHEAD: usize = 36;

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
// §2 Envelope — the only object
// ---------------------------------------------------------------------------

pub const VER: u8 = 0x01;

pub mod ty {
    pub const DATA: u8 = 0;
    pub const INV: u8 = 1;
    pub const WANT: u8 = 2;
    pub const ANNOUNCE: u8 = 3;
}
pub mod fl {
    pub const ENCRYPTED: u8 = 1;
    pub const SIGNED: u8 = 2;
    pub const FRAGMENT: u8 = 4;
    pub const ACKREQ: u8 = 8;
    pub const FLOOD: u8 = 16; // multicast / topic / public / route-discovery
    pub const SRC8: u8 = 32; // src carried as 8-byte address, not 32-byte key
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Src {
    None,
    Full([u8; 32]),
    Short(Addr),
}

#[derive(Clone, Debug)]
pub struct Envelope {
    pub typ: u8,
    pub flags: u8,
    pub hops: u8,
    pub expiry: u32,
    pub dest: Addr,
    pub src: Src,
    pub payload: Vec<u8>,
    pub sig: Option<[u8; 64]>,
}

#[derive(Debug)]
pub enum Err {
    Short,
    Version,
    Bad,
}

impl Envelope {
    pub fn new(typ: u8, dest: Addr, expiry: u32, payload: Vec<u8>) -> Self {
        Envelope { typ, flags: 0, hops: 16, expiry, dest, src: Src::None, payload, sig: None }
    }

    /// Header + src + plen + payload (no signature). `zero_hops` for the
    /// signing/ID pre-image; false for the wire form.
    fn body(&self, zero_hops: bool) -> Vec<u8> {
        let mut b = Vec::with_capacity(64 + self.payload.len());
        b.push(VER);
        b.push(self.typ);
        b.push(self.flags);
        b.push(if zero_hops { 0 } else { self.hops });
        b.extend_from_slice(&self.expiry.to_be_bytes());
        b.extend_from_slice(&self.dest);
        match &self.src {
            Src::None => {}
            Src::Full(pk) => b.extend_from_slice(pk),
            Src::Short(a) => b.extend_from_slice(a),
        }
        b.extend_from_slice(&(self.payload.len() as u16).to_be_bytes());
        b.extend_from_slice(&self.payload);
        b
    }

    /// The exact bytes put on the wire.
    pub fn wire(&self) -> Vec<u8> {
        let mut b = self.body(false);
        if let Some(sig) = &self.sig {
            b.extend_from_slice(sig);
        }
        b
    }

    /// ID = SHA-256(full envelope with hops byte zeroed)[..16]. Ties the id to
    /// the signature, and is stable under relays decrementing `hops`.
    pub fn id(&self) -> Id {
        let mut b = self.body(true);
        if let Some(sig) = &self.sig {
            b.extend_from_slice(sig);
        }
        let d = Sha256::digest(&b);
        let mut id = [0u8; 16];
        id.copy_from_slice(&d[..16]);
        id
    }

    /// Priority stamp: leading zero bits of the ID (proof-of-work, §10).
    pub fn stamp(&self) -> u8 {
        let id = self.id();
        let mut n = 0u8;
        for byte in id {
            if byte == 0 {
                n += 8;
            } else {
                n += byte.leading_zeros() as u8;
                break;
            }
        }
        n
    }

    pub fn sign(&mut self, sk: &SigningKey) {
        self.src = Src::Full(sk.verifying_key().to_bytes());
        self.flags |= fl::SIGNED;
        self.flags &= !fl::SRC8;
        let sig: Signature = sk.sign(&self.body(true));
        self.sig = Some(sig.to_bytes());
    }

    /// Verify a full-key signed envelope. SRC8 envelopes need `verify_with`.
    pub fn verify(&self) -> bool {
        match &self.src {
            Src::Full(pk) => self.verify_with(pk),
            _ => false,
        }
    }
    pub fn verify_with(&self, pubkey: &[u8; 32]) -> bool {
        if self.flags & fl::SIGNED == 0 {
            return false;
        }
        let (Some(sig), Ok(vk)) = (self.sig, VerifyingKey::from_bytes(pubkey)) else {
            return false;
        };
        vk.verify(&self.body(true), &Signature::from_bytes(&sig)).is_ok()
    }

    pub fn decode(buf: &[u8]) -> Result<(Envelope, usize), Err> {
        if buf.len() < 16 {
            return std::result::Result::Err(Err::Short);
        }
        if buf[0] != VER {
            return std::result::Result::Err(Err::Version);
        }
        let typ = buf[1];
        let flags = buf[2];
        let hops = buf[3];
        let expiry = u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]);
        let mut dest = [0u8; 8];
        dest.copy_from_slice(&buf[8..16]);
        let mut off = 16;
        let need = |off: usize, n: usize| -> Result<(), Err> {
            if off + n <= buf.len() {
                Ok(())
            } else {
                std::result::Result::Err(Err::Short)
            }
        };
        let src = if flags & fl::SIGNED != 0 {
            if flags & fl::SRC8 != 0 {
                need(off, 8)?;
                let mut a = [0u8; 8];
                a.copy_from_slice(&buf[off..off + 8]);
                off += 8;
                Src::Short(a)
            } else {
                need(off, 32)?;
                let mut pk = [0u8; 32];
                pk.copy_from_slice(&buf[off..off + 32]);
                off += 32;
                Src::Full(pk)
            }
        } else {
            Src::None
        };
        need(off, 2)?;
        let plen = u16::from_be_bytes([buf[off], buf[off + 1]]) as usize;
        off += 2;
        need(off, plen)?;
        let payload = buf[off..off + plen].to_vec();
        off += plen;
        let sig = if flags & fl::SIGNED != 0 {
            need(off, 64)?;
            let mut s = [0u8; 64];
            s.copy_from_slice(&buf[off..off + 64]);
            off += 64;
            Some(s)
        } else {
            None
        };
        Ok((Envelope { typ, flags, hops, expiry, dest, src, payload, sig }, off))
    }
}

// ---------------------------------------------------------------------------
// §3 Fragmentation — rateless fountain code over GF(2)
// ---------------------------------------------------------------------------

/// Selection bitmap for repair chunk `idx`: first `count` bits (MSB-first) of
/// SHA-256(orig_id ‖ idx). Empty selection maps to data chunk (idx mod count).
fn selection(orig_id: &Id, idx: u8, count: usize) -> BitVec {
    let mut h = Sha256::new();
    Digest::update(&mut h, orig_id);
    Digest::update(&mut h, [idx]);
    let d = h.finalize();
    let mut b = BitVec::zeros(count);
    for i in 0..count {
        if (d[i / 8] >> (7 - (i % 8))) & 1 == 1 {
            b.set(i);
        }
    }
    if b.is_zero() {
        b.set(idx as usize % count);
    }
    b
}

fn xor_into(dst: &mut [u8], src: &[u8]) {
    for (a, b) in dst.iter_mut().zip(src) {
        *a ^= *b;
    }
}

/// Produce fragment envelopes for the given chunk `indices`. Indices `< count`
/// are plain data chunks; `>= count` are repair chunks. The sender can mint
/// endless distinct repair indices, so this is rateless.
pub fn fragment(
    env_wire: &[u8],
    chunk: usize,
    hops: u8,
    expiry: u32,
    dest: Addr,
    orig_id: Id,
    indices: &[u8],
) -> Vec<Envelope> {
    let count = env_wire.len().div_ceil(chunk);
    assert!(count <= 255, "envelope too large for one fragment set");
    let mut padded = env_wire.to_vec();
    padded.resize(count * chunk, 0);
    let data = |i: usize| padded[i * chunk..(i + 1) * chunk].to_vec();

    let mut out = Vec::with_capacity(indices.len());
    for &idx in indices {
        let cbytes = if (idx as usize) < count {
            data(idx as usize)
        } else {
            let sel = selection(&orig_id, idx, count);
            let mut acc = vec![0u8; chunk];
            for i in 0..count {
                if sel.get(i) {
                    xor_into(&mut acc, &data(i));
                }
            }
            acc
        };
        let mut payload = Vec::with_capacity(18 + chunk);
        payload.extend_from_slice(&orig_id);
        payload.push(idx);
        payload.push(count as u8);
        payload.extend_from_slice(&cbytes);
        out.push(Envelope {
            typ: ty::DATA,
            flags: fl::FRAGMENT | fl::FLOOD,
            hops,
            expiry,
            dest,
            src: Src::None,
            payload,
            sig: None,
        });
    }
    out
}

/// Online reassembler: feed chunks in any order, at any loss rate. Decodes once
/// `count` linearly-independent chunks have arrived (typically count+2).
pub struct Fountain {
    count: usize,
    chunk: usize,
    rows: Vec<Row>, // kept in reduced row-echelon form
    done: Option<Vec<u8>>,
}
struct Row {
    pivot: usize,
    coeff: BitVec,
    data: Vec<u8>,
}
impl Fountain {
    pub fn new() -> Self {
        Fountain { count: 0, chunk: 0, rows: Vec::new(), done: None }
    }
    /// Feed one fragment's `(orig_id, idx, count, chunk_bytes)`.
    /// Returns the reassembled original envelope bytes once solvable.
    pub fn add(&mut self, orig_id: &Id, idx: u8, count: u8, chunk_bytes: Vec<u8>) -> Option<Vec<u8>> {
        if self.done.is_some() {
            return self.done.clone();
        }
        let count = count as usize;
        if self.count == 0 {
            self.count = count;
            self.chunk = chunk_bytes.len();
        }
        if count != self.count || chunk_bytes.len() != self.chunk {
            return None; // malformed / mismatched set
        }
        let coeff = if (idx as usize) < count {
            BitVec::unit(count, idx as usize)
        } else {
            selection(orig_id, idx, count)
        };
        self.insert(coeff, chunk_bytes);
        if self.rows.len() == self.count {
            self.solve()
        } else {
            None
        }
    }
    fn insert(&mut self, mut coeff: BitVec, mut data: Vec<u8>) {
        for r in &self.rows {
            if coeff.get(r.pivot) {
                coeff.xor(&r.coeff);
                xor_into(&mut data, &r.data);
            }
        }
        if let Some(p) = coeff.first_set() {
            for r in &mut self.rows {
                if r.coeff.get(p) {
                    r.coeff.xor(&coeff);
                    xor_into(&mut r.data, &data);
                }
            }
            self.rows.push(Row { pivot: p, coeff, data });
        } // else: linearly dependent, discard
    }
    fn solve(&mut self) -> Option<Vec<u8>> {
        let mut chunks = vec![vec![0u8; self.chunk]; self.count];
        for r in &self.rows {
            chunks[r.pivot] = r.data.clone();
        }
        let mut out = Vec::with_capacity(self.count * self.chunk);
        for c in &chunks {
            out.extend_from_slice(c);
        }
        // The reassembled envelope self-delimits: parse it to strip zero padding.
        match Envelope::decode(&out) {
            Ok((_, n)) => {
                let w = out[..n].to_vec();
                self.done = Some(w.clone());
                Some(w)
            }
            _ => None,
        }
    }
}
impl Default for Fountain {
    fn default() -> Self {
        Self::new()
    }
}

/// Minimal packed bit vector over GF(2).
#[derive(Clone)]
struct BitVec {
    words: Vec<u64>,
}
impl BitVec {
    fn zeros(len: usize) -> Self {
        BitVec { words: vec![0u64; len.div_ceil(64)] }
    }
    fn unit(len: usize, i: usize) -> Self {
        let mut b = Self::zeros(len);
        b.set(i);
        b
    }
    fn set(&mut self, i: usize) {
        self.words[i / 64] |= 1u64 << (i % 64);
    }
    fn get(&self, i: usize) -> bool {
        (self.words[i / 64] >> (i % 64)) & 1 == 1
    }
    fn xor(&mut self, o: &BitVec) {
        for k in 0..self.words.len() {
            self.words[k] ^= o.words[k];
        }
    }
    fn first_set(&self) -> Option<usize> {
        for (k, w) in self.words.iter().enumerate() {
            if *w != 0 {
                return Some(k * 64 + w.trailing_zeros() as usize);
            }
        }
        None
    }
    fn is_zero(&self) -> bool {
        self.words.iter().all(|w| *w == 0)
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

struct Stored {
    wire: Vec<u8>,
    expiry: u32,
    stamp: u8,
    seq: u64,
    dest: Addr,
}

// ---------------------------------------------------------------------------
// §7 Crypto — seal to a recipient prekey (libsodium crypto_box_seal shape).
// Forward secrecy comes from rotating prekeys daily and deleting the private
// half after 7 days: a seized device cannot read week-old mail.
// ---------------------------------------------------------------------------

fn seal_nonce(eph_pub: &[u8; 32], recip_pub: &[u8; 32]) -> [u8; 24] {
    let mut h = Blake2bVar::new(24).unwrap();
    h.update(eph_pub);
    h.update(recip_pub);
    let mut n = [0u8; 24];
    h.finalize_variable(&mut n).unwrap();
    n
}

/// Anonymous sealed box: output = ephemeral_pubkey(32) ‖ ciphertext.
pub fn seal(msg: &[u8], recip_prekey: &[u8; 32]) -> Vec<u8> {
    use crypto_box::aead::{generic_array::GenericArray, Aead};
    use crypto_box::{PublicKey, SalsaBox, SecretKey};
    let mut sb = [0u8; 32];
    OsRng.fill_bytes(&mut sb);
    let eph = SecretKey::from(sb);
    let eph_pub = eph.public_key();
    let their = PublicKey::from(*recip_prekey);
    let nonce = seal_nonce(eph_pub.as_bytes(), their.as_bytes());
    let ct = SalsaBox::new(&their, &eph).encrypt(GenericArray::from_slice(&nonce), msg).expect("seal");
    let mut out = Vec::with_capacity(32 + ct.len());
    out.extend_from_slice(eph_pub.as_bytes());
    out.extend_from_slice(&ct);
    out
}

/// Encrypted topic (§7): seal `msg` under a 32-byte pre-shared key with
/// XChaCha20-Poly1305. Output = 24-byte random nonce ‖ ciphertext. Everyone on
/// the topic shares the key; rotate it by flooding a `KEYROT` signed by the old.
pub fn topic_seal(msg: &[u8], psk: &[u8; 32]) -> Vec<u8> {
    use chacha20poly1305::aead::{Aead, KeyInit};
    use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
    let mut nonce = [0u8; 24];
    OsRng.fill_bytes(&mut nonce);
    let ct = XChaCha20Poly1305::new(Key::from_slice(psk))
        .encrypt(XNonce::from_slice(&nonce), msg)
        .expect("topic seal");
    let mut out = Vec::with_capacity(24 + ct.len());
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ct);
    out
}

/// Open an encrypted-topic payload; `None` if the key is wrong or it's corrupt.
pub fn topic_open(ct: &[u8], psk: &[u8; 32]) -> Option<Vec<u8>> {
    use chacha20poly1305::aead::{Aead, KeyInit};
    use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
    if ct.len() < 24 {
        return None;
    }
    XChaCha20Poly1305::new(Key::from_slice(psk)).decrypt(XNonce::from_slice(&ct[..24]), &ct[24..]).ok()
}

/// Fresh encryption **prekey** keypair `(secret, public)` for `seal`/`open_sealed`
/// (X25519, the same kind a `Node` rotates in its ANNOUNCE).
pub fn prekey_keypair() -> ([u8; 32], [u8; 32]) {
    let mut b = [0u8; 32];
    OsRng.fill_bytes(&mut b);
    let sec = crypto_box::SecretKey::from(b);
    let pubk = *sec.public_key().as_bytes();
    (b, pubk)
}

/// Open a sealed box (`seal`) with a prekey secret. Standalone twin of
/// `Node::open`, for callers holding the secret directly (bindings, tests).
pub fn open_sealed(sealed: &[u8], prekey_sec: &[u8; 32]) -> Option<Vec<u8>> {
    use crypto_box::aead::{generic_array::GenericArray, Aead};
    use crypto_box::{PublicKey, SalsaBox, SecretKey};
    if sealed.len() < 32 {
        return None;
    }
    let sec = SecretKey::from(*prekey_sec);
    let recip_pub = *sec.public_key().as_bytes();
    let mut ep = [0u8; 32];
    ep.copy_from_slice(&sealed[..32]);
    let eph_pub = PublicKey::from(ep);
    let nonce = seal_nonce(&ep, &recip_pub);
    SalsaBox::new(&eph_pub, &sec).decrypt(GenericArray::from_slice(&nonce), &sealed[32..]).ok()
}

// ---------------------------------------------------------------------------
// The node: §5 router + §6 sync, glued together. Transports call `on_rx`.
// ---------------------------------------------------------------------------

pub struct Node {
    pub sk: SigningKey,
    pub addr: Addr,
    prekey_sec: crypto_box::SecretKey,
    pub prekey_pub: [u8; 32],
    pub petname: String,

    pub topics: HashSet<Addr>,
    addrs: HashSet<Addr>,

    seen: HashMap<Id, u32>, // id -> retain-until
    store: HashMap<Id, Stored>,
    paths: Paths,
    peer_prekeys: HashMap<Addr, [u8; 32]>,
    peer_busy: HashMap<Addr, u8>,

    max_store_bytes: usize,
    seq: u64,
    frags: HashMap<Id, Fountain>,
    pub mtu: usize,
    manifests: HashMap<Id, file::Manifest>,
    pending: HashMap<Id, Pending>, // ACKREQ messages awaiting a receipt (§8)
    acked: HashSet<Id>,            // orig ids we've received receipts for
    rpc_pending: HashSet<u64>,     // request ids awaiting a response (L4)
    rpc_responses: HashMap<u64, rpc::Response>,
    rpc_inbox: Vec<(Addr, u64, rpc::Request)>, // requests delivered to a service
    feed_inbox: Vec<feed::Event>,              // feed events on subscribed topics (L5)
    quotas: congestion::Quotas,                // per-source flood quota (§10)
}

struct Pending {
    wire: Vec<u8>,
    backoff: congestion::Backoff,
}

impl Node {
    pub fn new(petname: &str, topics: &[&str]) -> Self {
        let mut seed = [0u8; 32];
        OsRng.fill_bytes(&mut seed);
        let sk = SigningKey::from_bytes(&seed);
        let addr = addr_of(&sk.verifying_key().to_bytes());

        let mut pb = [0u8; 32];
        OsRng.fill_bytes(&mut pb);
        let prekey_sec = crypto_box::SecretKey::from(pb);
        let prekey_pub = *prekey_sec.public_key().as_bytes();

        let mut addrs = HashSet::new();
        addrs.insert(addr);
        Node {
            sk,
            addr,
            prekey_sec,
            prekey_pub,
            petname: petname.to_string(),
            topics: topics.iter().map(|t| topic_of(t)).collect(),
            addrs,
            seen: HashMap::new(),
            store: HashMap::new(),
            paths: Paths::default(),
            peer_prekeys: HashMap::new(),
            peer_busy: HashMap::new(),
            max_store_bytes: 10 * 1024 * 1024,
            seq: 0,
            frags: HashMap::new(),
            mtu: DEFAULT_MTU,
            manifests: HashMap::new(),
            pending: HashMap::new(),
            acked: HashSet::new(),
            rpc_pending: HashSet::new(),
            rpc_responses: HashMap::new(),
            rpc_inbox: Vec::new(),
            feed_inbox: Vec::new(),
            quotas: congestion::Quotas::new(DEFAULT_SOURCE_QUOTA),
        }
    }

    /// Set the per-source flood quota (§10): the sustained bytes/second any single
    /// originating address may have this node store and relay. Stamped mail
    /// bypasses it. Defaults to [`DEFAULT_SOURCE_QUOTA`].
    pub fn set_source_quota(&mut self, bytes_per_sec: u32) {
        self.quotas = congestion::Quotas::new(bytes_per_sec);
    }

    /// Set the store's byte budget. When exceeded, low-priority envelopes are
    /// evicted (lowest stamp → largest → oldest), but chunks of a file still being
    /// assembled are pinned and never dropped. Defaults to 10 MiB.
    pub fn set_store_budget(&mut self, bytes: usize) {
        self.max_store_bytes = bytes.max(1);
        self.enforce_budget();
    }

    pub fn peer_prekey(&self, a: &Addr) -> Option<[u8; 32]> {
        self.peer_prekeys.get(a).copied()
    }
    pub fn open(&self, sealed: &[u8]) -> Option<Vec<u8>> {
        use crypto_box::aead::{generic_array::GenericArray, Aead};
        use crypto_box::{PublicKey, SalsaBox};
        if sealed.len() < 32 {
            return None;
        }
        let mut ep = [0u8; 32];
        ep.copy_from_slice(&sealed[..32]);
        let eph_pub = PublicKey::from(ep);
        let nonce = seal_nonce(&ep, &self.prekey_pub);
        SalsaBox::new(&eph_pub, &self.prekey_sec)
            .decrypt(GenericArray::from_slice(&nonce), &sealed[32..])
            .ok()
    }

    // ---- origination -----------------------------------------------------

    fn store_put(&mut self, e: &Envelope) {
        let id = e.id();
        self.store.insert(
            id,
            Stored { wire: e.wire(), expiry: e.expiry, stamp: e.stamp(), seq: self.seq, dest: e.dest },
        );
        self.seq += 1;
        self.enforce_budget();
    }
    fn enforce_budget(&mut self) {
        let mut total: usize = self.store.values().map(|s| s.wire.len()).sum();
        if total <= self.max_store_bytes {
            return;
        }
        // Pin the chunks (and manifest) of any file we're still assembling, so
        // memory pressure never drops a chunk we're actively collecting and
        // stalls the fetch forever. Completed files are unpinned and evictable.
        let pinned = self.pinned_ids();
        // evict order: lowest stamp -> largest -> oldest (smallest seq)
        while total > self.max_store_bytes {
            let victim = self
                .store
                .iter()
                .filter(|(k, _)| !pinned.contains(*k))
                .min_by(|a, b| {
                    a.1.stamp
                        .cmp(&b.1.stamp)
                        .then(b.1.wire.len().cmp(&a.1.wire.len()))
                        .then(a.1.seq.cmp(&b.1.seq))
                })
                .map(|(k, _)| *k);
            match victim {
                Some(k) => {
                    total -= self.store[&k].wire.len();
                    self.store.remove(&k);
                }
                None => break, // only in-progress file chunks remain — keep them
            }
        }
    }

    /// Content IDs that must not be evicted: the manifest and already-collected
    /// chunks of every file we hold a manifest for but haven't completed yet.
    fn pinned_ids(&self) -> HashSet<Id> {
        let mut pinned = HashSet::new();
        for (magnet, m) in &self.manifests {
            if self.has_file(magnet) {
                continue; // complete — its chunks may be evicted/re-fetched
            }
            pinned.insert(*magnet);
            for c in &m.chunk_ids {
                if self.store.contains_key(c) {
                    pinned.insert(*c);
                }
            }
        }
        pinned
    }

    /// Originate a signed public (flooded) message on topic/broadcast `dest`.
    pub fn originate(&mut self, dest: Addr, payload: Vec<u8>, now: u32) -> Vec<Forward> {
        let mut e = Envelope::new(ty::DATA, dest, now + 7 * 86400, payload);
        // Topics and public floods carry FLOOD; the relay uses this flag (not
        // structure) to tell multicast from unicast (§5).
        if dest == ZERO_DEST || self.topics.contains(&dest) {
            e.flags |= fl::FLOOD;
        }
        e.sign(&self.sk);
        // Unicast with no known path: flood to discover it (§5.6).
        if e.flags & fl::FLOOD == 0 && self.paths.fresh(&dest, now).is_none() {
            e.flags |= fl::FLOOD;
            e.sign(&self.sk);
        }
        self.mark_seen(&e);
        self.store_put(&e);
        self.forward_intents(&e, NO_IFACE, now)
    }

    /// High-level send: deliver `data` of *any* size to an address or topic.
    ///
    /// Small payloads ride a single signed envelope (identical to `originate`).
    /// Anything larger than `self.mtu` is fountain-fragmented (§3) into equal
    /// chunks plus a margin of repair chunks, so it survives lossy, reordered,
    /// even one-way delivery; the receiver reassembles and verifies the original
    /// signature before the app ever sees it. Callers never think about MTUs.
    ///
    /// One fountain set caps at ~`mtu`×255 (≈ 50 KB at defaults); larger objects
    /// belong to the manifest+swarm layer (files), not a single `send`.
    pub fn send(&mut self, dest: Addr, data: Vec<u8>, now: u32) -> Vec<Forward> {
        let mut e = Envelope::new(ty::DATA, dest, now + 7 * 86400, data);
        if dest == ZERO_DEST || self.topics.contains(&dest) {
            e.flags |= fl::FLOOD;
        }
        e.sign(&self.sk);
        // Unicast with no known path: flood to discover it (§5.6).
        if e.flags & fl::FLOOD == 0 && self.paths.fresh(&dest, now).is_none() {
            e.flags |= fl::FLOOD;
            e.sign(&self.sk);
        }

        let wire = e.wire();
        if wire.len() <= self.mtu {
            self.mark_seen(&e);
            self.store_put(&e);
            return self.forward_intents(&e, NO_IFACE, now);
        }

        // Too big for one envelope: fountain-fragment the signed wire form.
        let chunk = self.mtu.saturating_sub(FRAG_OVERHEAD).max(1);
        let count = wire.len().div_ceil(chunk);
        assert!(
            count <= 255,
            "object too large for one fountain set (~mtu×255); use the file/manifest layer"
        );
        let orig_id = e.id();
        // Data chunks 0..count, then a few repair chunks for loss resilience.
        let repair = (count / 8 + 2).min(255 - count);
        let indices: Vec<u8> = (0..(count + repair)).map(|i| i as u8).collect();
        let frags = fragment(&wire, chunk, e.hops, e.expiry, dest, orig_id, &indices);

        let mut forwards = Vec::new();
        for fr in &frags {
            self.mark_seen(fr);
            self.store_put(fr);
            forwards.append(&mut self.forward_intents(fr, NO_IFACE, now));
        }
        forwards
    }

    /// Originate a unicast message that asks the recipient for a delivery
    /// receipt (§8). Tracks it for backoff resend until a receipt arrives.
    pub fn originate_ackreq(&mut self, dest: Addr, payload: Vec<u8>, now: u32) -> Vec<Forward> {
        let mut e = Envelope::new(ty::DATA, dest, now + 7 * 86400, payload);
        e.flags |= fl::ACKREQ;
        if dest == ZERO_DEST || self.topics.contains(&dest) {
            e.flags |= fl::FLOOD;
        }
        e.sign(&self.sk);
        if e.flags & fl::FLOOD == 0 && self.paths.fresh(&dest, now).is_none() {
            e.flags |= fl::FLOOD;
            e.sign(&self.sk);
        }
        let id = e.id();
        self.mark_seen(&e);
        self.store_put(&e);
        self.pending.insert(id, Pending { wire: e.wire(), backoff: congestion::Backoff::new(now) });
        self.forward_intents(&e, NO_IFACE, now)
    }

    /// Has a receipt for `id` come back?
    pub fn acked(&self, id: &Id) -> bool {
        self.acked.contains(id)
    }

    /// Resend any ACKREQ messages whose backoff has elapsed without a receipt
    /// (§5.6: flooding is route discovery). Drops exhausted or acked ones.
    pub fn resend_unacked(&mut self, now: u32) -> Vec<Forward> {
        let mut out = Vec::new();
        let mut done = Vec::new();
        for (id, p) in self.pending.iter_mut() {
            if self.acked.contains(id) || p.backoff.exhausted() {
                done.push(*id);
                continue;
            }
            if p.backoff.due(now) {
                p.backoff.fired(now);
                out.push(Forward::Flood { except: NO_IFACE, bytes: p.wire.clone() });
            }
        }
        for id in done {
            self.pending.remove(&id);
        }
        out
    }

    // ---- L4 request/response (RPC) --------------------------------------

    /// Call a service (an address or a served topic). Returns the request id
    /// (to match the reply) and the `Forward`s to send. The reply arrives via
    /// `take_response`.
    pub fn request(&mut self, service: Addr, req: rpc::Request, now: u32) -> (u64, Vec<Forward>) {
        let mut idb = [0u8; 8];
        OsRng.fill_bytes(&mut idb);
        let id = u64::from_be_bytes(idb);
        let payload = rpc::encode_request(id, &req);
        let mut e = Envelope::new(ty::DATA, service, now + 7 * 86400, payload);
        if service == ZERO_DEST || self.topics.contains(&service) {
            e.flags |= fl::FLOOD;
        }
        e.sign(&self.sk);
        if e.flags & fl::FLOOD == 0 && self.paths.fresh(&service, now).is_none() {
            e.flags |= fl::FLOOD;
            e.sign(&self.sk);
        }
        self.rpc_pending.insert(id);
        self.mark_seen(&e);
        self.store_put(&e);
        (id, self.forward_intents(&e, NO_IFACE, now))
    }

    /// Drain requests delivered to us as a service: `(requester, req_id, request)`.
    pub fn poll_requests(&mut self) -> Vec<(Addr, u64, rpc::Request)> {
        std::mem::take(&mut self.rpc_inbox)
    }

    /// Reply to a request, routed back toward the requester.
    pub fn respond(&mut self, to: Addr, req_id: u64, resp: rpc::Response, now: u32) -> Vec<Forward> {
        let payload = rpc::encode_response(req_id, &resp);
        let mut e = Envelope::new(ty::DATA, to, now + 7 * 86400, payload);
        e.sign(&self.sk);
        if self.paths.fresh(&to, now).is_none() {
            e.flags |= fl::FLOOD; // reverse path unknown -> flood to find it
            e.sign(&self.sk);
        }
        self.mark_seen(&e);
        self.store_put(&e);
        self.forward_intents(&e, NO_IFACE, now)
    }

    /// Take the response to `id` if it has arrived.
    pub fn take_response(&mut self, id: u64) -> Option<rpc::Response> {
        self.rpc_responses.remove(&id)
    }

    // ---- L5 feeds (pub/sub) ---------------------------------------------

    /// Follow a feed topic so its events are delivered to us.
    pub fn subscribe(&mut self, topic: &str) {
        self.topics.insert(topic_of(topic));
    }

    /// Publish an event to a feed topic (floods to all subscribers).
    pub fn publish(&mut self, topic: &str, event: Vec<u8>, now: u32) -> Vec<Forward> {
        let mut payload = Vec::with_capacity(1 + event.len());
        payload.push(feed::FEED_TAG);
        payload.extend_from_slice(&event);
        let mut e = Envelope::new(ty::DATA, topic_of(topic), now + 7 * 86400, payload);
        e.flags |= fl::FLOOD;
        e.sign(&self.sk);
        self.mark_seen(&e);
        self.store_put(&e);
        self.forward_intents(&e, NO_IFACE, now)
    }

    /// Drain feed events received on subscribed topics.
    pub fn poll_feed(&mut self) -> Vec<feed::Event> {
        std::mem::take(&mut self.feed_inbox)
    }

    /// Our current backpressure `busy` byte (§5.4c): store fill scaled to 0–255.
    /// Neighbours use it to throttle relays toward a swamped peer.
    pub fn busy(&self) -> u8 {
        let used: usize = self.store.values().map(|s| s.wire.len()).sum();
        (used.saturating_mul(255) / self.max_store_bytes.max(1)).min(255) as u8
    }
    /// The `busy` byte a peer last advertised in its ANNOUNCE, if heard.
    pub fn peer_busy(&self, a: &Addr) -> Option<u8> {
        self.peer_busy.get(a).copied()
    }

    /// Build+sign this node's ANNOUNCE (prekey + busy + topics), ready to flood (§4).
    pub fn build_announce(&mut self, now: u32) -> Vec<Forward> {
        let mut p = Vec::new();
        p.extend_from_slice(&self.prekey_pub);
        p.push(self.busy()); // §5.4c backpressure
        p.push(self.topics.len() as u8);
        for t in &self.topics {
            p.extend_from_slice(t);
        }
        p.push(0); // np: we advertise no distant paths in this reference build
        p.extend_from_slice(self.petname.as_bytes());
        let mut e = Envelope::new(ty::ANNOUNCE, ZERO_DEST, now + 3600, p);
        e.flags |= fl::FLOOD;
        e.sign(&self.sk);
        self.mark_seen(&e);
        self.store_put(&e);
        self.forward_intents(&e, NO_IFACE, now)
    }

    fn absorb_announce(&mut self, e: &Envelope) {
        if !e.verify() {
            return;
        }
        let Src::Full(pk) = &e.src else { return };
        if e.payload.len() < 33 {
            return;
        }
        let src_addr = addr_of(pk);
        let mut prekey = [0u8; 32];
        prekey.copy_from_slice(&e.payload[..32]);
        self.peer_prekeys.insert(src_addr, prekey);
        self.peer_busy.insert(src_addr, e.payload[32]); // §5.4c busy byte
                                                        // (topics/petname parsed here in a fuller build; omitted for brevity)
    }

    // ---- receive (the entire router, §5) --------------------------------

    fn mark_seen(&mut self, e: &Envelope) {
        let retain = e.expiry.max(0u32.wrapping_add(SEEN_MIN_SECS)); // >= expiry
        self.seen.insert(e.id(), retain);
    }

    pub fn on_rx(&mut self, raw: &[u8], iface: Iface, nbr: Option<Addr>, now: u32) -> Rx {
        let Ok((e, _)) = Envelope::decode(raw) else {
            return Rx::default();
        };

        // INV/WANT are per-link, hops=0, consumed on receipt — never stored,
        // never deduped, never relayed (§6).
        match e.typ {
            ty::INV => return self.on_inv(&e, iface, nbr),
            ty::WANT => return self.on_want(&e, iface, nbr),
            _ => {}
        }
        self.ingest(&e, iface, nbr, now, true)
    }

    /// The router core (§5). `allow_forward` is true for envelopes off the wire
    /// and false for an original recombined from fragments — its chunks already
    /// propagate on their own, so the giant reassembled copy must not re-flood.
    fn ingest(&mut self, e: &Envelope, iface: Iface, nbr: Option<Addr>, now: u32, allow_forward: bool) -> Rx {
        let id = e.id();
        if self.seen.contains_key(&id) || e.expiry < now {
            return Rx::default(); // duplicate or expired -> drop
        }
        self.seen.insert(id, e.expiry.max(now + SEEN_MIN_SECS));

        // Path learning: the first copy of a signed envelope raced every route
        // and won, so its src is reachable via the interface that delivered it.
        if e.flags & fl::SIGNED != 0 {
            if let Src::Full(pk) = &e.src {
                self.paths.learn(addr_of(pk), iface, nbr, now);
            }
        }

        // Per-source flood quota (§10): charge this envelope against its origin's
        // byte budget. Over budget, we still deliver it locally if it's for us,
        // but we do not amplify it — no reassembly hoarding, no store, no relay.
        let src_addr = match &e.src {
            Src::Full(pk) => Some(addr_of(pk)),
            Src::Short(a) => Some(*a),
            Src::None => None,
        };
        let within_quota = match src_addr {
            Some(a) => self.quotas.admit(a, e.wire().len() as u32, e.stamp(), now),
            None => true, // unattributable frames: dedup/expiry already bound them
        };

        let mut rx = Rx::default();

        if e.typ == ty::ANNOUNCE {
            self.absorb_announce(e);
        }

        let deliverable =
            self.addrs.contains(&e.dest) || self.topics.contains(&e.dest) || e.dest == ZERO_DEST;

        // Deliver to the app — but a raw FRAGMENT is transport plumbing, never
        // app data; delivery of a fragmented object happens on reassembly below.
        if deliverable && e.flags & fl::FRAGMENT == 0 {
            rx.delivered.push(e.clone());
        }

        // Auto-learn manifests addressed to us (endpoint demux on the app tag).
        if deliverable && e.typ == ty::DATA && e.payload.first() == Some(&file::MANIFEST_TAG) {
            self.absorb_manifest(e);
        }

        // L4/L5 endpoint demux: queue requests/feed events, match responses.
        if deliverable && e.typ == ty::DATA && e.flags & fl::FRAGMENT == 0 {
            match e.payload.first().copied() {
                Some(rpc::REQUEST_TAG) => {
                    if let Src::Full(pk) = &e.src {
                        if let Some((id, req)) = rpc::decode_request(&e.payload) {
                            self.rpc_inbox.push((addr_of(pk), id, req));
                        }
                    }
                }
                Some(rpc::RESPONSE_TAG) => {
                    if let Some((id, resp)) = rpc::decode_response(&e.payload) {
                        if self.rpc_pending.remove(&id) {
                            self.rpc_responses.insert(id, resp);
                        }
                    }
                }
                Some(feed::FEED_TAG) => {
                    let from = match &e.src {
                        Src::Full(pk) => Some(addr_of(pk)),
                        _ => None,
                    };
                    self.feed_inbox.push(feed::Event { topic: e.dest, from, data: e.payload[1..].to_vec() });
                }
                _ => {}
            }
        }

        // Receipts (§8), only for mail addressed specifically to one of our
        // addresses (never for topic/public floods).
        if e.typ == ty::DATA && self.addrs.contains(&e.dest) {
            // A receipt for something we sent -> record the delivery.
            if e.flags & fl::ACKREQ == 0 && e.payload.first() == Some(&RECEIPT_TAG) && e.payload.len() >= 17 {
                let mut oid = [0u8; 16];
                oid.copy_from_slice(&e.payload[1..17]);
                self.acked.insert(oid);
                self.pending.remove(&oid);
            }
            // A message that asked for a receipt -> flood one back to its src.
            if e.flags & fl::ACKREQ != 0 && e.flags & fl::FRAGMENT == 0 {
                if let Src::Full(pk) = &e.src {
                    let mut p = Vec::with_capacity(17);
                    p.push(RECEIPT_TAG);
                    p.extend_from_slice(&e.id());
                    let mut ack = Envelope::new(ty::DATA, addr_of(pk), e.expiry, p);
                    ack.flags |= fl::FLOOD; // receipts flood and teach reverse paths
                    ack.sign(&self.sk);
                    self.mark_seen(&ack);
                    self.store_put(&ack);
                    rx.forwards.append(&mut self.forward_intents(&ack, iface, now));
                }
            }
        }

        // Reassemble only objects bound for us; a pure relay just forwards the
        // fragments (each is an ordinary envelope) without hoarding chunks. A
        // source over its quota can't make us hoard its chunks either.
        if within_quota && deliverable && e.flags & fl::FRAGMENT != 0 && e.payload.len() >= 18 {
            let mut oid = [0u8; 16];
            oid.copy_from_slice(&e.payload[..16]);
            let idx = e.payload[16];
            let count = e.payload[17];
            let chunk = e.payload[18..].to_vec();
            if let Some(orig) = self.frags.entry(oid).or_default().add(&oid, idx, count, chunk) {
                if let Ok((oe, _)) = Envelope::decode(&orig) {
                    // Deliver the recombined original; do not re-forward it.
                    let mut inner = self.ingest(&oe, iface, nbr, now, false);
                    rx.delivered.append(&mut inner.delivered);
                    rx.forwards.append(&mut inner.forwards);
                }
            }
        }

        // Store + relay only within the source's quota — this is the mesh-load
        // that §10 caps. Local delivery above already happened regardless.
        if within_quota {
            // Store for later opportunistic sync.
            self.store_put(e);

            // Relay.
            if allow_forward && e.hops > 0 {
                let mut f = e.clone();
                f.hops -= 1;
                rx.forwards.append(&mut self.forward_intents(&f, iface, now));
            }
        }
        rx
    }

    fn forward_intents(&self, e: &Envelope, except: Iface, now: u32) -> Vec<Forward> {
        let bytes = e.wire();
        // FLOOD flag or public dest -> epidemic flood. Otherwise unicast: use a
        // fresh path if we have one, else stay silent (discovery is the
        // originator's job, §5.6).
        if e.flags & fl::FLOOD != 0 || e.dest == ZERO_DEST {
            vec![Forward::Flood { except, bytes }]
        } else if let Some(p) = self.paths.fresh(&e.dest, now) {
            vec![Forward::Directed { iface: p.iface, nbr: p.nbr, bytes }]
        } else {
            Vec::new()
        }
    }

    // ---- sync (§6) -------------------------------------------------------

    /// INV = concatenated 16-byte IDs of stored envelopes relevant to a peer
    /// that follows `peer_topics` (public + those topics + unicast for custody).
    pub fn build_inv(&self, peer_topics: &HashSet<Addr>) -> Vec<u8> {
        let mut ids: Vec<(&Id, &Stored)> = self.store.iter().collect();
        ids.sort_by(|a, b| b.1.expiry.cmp(&a.1.expiry)); // newest first
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

    fn on_inv(&self, e: &Envelope, iface: Iface, nbr: Option<Addr>) -> Rx {
        let mut want = Vec::new();
        for chunk in e.payload.chunks(16) {
            if chunk.len() == 16 {
                let mut id = [0u8; 16];
                id.copy_from_slice(chunk);
                if !self.store.contains_key(&id) {
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

    fn on_want(&self, e: &Envelope, iface: Iface, nbr: Option<Addr>) -> Rx {
        let mut rx = Rx::default();
        for chunk in e.payload.chunks(16) {
            if chunk.len() == 16 {
                let mut id = [0u8; 16];
                id.copy_from_slice(chunk);
                if let Some(s) = self.store.get(&id) {
                    rx.forwards.push(Forward::Directed { iface, nbr, bytes: s.wire.clone() });
                }
            }
        }
        rx
    }

    pub fn store_len(&self) -> usize {
        self.store.len()
    }
    pub fn has(&self, id: &Id) -> bool {
        self.store.contains_key(id)
    }

    /// All stored envelope IDs, concatenated (16 B each) — a bag INV.
    pub fn stored_ids(&self) -> Vec<u8> {
        let mut v = Vec::with_capacity(self.store.len() * 16);
        for id in self.store.keys() {
            v.extend_from_slice(id);
        }
        v
    }
    /// The wire bytes of a stored envelope, if held.
    pub fn get_wire(&self, id: &Id) -> Option<Vec<u8>> {
        self.store.get(id).map(|s| s.wire.clone())
    }
    /// Every stored envelope as `(id, wire)` — the whole bag.
    pub fn store_wires(&self) -> Vec<(Id, Vec<u8>)> {
        self.store.iter().map(|(id, s)| (*id, s.wire.clone())).collect()
    }

    // ---- datagram sessions (§ application layer, tag 0x04) ---------------

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

    /// Publish `bytes` as a content-addressed file. Splits it into chunk
    /// envelopes (each addressed by its own content ID), stores them, and builds
    /// a signed manifest that lists those IDs. Returns the manifest ID — the
    /// **magnet** — and the `Forward`s to flood the small manifest. The data
    /// itself is pulled on demand (§6 custody / swarm), BitTorrent-style.
    pub fn publish_file(&mut self, name: &str, bytes: &[u8], dest: Addr, now: u32) -> (Id, Vec<Forward>) {
        let chunk_size = self.mtu.saturating_sub(64).max(1);
        let count = bytes.len().div_ceil(chunk_size).max(1);
        let expiry = now + 7 * 86400;
        let mut file_id = [0u8; 16];
        OsRng.fill_bytes(&mut file_id);
        // Chunks ride a per-file topic so only interested nodes carry them.
        let mut ft = [0u8; 8];
        ft.copy_from_slice(&Sha256::digest(file_id)[..8]);

        let mut chunk_ids = Vec::with_capacity(count);
        for i in 0..count {
            let start = i * chunk_size;
            let end = ((i + 1) * chunk_size).min(bytes.len());
            let mut payload = Vec::with_capacity(21 + (end - start));
            payload.push(file::CHUNK_TAG);
            payload.extend_from_slice(&file_id);
            payload.extend_from_slice(&(i as u32).to_be_bytes());
            payload.extend_from_slice(&bytes[start..end]);
            let mut ce = Envelope::new(ty::DATA, ft, expiry, payload);
            ce.flags |= fl::FLOOD;
            chunk_ids.push(ce.id());
            self.mark_seen(&ce);
            self.store_put(&ce);
        }

        let manifest = file::Manifest {
            file_id,
            chunk_size: chunk_size as u32,
            count: count as u32,
            total_len: bytes.len() as u64,
            name: name.to_string(),
            chunk_ids,
        };
        let mut me = Envelope::new(ty::DATA, dest, expiry, manifest.encode());
        if dest == ZERO_DEST || self.topics.contains(&dest) {
            me.flags |= fl::FLOOD;
        }
        me.sign(&self.sk);
        let magnet = me.id();
        self.manifests.insert(magnet, manifest);
        self.mark_seen(&me);
        self.store_put(&me);
        let forwards = self.forward_intents(&me, NO_IFACE, now);
        (magnet, forwards)
    }

    /// Register a manifest we received. Called automatically on delivery; also
    /// usable directly. Verifies the signature before trusting the chunk list.
    pub fn absorb_manifest(&mut self, e: &Envelope) -> Option<Id> {
        if !e.verify() {
            return None;
        }
        let m = file::Manifest::decode(&e.payload)?;
        let magnet = e.id();
        self.manifests.entry(magnet).or_insert(m);
        Some(magnet)
    }

    /// Ask neighbours for the chunks of `magnet` we don't hold yet. Reuses the
    /// WANT machinery: a chunk is an ordinary stored envelope, named by content,
    /// so any peer that has it answers from its store.
    pub fn fetch(&mut self, magnet: &Id) -> Vec<Forward> {
        let Some(m) = self.manifests.get(magnet) else {
            return Vec::new();
        };
        let mut want = Vec::new();
        for cid in &m.chunk_ids {
            if !self.store.contains_key(cid) {
                want.extend_from_slice(cid);
            }
        }
        if want.is_empty() {
            return Vec::new();
        }
        let bytes = Envelope::new(ty::WANT, ZERO_DEST, 0, want).wire();
        vec![Forward::Flood { except: NO_IFACE, bytes }]
    }

    /// Request the chunks of every manifest we know but don't yet hold — the
    /// subscriber half of folder sync.
    pub fn fetch_all(&mut self) -> Vec<Forward> {
        let magnets: Vec<Id> = self.manifests.keys().copied().collect();
        let mut out = Vec::new();
        for m in magnets {
            out.append(&mut self.fetch(&m));
        }
        out
    }

    /// Every complete file we hold, as `(name, bytes)`, newest manifest per name
    /// winning (by envelope expiry). Drives materialising a synced folder.
    pub fn complete_files(&self) -> Vec<(String, Vec<u8>)> {
        let mut best: HashMap<String, (Id, u32)> = HashMap::new();
        for (magnet, m) in &self.manifests {
            if !self.has_file(magnet) {
                continue;
            }
            let exp = self.store.get(magnet).map(|s| s.expiry).unwrap_or(0);
            best.entry(m.name.clone())
                .and_modify(|(id, e)| {
                    if exp > *e {
                        *id = *magnet;
                        *e = exp;
                    }
                })
                .or_insert((*magnet, exp));
        }
        best.into_iter()
            .filter_map(|(name, (magnet, _))| self.file_bytes(&magnet).map(|b| (name, b)))
            .collect()
    }

    /// True once every chunk named by the manifest is in our store.
    pub fn has_file(&self, magnet: &Id) -> bool {
        match self.manifests.get(magnet) {
            Some(m) => m.chunk_ids.iter().all(|c| self.store.contains_key(c)),
            None => false,
        }
    }

    /// Reassemble the file, or `None` if a chunk is still missing. Every chunk
    /// is content-verified for free: we only count it as present if the store
    /// holds an envelope whose ID equals the one the signed manifest named.
    pub fn file_bytes(&self, magnet: &Id) -> Option<Vec<u8>> {
        let m = self.manifests.get(magnet)?;
        let mut out = Vec::with_capacity(m.total_len as usize);
        for cid in &m.chunk_ids {
            let s = self.store.get(cid)?;
            let (ce, _) = Envelope::decode(&s.wire).ok()?;
            if ce.payload.len() < 21 {
                return None; // not a well-formed chunk
            }
            out.extend_from_slice(&ce.payload[21..]);
        }
        out.truncate(m.total_len as usize);
        Some(out)
    }

    // ---- mix mode (§9) ---------------------------------------------------

    /// If `e` is an onion layer sealed to us, peel it: open the payload, confirm
    /// the `'O'` marker, and return the inner envelope's wire bytes (padding
    /// stripped by self-delimiting decode). A mix re-injects this as its own
    /// traffic. `None` if it isn't an onion for us.
    pub fn onion_peel(&self, e: &Envelope) -> Option<Vec<u8>> {
        let opened = self.open(&e.payload)?;
        if opened.first() != Some(&mix::ONION_TAG) {
            return None;
        }
        let (_, n) = Envelope::decode(&opened[1..]).ok()?;
        Some(opened[1..1 + n].to_vec())
    }
}

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
// Page 2, rule 2 — KISS framing for byte streams (TCP, serial, RFCOMM, TNCs).
// ---------------------------------------------------------------------------

pub mod kiss;

// ---------------------------------------------------------------------------
// Page 2, rule 3 — text-channel armor (SMS, email, Usenet, paper, voice).
// ~S1.<base32(env)>.<base32(sha256(env)[..4])>~
// ---------------------------------------------------------------------------

pub mod armor;

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

// Bridges — SPORE rides everything (spec Page 2). See `src/bridge/` for the
// per-medium modules; each only moves envelope bytes in and out of a `Node`.
pub mod bridge;

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
        let f = a.send(topic_of("news"), b"hi".to_vec(), now);
        assert_eq!(f.len(), 1, "a small payload must not fragment");
    }

    #[test]
    fn send_large_object_fragments_and_reassembles() {
        let now = 1_700_000_000;
        let mut a = Node::new("a", &["news"]);
        let mut b = Node::new("b", &["news"]);

        let payload = vec![0x5Au8; 5000]; // well over one MTU
        let forwards = a.send(topic_of("news"), payload.clone(), now);
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
        let forwards = a.send(topic_of("news"), vec![0x11u8; 5000], now);

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
        let (a_sec, a_pub) = ratchet::keypair();
        let (b_sec, b_pub) = ratchet::keypair();
        let mut alice = ratchet::Ratchet::init_alice(a_sec, b_pub);
        let mut bob = ratchet::Ratchet::init_bob(b_sec, b_pub, a_pub);

        // Alice -> Bob (bootstraps Bob's receiving chain).
        let m1 = alice.encrypt(b"hello bob");
        assert_eq!(bob.decrypt(&m1).as_deref(), Some(&b"hello bob"[..]));

        // Bob -> Alice (turns the ratchet in both directions).
        let r1 = bob.encrypt(b"hi alice");
        assert_eq!(alice.decrypt(&r1).as_deref(), Some(&b"hi alice"[..]));

        // Alice -> Bob, delivered out of order: the later one first.
        let m2 = alice.encrypt(b"one");
        let m3 = alice.encrypt(b"two");
        assert_eq!(bob.decrypt(&m3).as_deref(), Some(&b"two"[..]), "arrives first");
        assert_eq!(bob.decrypt(&m2).as_deref(), Some(&b"one"[..]), "skipped key recovers it");

        // A replay of an already-consumed message is rejected.
        assert!(bob.decrypt(&m1).is_none(), "replay rejected");
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

        // (c) Backpressure: idle admits all; busy drops unstamped; stamped rides.
        assert!(admit(0, 0, 200));
        assert!(!admit(255, 0, 100));
        assert!(admit(255, 3, 100), "stamped mail is always admitted");
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
