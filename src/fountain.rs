//! Fountain coding — the erasure code SPORE fragments over (§3).
//!
//! An object too large for one envelope is split into `count` chunks and sent as
//! *rateless* fragments: data chunks `0..count`, then repair chunks the sender can
//! mint endlessly, each an XOR of a pseudo-random subset. A receiver needs any
//! `count` linearly independent fragments — which ones does not matter — so a lossy
//! radio, a torn Seed Sheet and a half-heard audio burst all recover the same way.
//!
//! Extracted from `lib.rs` unchanged: same code, same wire, re-exported so every
//! existing path (`spore::Fountain`, `spore::fragment`) keeps working.
//!
//! Everything here parses attacker-chosen input. `count` and `idx` come off the
//! wire, which is why `selection` refuses a zero count (S-001, a remote panic via
//! `idx % count`) and why `Fountain::add` bounds what one sender can leave
//! half-finished (S-013).

use crate::*;

/// Bytes a fragment envelope adds around its chunk: 16 header + 2 plen +
/// 16 orig_id + 1 index + 1 count. `chunk = mtu - FRAG_OVERHEAD`.
pub(crate) const FRAG_OVERHEAD: usize = 36;

/// Most chunks one fountain set can hold.
///
/// Structural, not policy: the fragment header carries `count` as a single wire
/// byte (§1), so a set addresses at most 255 chunks of `mtu - FRAG_OVERHEAD`. An
/// object past that needs the file/manifest layer, which is what that layer is
/// for.
pub const MAX_FOUNTAIN_CHUNKS: usize = 255;

/// Selection bitmap for repair chunk `idx`: first `count` bits (MSB-first) of
/// SHA-256(orig_id ‖ idx). Empty selection maps to data chunk (idx mod count).
///
/// `count` is caller-supplied and, on the receive path, comes off the wire. Zero
/// is rejected here as well as at the caller: the fallback below is `idx % count`
/// and a panic in a pure helper is a poor place to learn that. The digest holds
/// 32 bytes, so `count` must also stay within the 256 bits it can index — the
/// sender asserts `count <= 255` and the wire field is a `u8`, but the bound is
/// checked rather than assumed, because both of those are facts about *callers*.
fn selection(orig_id: &Id, idx: u8, count: usize) -> BitVec {
    if count == 0 || count > 256 {
        return BitVec::zeros(count.min(256));
    }
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
    /// When the first chunk of this set arrived, so an incomplete set can be
    /// collected. A sender who opens a set and never finishes it is otherwise a
    /// permanent allocation.
    ///
    /// `started`, `count`, `rows` and `done` are `pub(crate)` rather than private
    /// only because `Node::enforce_bounds` sweeps and evicts partial sets from
    /// outside this module. Extracting the code did not widen the *public* API —
    /// none of these appear in `spore`'s surface — but it is worth naming the one
    /// coupling the move exposed rather than leaving it implicit.
    pub(crate) started: u32,
    /// The interface this set was first heard on, so the partial budget can be
    /// shared fairly between links instead of first-come-first-served.
    ///
    /// First arrival, not last: fragments flood, so the same set legitimately
    /// arrives on several interfaces. Attribution has to pick one, and the link
    /// that opened the allocation is the one that caused it.
    pub(crate) iface: Iface,
    pub(crate) count: usize,
    chunk: usize,
    pub(crate) rows: Vec<Row>, // kept in reduced row-echelon form
    pub(crate) done: Option<Vec<u8>>,
}
pub(crate) struct Row {
    pivot: usize,
    coeff: BitVec,
    data: Vec<u8>,
}
impl Fountain {
    /// Chunk bytes this set is holding.
    ///
    /// `MAX_PARTIAL_OBJECTS` bounds how many sets exist, not how large they are:
    /// a set holds up to `count` rows of `chunk` bytes, and both come off the
    /// wire. Counting sets is therefore not the same as counting memory, which
    /// is what the eviction sweep actually needs to bound.
    pub(crate) fn held_bytes(&self) -> usize {
        self.rows.iter().map(|r| r.data.len()).sum()
    }

    pub fn new() -> Self {
        Fountain { started: 0, iface: NO_IFACE, count: 0, chunk: 0, rows: Vec::new(), done: None }
    }
    /// Feed one fragment's `(orig_id, idx, count, chunk_bytes)`.
    /// Returns the reassembled original envelope bytes once solvable.
    pub fn add(&mut self, orig_id: &Id, idx: u8, count: u8, chunk_bytes: Vec<u8>) -> Option<Vec<u8>> {
        if self.done.is_some() {
            return self.done.clone();
        }
        let count = count as usize;
        // `count` is one byte taken straight off the wire. A set of zero chunks
        // cannot reassemble into anything, and believing it reaches
        // `idx % count` below — a division by zero, which is a panic, which is
        // the whole node. Any peer could send that: a public FRAGMENT with a
        // zero count needs no key and no forgery.
        if count == 0 {
            return None;
        }
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
impl Fountain {
    /// A reassembler that records when it opened, so [`PARTIAL_TIMEOUT_SECS`] can
    /// collect it if the sender never finishes the set.
    pub fn started_at(now: u32) -> Self {
        Self::started_on(now, NO_IFACE)
    }

    /// As [`Fountain::started_at`], recording which interface opened the set so
    /// the partial budget can be shared between links.
    pub fn started_on(now: u32, iface: Iface) -> Self {
        Fountain { started: now, iface, ..Self::new() }
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
