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
/// Bytes a fragment envelope adds around its chunk: 16 header + 2 plen +
/// 16 orig_id + 1 index + 1 count. `chunk = mtu - FRAG_OVERHEAD`.
const FRAG_OVERHEAD: usize = 36;

/// Datagrams are ephemeral: a short expiry keeps interactive session traffic
/// out of anyone's long-term store.
pub const SESSION_EXPIRY_SECS: u32 = 300;

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
            if off + n <= buf.len() { Ok(()) } else { std::result::Result::Err(Err::Short) }
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
    let count = (env_wire.len() + chunk - 1) / chunk;
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
        BitVec { words: vec![0u64; (len + 63) / 64] }
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
        self.map
            .get(a)?
            .iter()
            .find(|p| now.saturating_sub(p.age) < PATH_FRESH_SECS)
            .cloned()
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
    let ct = SalsaBox::new(&their, &eph)
        .encrypt(GenericArray::from_slice(&nonce), msg)
        .expect("seal");
    let mut out = Vec::with_capacity(32 + ct.len());
    out.extend_from_slice(eph_pub.as_bytes());
    out.extend_from_slice(&ct);
    out
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

    max_store_bytes: usize,
    seq: u64,
    frags: HashMap<Id, Fountain>,
    pub mtu: usize,
    manifests: HashMap<Id, file::Manifest>,
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
            max_store_bytes: 10 * 1024 * 1024,
            seq: 0,
            frags: HashMap::new(),
            mtu: DEFAULT_MTU,
            manifests: HashMap::new(),
        }
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
        // evict order: lowest stamp -> largest -> oldest (smallest seq)
        while total > self.max_store_bytes {
            let victim = self
                .store
                .iter()
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
                None => break,
            }
        }
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
        let count = (wire.len() + chunk - 1) / chunk;
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

    /// Build+sign this node's ANNOUNCE (prekey + topics), ready to flood (§4).
    pub fn build_announce(&mut self, now: u32) -> Vec<Forward> {
        let mut p = Vec::new();
        p.extend_from_slice(&self.prekey_pub);
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

        // Reassemble only objects bound for us; a pure relay just forwards the
        // fragments (each is an ordinary envelope) without hoarding chunks.
        if deliverable && e.flags & fl::FRAGMENT != 0 && e.payload.len() >= 18 {
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

        // Store for later opportunistic sync.
        self.store_put(e);

        // Relay.
        if allow_forward && e.hops > 0 {
            let mut f = e.clone();
            f.hops -= 1;
            rx.forwards.append(&mut self.forward_intents(&f, iface, now));
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
            let relevant = s.dest == ZERO_DEST
                || peer_topics.contains(&s.dest)
                || !self.topics.contains(&s.dest); // unicast -> carry (custody)
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
        let count = ((bytes.len() + chunk_size - 1) / chunk_size).max(1);
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

pub mod session {
    use super::*;

    /// Application tag marking a datagram payload (§ application layer).
    pub const TAG_DGRAM: u8 = 0x04;

    /// One end of a UDP-like link. Pure local state: peer address, port, the
    /// peer's prekey (to seal to), a TX counter and a 64-wide replay window.
    pub struct Session {
        me: Addr,
        peer: Addr,
        port: u16,
        peer_prekey: [u8; 32],
        tx_seq: u64,
        rx_hi: u64,
        rx_win: u64,
    }

    impl Session {
        pub fn new(me: Addr, peer: Addr, port: u16, peer_prekey: [u8; 32]) -> Self {
            Session { me, peer, port, peer_prekey, tx_seq: 0, rx_hi: 0, rx_win: 0 }
        }
        pub fn me(&self) -> Addr {
            self.me
        }
        pub fn peer(&self) -> Addr {
            self.peer
        }
        pub fn port(&self) -> u16 {
            self.port
        }
        pub fn peer_prekey(&self) -> [u8; 32] {
            self.peer_prekey
        }
        /// Next outbound sequence (1-based; 0 means "nothing sent/seen").
        pub fn next_tx_seq(&mut self) -> u64 {
            self.tx_seq += 1;
            self.tx_seq
        }
        /// DTLS-style sliding replay window over the last 64 sequences. Returns
        /// false for a replayed or too-old datagram; true (and records it) for a
        /// fresh one.
        pub fn accept_rx(&mut self, seq: u64) -> bool {
            const W: u64 = 64;
            if seq == 0 {
                return false;
            }
            if self.rx_hi == 0 {
                self.rx_hi = seq;
                self.rx_win = 1; // bit 0 == rx_hi seen
                return true;
            }
            if seq > self.rx_hi {
                let shift = seq - self.rx_hi;
                self.rx_win = if shift >= W { 1 } else { (self.rx_win << shift) | 1 };
                self.rx_hi = seq;
                return true;
            }
            let diff = self.rx_hi - seq;
            if diff >= W {
                return false; // fell off the window
            }
            let bit = 1u64 << diff;
            if self.rx_win & bit != 0 {
                return false; // replay
            }
            self.rx_win |= bit;
            true
        }
    }

    // Reliable-stream frames, carried inside a datagram's sealed payload.
    const F_DATA: u8 = 0x00; // [0x00][offset:8][len:2][bytes]
    const F_ACK: u8 = 0x01; //  [0x01][recv_next:8]

    fn data_frame(offset: u64, bytes: &[u8]) -> Vec<u8> {
        let mut f = Vec::with_capacity(11 + bytes.len());
        f.push(F_DATA);
        f.extend_from_slice(&offset.to_be_bytes());
        f.extend_from_slice(&(bytes.len() as u16).to_be_bytes());
        f.extend_from_slice(bytes);
        f
    }
    fn ack_frame(recv_next: u64) -> Vec<u8> {
        let mut f = Vec::with_capacity(9);
        f.push(F_ACK);
        f.extend_from_slice(&recv_next.to_be_bytes());
        f
    }

    /// A simple QUIC-style reliable, ordered byte stream over a `Session`.
    ///
    /// Go-Back-N: the sender streams `F_DATA` frames within a fixed window and,
    /// on an ACK-progress timeout, rewinds to the last acknowledged offset and
    /// resends. The receiver accepts only in-order bytes and cumulatively ACKs
    /// the next offset it needs. No fancy congestion control — a fixed window and
    /// a fixed retransmit timeout, on purpose.
    pub struct Reliable {
        s: Session,
        // send side
        send_base: u64, // absolute offset of the first unacked byte
        send_next: u64, // absolute offset of the next byte to put on the wire
        out: Vec<u8>,   // buffered bytes; out[0] is byte at absolute send_base
        last_progress: u32,
        // recv side
        recv_next: u64,
        inbox: Vec<u8>, // delivered, in-order, awaiting read()
        // params
        max_frame: usize,
        window: usize,
        rto: u32,
    }

    impl Reliable {
        pub fn new(s: Session, max_frame: usize) -> Self {
            Reliable {
                s,
                send_base: 0,
                send_next: 0,
                out: Vec::new(),
                last_progress: 0,
                recv_next: 0,
                inbox: Vec::new(),
                max_frame: max_frame.max(1),
                window: max_frame.max(1) * 8,
                rto: 1,
            }
        }
        pub fn session(&self) -> &Session {
            &self.s
        }

        /// Queue bytes and send whatever the window allows now.
        pub fn write(&mut self, node: &mut Node, data: &[u8], now: u32) -> Vec<Forward> {
            self.out.extend_from_slice(data);
            self.flush(node, now)
        }

        /// Hand an inbound datagram envelope to the stream. Decrypts it, applies
        /// the frame, and returns any ACK or windowed sends that result.
        pub fn deliver(&mut self, node: &mut Node, e: &Envelope, now: u32) -> Vec<Forward> {
            match node.dg_recv(&mut self.s, e) {
                Some(frame) => self.on_frame(node, &frame, now),
                None => Vec::new(),
            }
        }

        /// Drain the in-order bytes delivered so far.
        pub fn read(&mut self) -> Vec<u8> {
            std::mem::take(&mut self.inbox)
        }

        /// Drive retransmission timers. Call periodically with a monotonic `now`.
        pub fn poll(&mut self, node: &mut Node, now: u32) -> Vec<Forward> {
            if self.send_next > self.send_base && now.saturating_sub(self.last_progress) >= self.rto {
                self.send_next = self.send_base; // Go-Back-N: rewind and resend
                self.last_progress = now;
                return self.flush(node, now);
            }
            Vec::new()
        }

        fn flush(&mut self, node: &mut Node, now: u32) -> Vec<Forward> {
            let mut fwd = Vec::new();
            while (self.send_next - self.send_base) < self.window as u64 {
                let start = (self.send_next - self.send_base) as usize;
                if start >= self.out.len() {
                    break;
                }
                let end = (start + self.max_frame).min(self.out.len());
                let frame = data_frame(self.send_next, &self.out[start..end]);
                fwd.append(&mut node.dg_send(&mut self.s, &frame, now));
                self.send_next += (end - start) as u64;
                self.last_progress = now;
            }
            fwd
        }

        fn on_frame(&mut self, node: &mut Node, frame: &[u8], now: u32) -> Vec<Forward> {
            let mut fwd = Vec::new();
            match frame.first().copied() {
                Some(F_DATA) if frame.len() >= 11 => {
                    let mut ob = [0u8; 8];
                    ob.copy_from_slice(&frame[1..9]);
                    let offset = u64::from_be_bytes(ob);
                    let len = u16::from_be_bytes([frame[9], frame[10]]) as usize;
                    if frame.len() >= 11 + len {
                        if offset == self.recv_next {
                            self.inbox.extend_from_slice(&frame[11..11 + len]);
                            self.recv_next += len as u64;
                        }
                        // Cumulative ACK of the next offset we still need.
                        fwd.append(&mut node.dg_send(&mut self.s, &ack_frame(self.recv_next), now));
                    }
                }
                Some(F_ACK) if frame.len() >= 9 => {
                    let mut ab = [0u8; 8];
                    ab.copy_from_slice(&frame[1..9]);
                    let ackn = u64::from_be_bytes(ab);
                    if ackn > self.send_base {
                        let adv = ((ackn - self.send_base) as usize).min(self.out.len());
                        self.out.drain(0..adv);
                        self.send_base = ackn;
                        self.last_progress = now;
                        fwd.append(&mut self.flush(node, now)); // window reopened
                    }
                }
                _ => {}
            }
            fwd
        }
    }
}

// ---------------------------------------------------------------------------
// Files — content-addressed objects. A signed manifest indexes a set of
// ordinary chunk envelopes by their content IDs. Integrity is free (an
// envelope's ID *is* the hash of its bytes), and swarming is just WANT: any
// peer that holds a chunk can answer for it, since the chunk is named by
// content, not by who made it.
// ---------------------------------------------------------------------------

pub mod file {
    use super::*;

    /// First payload byte of a manifest.
    pub const MANIFEST_TAG: u8 = 0x01;
    /// First payload byte of a chunk: `[CHUNK_TAG][file_id:16][index:4][bytes]`.
    pub const CHUNK_TAG: u8 = 0x07;

    /// A published file: metadata plus the content IDs of its chunk envelopes in
    /// index order. Signed on the wire, so the chunk IDs are authentic; because a
    /// chunk envelope's ID is the hash of its bytes, holding a matching-ID
    /// envelope is itself the integrity proof.
    #[derive(Clone)]
    pub struct Manifest {
        pub file_id: [u8; 16],
        pub chunk_size: u32,
        pub count: u32,
        pub total_len: u64,
        pub name: String,
        pub chunk_ids: Vec<Id>,
    }

    impl Manifest {
        pub fn encode(&self) -> Vec<u8> {
            let name = self.name.as_bytes();
            let mut p = Vec::with_capacity(35 + name.len() + 16 * self.chunk_ids.len());
            p.push(MANIFEST_TAG);
            p.extend_from_slice(&self.file_id);
            p.extend_from_slice(&self.chunk_size.to_be_bytes());
            p.extend_from_slice(&self.count.to_be_bytes());
            p.extend_from_slice(&self.total_len.to_be_bytes());
            p.extend_from_slice(&(name.len() as u16).to_be_bytes());
            p.extend_from_slice(name);
            for c in &self.chunk_ids {
                p.extend_from_slice(c);
            }
            p
        }

        pub fn decode(p: &[u8]) -> Option<Manifest> {
            if p.first() != Some(&MANIFEST_TAG) {
                return None;
            }
            let end = p.len();
            let mut o = 1usize;
            if o + 16 > end {
                return None;
            }
            let mut file_id = [0u8; 16];
            file_id.copy_from_slice(&p[o..o + 16]);
            o += 16;
            if o + 4 > end {
                return None;
            }
            let chunk_size = u32::from_be_bytes([p[o], p[o + 1], p[o + 2], p[o + 3]]);
            o += 4;
            if o + 4 > end {
                return None;
            }
            let count = u32::from_be_bytes([p[o], p[o + 1], p[o + 2], p[o + 3]]);
            o += 4;
            if o + 8 > end {
                return None;
            }
            let mut tb = [0u8; 8];
            tb.copy_from_slice(&p[o..o + 8]);
            let total_len = u64::from_be_bytes(tb);
            o += 8;
            if o + 2 > end {
                return None;
            }
            let name_len = u16::from_be_bytes([p[o], p[o + 1]]) as usize;
            o += 2;
            if o + name_len > end {
                return None;
            }
            let name = String::from_utf8_lossy(&p[o..o + name_len]).into_owned();
            o += name_len;
            // Reject an implausible count before allocating for it.
            if count as usize > (end - o) / 16 {
                return None;
            }
            let mut chunk_ids = Vec::with_capacity(count as usize);
            for _ in 0..count {
                let mut c = [0u8; 16];
                c.copy_from_slice(&p[o..o + 16]);
                o += 16;
                chunk_ids.push(c);
            }
            Some(Manifest { file_id, chunk_size, count, total_len, name, chunk_ids })
        }
    }
}

// ---------------------------------------------------------------------------
// Page 2, rule 2 — KISS framing for byte streams (TCP, serial, RFCOMM, TNCs).
// ---------------------------------------------------------------------------

pub mod kiss {
    const FEND: u8 = 0xC0;
    const FESC: u8 = 0xDB;
    const TFEND: u8 = 0xDC;
    const TFESC: u8 = 0xDD;

    pub fn encode(frame: &[u8]) -> Vec<u8> {
        let mut o = vec![FEND, 0x00]; // FEND + command byte
        for &b in frame {
            match b {
                FEND => o.extend_from_slice(&[FESC, TFEND]),
                FESC => o.extend_from_slice(&[FESC, TFESC]),
                _ => o.push(b),
            }
        }
        o.push(FEND);
        o
    }

    /// Extract complete frames from a stream buffer (command byte stripped).
    pub fn decode(stream: &[u8]) -> Vec<Vec<u8>> {
        let mut frames = Vec::new();
        let mut cur = Vec::new();
        let mut in_frame = false;
        let mut got_cmd = false;
        let mut esc = false;
        for &b in stream {
            if b == FEND {
                if in_frame && !cur.is_empty() {
                    frames.push(std::mem::take(&mut cur));
                }
                in_frame = true;
                got_cmd = false;
                esc = false;
                cur.clear();
                continue;
            }
            if !in_frame {
                continue;
            }
            if !got_cmd {
                got_cmd = true; // skip the command byte
                continue;
            }
            if esc {
                cur.push(if b == TFEND { FEND } else if b == TFESC { FESC } else { b });
                esc = false;
            } else if b == FESC {
                esc = true;
            } else {
                cur.push(b);
            }
        }
        frames
    }
}

// ---------------------------------------------------------------------------
// Page 2, rule 3 — text-channel armor (SMS, email, Usenet, paper, voice).
// ~S1.<base32(env)>.<base32(sha256(env)[..4])>~
// ---------------------------------------------------------------------------

pub mod armor {
    use super::*;
    const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

    fn b32enc(data: &[u8]) -> String {
        let mut out = String::new();
        let mut buf = 0u32;
        let mut bits = 0u32;
        for &b in data {
            buf = (buf << 8) | b as u32;
            bits += 8;
            while bits >= 5 {
                bits -= 5;
                out.push(A[((buf >> bits) & 31) as usize] as char);
            }
        }
        if bits > 0 {
            out.push(A[((buf << (5 - bits)) & 31) as usize] as char);
        }
        out
    }
    fn b32dec(s: &str) -> Option<Vec<u8>> {
        let mut buf = 0u32;
        let mut bits = 0u32;
        let mut out = Vec::new();
        for c in s.chars() {
            if c.is_whitespace() {
                continue;
            }
            let u = c.to_ascii_uppercase();
            let v = A.iter().position(|&x| x as char == u)? as u32;
            buf = (buf << 5) | v;
            bits += 5;
            if bits >= 8 {
                bits -= 8;
                out.push(((buf >> bits) & 0xff) as u8);
            }
        }
        Some(out)
    }

    pub fn wrap(env_wire: &[u8]) -> String {
        let d = Sha256::digest(env_wire);
        format!("~S1.{}.{}~", b32enc(env_wire), b32enc(&d[..4]))
    }
    /// Recover envelope bytes from armor found anywhere in `text`.
    pub fn unwrap(text: &str) -> Option<Vec<u8>> {
        let start = text.find("~S1.")? + 4;
        let end = text[start..].find('~')? + start;
        let body = &text[start..end];
        let (b32, ck) = body.rsplit_once('.')?;
        let env = b32dec(b32)?;
        let want = b32dec(ck)?;
        let got = Sha256::digest(&env);
        if got[..4] == want[..] {
            Some(env)
        } else {
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Bridges — SPORE rides everything. Each medium has one of five shapes (spec
// Page 2); bind by shape and the router never changes. A bridge only moves
// envelope bytes in and out of `Node`; it is not part of the protocol. HTTP,
// a folder, a serial line — all just bridges, none more special than another.
// ---------------------------------------------------------------------------

pub mod bridge {
    use super::*;

    // -- Shape 2: byte streams (TCP, serial, RFCOMM, TNCs) — KISS framing. --

    /// Streaming KISS de-framer. Feed byte slices as they arrive off a stream;
    /// get back complete frames. (Unlike `kiss::decode`, this keeps state across
    /// reads, so a frame split over two `read()`s still reassembles.)
    #[derive(Default)]
    pub struct KissStream {
        cur: Vec<u8>,
        in_frame: bool,
        got_cmd: bool,
        esc: bool,
    }
    impl KissStream {
        pub fn new() -> Self {
            Self::default()
        }
        /// Frame `payload` for transmission on a byte stream.
        pub fn frame(payload: &[u8]) -> Vec<u8> {
            kiss::encode(payload)
        }
        /// Feed raw bytes; return any complete frames they finished.
        pub fn push(&mut self, bytes: &[u8]) -> Vec<Vec<u8>> {
            const FEND: u8 = 0xC0;
            const FESC: u8 = 0xDB;
            const TFEND: u8 = 0xDC;
            const TFESC: u8 = 0xDD;
            let mut out = Vec::new();
            for &b in bytes {
                if b == FEND {
                    if self.in_frame && !self.cur.is_empty() {
                        out.push(std::mem::take(&mut self.cur));
                    }
                    self.in_frame = true;
                    self.got_cmd = false;
                    self.esc = false;
                    self.cur.clear();
                    continue;
                }
                if !self.in_frame {
                    continue;
                }
                if !self.got_cmd {
                    self.got_cmd = true; // skip KISS command byte
                    continue;
                }
                if self.esc {
                    self.cur.push(if b == TFEND {
                        FEND
                    } else if b == TFESC {
                        FESC
                    } else {
                        b
                    });
                    self.esc = false;
                } else if b == FESC {
                    self.esc = true;
                } else {
                    self.cur.push(b);
                }
            }
            out
        }
    }

    // -- Shape 5: shared stores over any bag transport (HTTP, folder, …). --

    /// The three transport-agnostic operations of a "bag" — a container that
    /// carries envelopes between two nodes (spec Page 2's HTTP bag API, but the
    /// same three ops serve a folder, a pastebin, or a BBS).
    pub enum Bag {
        /// Incoming envelopes (one or more concatenated wire forms).
        Push(Vec<u8>),
        /// Advertise what we hold: return our stored IDs (16 B each).
        Inv,
        /// Fetch by ID: body is concatenated 16-B IDs; return their envelopes.
        Want(Vec<u8>),
    }

    /// Apply a bag operation. Returns `(forwards, response_body)` — forwards are
    /// any relays the push triggered (run them on your other interfaces), and
    /// the body is what to send back to the bag peer (empty for `Push`).
    pub fn bag(node: &mut Node, op: Bag, iface: Iface, now: u32) -> (Vec<Forward>, Vec<u8>) {
        match op {
            Bag::Push(body) => {
                let mut fwd = Vec::new();
                let mut off = 0;
                while off < body.len() {
                    match Envelope::decode(&body[off..]) {
                        Ok((_, n)) => {
                            let mut rx = node.on_rx(&body[off..off + n], iface, None, now);
                            fwd.append(&mut rx.forwards);
                            off += n;
                        }
                        Err(_) => break,
                    }
                }
                (fwd, Vec::new())
            }
            Bag::Inv => (Vec::new(), node.stored_ids()),
            Bag::Want(ids) => {
                let mut body = Vec::new();
                for chunk in ids.chunks(16) {
                    if chunk.len() == 16 {
                        let mut id = [0u8; 16];
                        id.copy_from_slice(chunk);
                        if let Some(w) = node.get_wire(&id) {
                            body.extend_from_slice(&w);
                        }
                    }
                }
                (Vec::new(), body)
            }
        }
    }

    /// A shared-store folder: envelopes are files named `<hexid>.spore`. The
    /// folder *is* a persistent INV — reading it is receiving, writing to it is
    /// sending. Backs USB sneakernet, Syncthing, NFS, Dropbox.
    #[cfg(not(target_arch = "wasm32"))]
    pub mod store {
        use super::super::*;
        use std::path::Path;
        use std::{fs, io};

        pub fn filename(id: &Id) -> String {
            let mut s = String::with_capacity(38);
            for b in id {
                s.push_str(&format!("{b:02x}"));
            }
            s.push_str(".spore");
            s
        }

        /// Write one envelope into `dir` if not already present. Returns whether
        /// it was newly written.
        pub fn export(dir: &Path, e: &Envelope) -> io::Result<bool> {
            fs::create_dir_all(dir)?;
            let path = dir.join(filename(&e.id()));
            if path.exists() {
                return Ok(false);
            }
            fs::write(path, e.wire())?;
            Ok(true)
        }

        /// Write the node's whole store into `dir`. Returns how many were new.
        pub fn export_all(dir: &Path, node: &Node) -> io::Result<usize> {
            fs::create_dir_all(dir)?;
            let mut n = 0;
            for (id, wire) in node.store_wires() {
                let path = dir.join(filename(&id));
                if !path.exists() {
                    fs::write(path, wire)?;
                    n += 1;
                }
            }
            Ok(n)
        }

        /// Feed every `*.spore` file in `dir` to the node (reading = receiving).
        /// Returns the aggregate `Rx` (delivered + forwards).
        pub fn import(dir: &Path, node: &mut Node, iface: Iface, now: u32) -> io::Result<Rx> {
            let mut rx = Rx::default();
            if !dir.exists() {
                return Ok(rx);
            }
            for entry in fs::read_dir(dir)? {
                let path = entry?.path();
                if path.extension().and_then(|e| e.to_str()) != Some("spore") {
                    continue;
                }
                let bytes = fs::read(&path)?;
                let mut r = node.on_rx(&bytes, iface, None, now);
                rx.delivered.append(&mut r.delivered);
                rx.forwards.append(&mut r.forwards);
            }
            Ok(rx)
        }
    }
}

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
        let count = (wire.len() + cs - 1) / cs;

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
}
