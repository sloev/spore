//! **Link fragmentation (M11-D)** — how one envelope crosses one narrow hop.
//!
//! This is not the fountain, and it is not the file layer. It sits *below* the
//! node and *below* the signature: a bridge whose link cannot carry a frame
//! splits it, the far end of that same link puts it back together, and the node
//! never sees a piece. Fragments live for one hop and are never relayed.
//!
//! **Why this exists.** `Node::send` fragments once, at the sender's own MTU,
//! and those fragments are ordinary envelopes that flood the mesh — and cannot
//! be fragmented again, because their header has one level and no nesting. So a
//! Wi-Fi node emitting 1364-byte fragments produces frames that die at the first
//! LoRa hop they meet, and no node on the path can repair it. `spore-sim`'s
//! `mixed-mtu` scenario is exactly that: 18 frames and 25 kB transmitted, six
//! refused by the narrow link, nothing delivered.
//!
//! **The header is link framing, not wire.** Because a fragment never leaves the
//! link, its shape is a bridge concern like KISS is, not part of §3. That is
//! what makes it cheap:
//!
//! ```text
//! [0xF6][set:2][idx:2][count:2][chunk …]        7 bytes
//! ```
//!
//! against the 36 the end-to-end fountain header costs (16 envelope header, 2
//! plen, 16 orig_id, idx, count). On a 54-byte Zigbee frame that is 47 usable
//! bytes per fragment instead of 18. `count` is a `u16` rather than the
//! fountain's `u8`, which closes the gap where a legal envelope existed that one
//! hop could not carry: 255 fragments at a 237-byte LoRa frame is 51 kB, against
//! a 65 649-byte maximum envelope.
//!
//! `0xF6` is the discriminator. An envelope's first byte is [`VER`] = `0x01`, so
//! a receiver can tell a fragment from a whole frame without being told, which
//! is what lets a link carry both and lets a peer that never fragments stay
//! exactly as it was.
//!
//! **Bounds.** Reassembly is the one place a neighbour allocates memory here
//! without asking, so it obeys the resource invariant the same way everything
//! else does: a cap on open sets, a cap on bytes, a timeout, and — the part that
//! matters on a shared radio — accounting *per key*, so one loud peer cannot
//! evict another's half-finished frame. On media with underlay addresses the key
//! is the neighbour; on broadcast-only media (`U = ()`: raw LoRa P2P, audio) the
//! interface is the finest unit available, and that is enough, because anyone
//! who can flood a shared radio with fragments can also just jam it.

use crate::{Fountain, Id};
use std::collections::HashMap;

/// First byte of a link fragment. Not [`VER`], so a whole envelope and a
/// fragment are distinguishable on sight.
pub const LINK_FRAG_MAGIC: u8 = 0xF6;

/// Bytes a link fragment adds around its chunk.
pub const LINK_FRAG_OVERHEAD: usize = 7;

/// Most fragments one link set may hold — the `u16` count field's range. At a
/// 237-byte LoRa frame that is 15 MB, far past the 64 KB an envelope can be.
pub const MAX_LINK_FRAGMENTS: usize = u16::MAX as usize;

/// How long a half-finished set is held before it is dropped.
pub const LINK_PARTIAL_TIMEOUT_SECS: u32 = 30;

/// Largest set that can carry repair symbols — the `u16` index field's range,
/// so in practice every set a link will ever see.
///
/// It used to be 255, because the code derived each repair symbol's inputs from
/// a single SHA-256 and one digest addresses 256 chunks. That ceiling bit hardest
/// exactly where loss does: 255 pieces is under 12 kB on a 54-byte Zigbee frame,
/// and past it a set had no repair at all and needed every piece. A 20 kB
/// envelope is 426 pieces, which at 1% frame loss delivers 1.4% of the time.
/// `selection` now hashes in numbered blocks, so the ceiling is the field.
pub const MAX_REPAIRABLE_PIECES: usize = u16::MAX as usize;

/// How many repair symbols to send with a set of `count` pieces.
///
/// A quarter, at least one — about 25% extra on a fragmented set. Measured over
/// 200 trials of a 900-byte envelope on a 237-byte link (five pieces):
///
/// | frame loss | no repair | one repair symbol |
/// |---|---|---|
/// | 5%  | 79% | 97% |
/// | 10% | 54% | 90% |
/// | 20% | 36% | 67% |
///
/// The cost lands where the loss is, which is what makes a flat fraction
/// defensible: a link wide enough to carry the frame never fragments and never
/// pays, and the links that *do* fragment — a 237-byte radio, a 54-byte Zigbee
/// frame — are the lossy ones. A wired MTU-1400 hop is untouched.
///
/// Local policy, not a wire rule: the sender picks, the receiver decodes from
/// whatever arrives, and the two never need to agree. A transport that has
/// measured its own loss should override it, and one on a clean link may set 0.
/// Adapting it to observed loss is the obvious refinement and is not done.
pub fn default_repair(count: usize) -> usize {
    if count < 2 {
        0
    } else {
        (count / 4).max(1)
    }
}

/// The seed a set's repair symbols are selected from.
///
/// Both ends derive it from what the frame already carries, so nothing extra
/// goes on the wire. It only has to agree between the two ends of one link for
/// the length of one set.
fn set_seed(set_id: u16, count: usize) -> Id {
    let mut id = [0u8; 16];
    id[..2].copy_from_slice(&set_id.to_be_bytes());
    id[2..4].copy_from_slice(&(count as u16).to_be_bytes());
    id
}

/// Is this frame a link fragment rather than a whole envelope?
pub fn is_fragment(frame: &[u8]) -> bool {
    frame.len() >= LINK_FRAG_OVERHEAD && frame[0] == LINK_FRAG_MAGIC
}

/// Split `wire` for a link whose frame is `mtu`, with `repair` extra symbols.
///
/// **Why repair symbols and not retries.** An envelope split into n pieces
/// arrives only if all n do, so a link dropping 10% of frames loses ~46% of
/// fragmented messages — measured, and it tracks `(1-p)^n` exactly as it should.
/// Repetition was measured as the cheapest alternative and recovers 70% for 40%
/// extra traffic where this code gives 97% for the same, because a duplicate
/// only helps if it lands on a gap and duplicates collide. Decoding costs
/// 1.4–2.4× plain reassembly, which is single-digit parts per million of the
/// airtime needed to receive the pieces — so the radio, not the arithmetic, is
/// what this costs.
///
/// Repair needs no return path, which is what makes it usable on the media that
/// need it most: a one-way radio, a shared channel where a NACK would collide
/// with the traffic it is complaining about.
///
/// Returns the wire unchanged, in one piece, when it already fits — a link that
/// can carry the frame pays nothing for any of this.
pub fn split_with_repair(wire: &[u8], mtu: usize, set_id: u16, repair: usize) -> Vec<Vec<u8>> {
    let mut out = split(wire, mtu, set_id);
    let count = out.len();
    if count < 2 || repair == 0 || count > MAX_REPAIRABLE_PIECES {
        return out;
    }
    let chunk = mtu.saturating_sub(LINK_FRAG_OVERHEAD).max(1);
    let seed = set_seed(set_id, count);
    // Indices at or past `count` are repair symbols; the sender can mint as many
    // distinct ones as it likes, which is what rateless means.
    let repair = repair.min(MAX_REPAIRABLE_PIECES - count);
    let indices: Vec<u16> = (count..count + repair).map(|i| i as u16).collect();
    for e in crate::fragment(wire, chunk, 0, 0, crate::ZERO_DEST, seed, &indices) {
        // `fragment` hands back envelopes; only the chunk body matters here,
        // re-framed as a link fragment. The envelope header it built is the
        // end-to-end form this layer exists to replace.
        let idx = u16::from_be_bytes([e.payload[16], e.payload[17]]);
        let body = &e.payload[20..];
        let mut f = Vec::with_capacity(LINK_FRAG_OVERHEAD + body.len());
        f.push(LINK_FRAG_MAGIC);
        f.extend_from_slice(&set_id.to_be_bytes());
        f.extend_from_slice(&idx.to_be_bytes());
        f.extend_from_slice(&(count as u16).to_be_bytes());
        f.extend_from_slice(body);
        out.push(f);
    }
    out
}

/// Split for this link with the default amount of repair — what a bridge wants
/// unless it knows something specific about its own loss.
pub fn split_for_link(wire: &[u8], mtu: usize, set_id: u16) -> Vec<Vec<u8>> {
    if wire.len() <= mtu {
        return vec![wire.to_vec()];
    }
    let chunk = mtu.saturating_sub(LINK_FRAG_OVERHEAD).max(1);
    let count = wire.len().div_ceil(chunk);
    split_with_repair(wire, mtu, set_id, default_repair(count))
}

/// Split with no repair symbols.
pub fn split(wire: &[u8], mtu: usize, set_id: u16) -> Vec<Vec<u8>> {
    if wire.len() <= mtu {
        return vec![wire.to_vec()];
    }
    let chunk = mtu.saturating_sub(LINK_FRAG_OVERHEAD).max(1);
    let count = wire.len().div_ceil(chunk);
    if count > MAX_LINK_FRAGMENTS {
        // Structurally impossible for a legal envelope: 65 535 fragments of even
        // one byte exceeds what `plen` can describe. Refuse rather than emit a
        // set that can never be reassembled.
        return Vec::new();
    }
    let mut out = Vec::with_capacity(count);
    for (i, part) in wire.chunks(chunk).enumerate() {
        let mut f = Vec::with_capacity(LINK_FRAG_OVERHEAD + part.len());
        f.push(LINK_FRAG_MAGIC);
        f.extend_from_slice(&set_id.to_be_bytes());
        f.extend_from_slice(&(i as u16).to_be_bytes());
        f.extend_from_slice(&(count as u16).to_be_bytes());
        f.extend_from_slice(part);
        out.push(f);
    }
    out
}

struct Partial {
    started: u32,
    count: usize,
    bytes: usize,
    /// Symbols as they arrived, by index. Held rather than decoded incrementally
    /// because the decoder needs every symbol to be the same length, and the
    /// *last* data piece of a set is short — so the full chunk size is only
    /// known once a full-length piece has been seen. Padding on the wire instead
    /// would waste up to a whole frame on every fragmented set, which on a
    /// 237-byte LoRa link is most of a packet.
    symbols: HashMap<u16, Vec<u8>>,
}

/// Feed a set's symbols to the erasure decoder.
///
/// Every symbol must be the same length, and the last data piece of a set is
/// short — so pad up to the longest symbol seen. That length *is* the chunk
/// size: every piece but the last fills the frame, and a set that has reached
/// `count` symbols has at least one of those.
///
/// The decoder finishes by parsing the result as an envelope, which is how it
/// strips the padding, and is also why nothing but an envelope can come out of
/// here — on a medium without addresses two senders can collide on a set id, and
/// the bytes that produces must not reach the router.
fn decode_coded(set: u16, count: usize, symbols: &HashMap<u16, Vec<u8>>) -> Option<Vec<u8>> {
    let chunk = symbols.values().map(Vec::len).max()?;
    let seed = set_seed(set, count);
    let mut f = Fountain::new();
    let mut out = None;
    for (idx, body) in symbols {
        let mut padded = body.clone();
        padded.resize(chunk, 0);
        out = f.add(&seed, *idx, count as u16, padded);
        if out.is_some() {
            break;
        }
    }
    out
}

/// Per-link reassembly, bounded in every direction a neighbour could push.
pub struct Reassembler {
    /// (key, set id) -> partial. `key` is the neighbour where the medium has
    /// addresses, and the interface where it does not.
    open: HashMap<(u32, u16), Partial>,
    max_sets: usize,
    max_bytes: usize,
    timeout: u32,
}

impl Default for Reassembler {
    /// The node-wide ceilings, so an operator who sized a small runtime with
    /// [`crate::Limits::for_budget`] sizes this too. Reassembly moved from the
    /// node to the bridge; the budget for it should not have been left behind.
    fn default() -> Self {
        Self::new(crate::MAX_PARTIAL_OBJECTS, crate::DEFAULT_PARTIAL_BUDGET)
    }
}

impl Reassembler {
    pub fn new(max_sets: usize, max_bytes: usize) -> Self {
        Reassembler {
            open: HashMap::new(),
            max_sets: max_sets.max(1),
            max_bytes: max_bytes.max(2048),
            timeout: LINK_PARTIAL_TIMEOUT_SECS,
        }
    }

    /// Open sets held right now — reassembly is where a neighbour allocates
    /// memory here unasked, so it is worth being able to look at.
    pub fn open_sets(&self) -> usize {
        self.open.len()
    }

    /// Open sets charged to one key.
    pub fn open_sets_for(&self, key: u32) -> usize {
        self.open.keys().filter(|(k, _)| *k == key).count()
    }

    /// Feed one arriving frame.
    ///
    /// Returns the whole envelope when this frame completed it, the frame itself
    /// when it was never a fragment, and `None` while a set is still short.
    pub fn accept(&mut self, key: u32, frame: &[u8], now: u32) -> Option<Vec<u8>> {
        if !is_fragment(frame) {
            return Some(frame.to_vec());
        }
        let set = u16::from_be_bytes([frame[1], frame[2]]);
        let idx = u16::from_be_bytes([frame[3], frame[4]]);
        let count = u16::from_be_bytes([frame[5], frame[6]]) as usize;
        // A set of zero fragments cannot complete. Both fields come off a link a
        // stranger can write to, so neither is believed.
        if count == 0 {
            return None;
        }
        // A repair symbol is one whose index is at or past the count. Only a set
        // small enough for the code can have them; on a larger set such an index
        // is nonsense and refused.
        let repairable = count <= MAX_REPAIRABLE_PIECES;
        if !repairable && idx as usize >= count {
            return None;
        }
        let body = frame[LINK_FRAG_OVERHEAD..].to_vec();

        self.sweep(now);
        let entry = self.open.entry((key, set)).or_insert_with(|| Partial {
            started: now,
            count,
            bytes: 0,
            symbols: HashMap::new(),
        });
        // A set's size is fixed by its first fragment. A later frame claiming a
        // different count is a different object wearing the same id.
        if entry.count != count {
            return None;
        }
        if entry.symbols.insert(idx, body).is_none() {
            entry.bytes = entry.symbols.values().map(Vec::len).sum();
        }

        // Not enough to try yet. `count` distinct symbols is the *minimum* for
        // the code, and exactly what the plain path needs too.
        if entry.symbols.len() < count {
            self.enforce(now);
            return None;
        }

        let whole = if repairable {
            decode_coded(set, count, &entry.symbols)
        } else {
            // No code available at this size: the pieces must all be data, in
            // order, and every one of them must have arrived.
            let mut w = Vec::with_capacity(entry.bytes);
            for i in 0..count as u16 {
                w.extend_from_slice(entry.symbols.get(&i)?);
            }
            crate::Envelope::decode(&w).ok().map(|(_, n)| w[..n].to_vec())
        };
        match whole {
            Some(w) => {
                self.open.remove(&(key, set));
                Some(w)
            }
            None => {
                // Rank-deficient, or the bytes are not an envelope. Either way
                // more symbols may still help, so the set stays open until its
                // timeout — bounded like everything else.
                self.enforce(now);
                None
            }
        }
    }

    fn sweep(&mut self, now: u32) {
        let t = self.timeout;
        self.open.retain(|_, p| now.saturating_sub(p.started) < t);
    }

    /// Evict from whichever key is holding the most, oldest first — so a loud
    /// neighbour spends its own share and never another's. The same rule the
    /// node applies to end-to-end partial sets, for the same reason.
    fn enforce(&mut self, _now: u32) {
        loop {
            let sets = self.open.len();
            let bytes: usize = self.open.values().map(|p| p.bytes).sum();
            if sets <= self.max_sets && bytes <= self.max_bytes {
                return;
            }
            let mut per_key: HashMap<u32, usize> = HashMap::new();
            for ((k, _), p) in &self.open {
                *per_key.entry(*k).or_insert(0) += p.bytes;
            }
            let Some((&worst, _)) = per_key.iter().max_by_key(|(k, v)| (**v, std::cmp::Reverse(**k))) else {
                return;
            };
            let victim = self
                .open
                .iter()
                .filter(|((k, _), _)| *k == worst)
                .min_by_key(|((_, s), p)| (p.started, *s))
                .map(|(k, _)| *k);
            match victim {
                Some(k) => {
                    self.open.remove(&k);
                }
                None => return,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{fl, ty, Envelope, ZERO_DEST};

    const NOW: u32 = 1_700_000_000;

    /// A **real** envelope of roughly `n` bytes.
    ///
    /// It has to be real. The decoder finishes by parsing what it reassembled —
    /// that is how it strips the padding — so a fixture of plausible-looking
    /// bytes never completes, and a test built on one measures the timeout
    /// rather than the code.
    fn envelope_like(n: usize) -> Vec<u8> {
        let payload = n.saturating_sub(18).max(1);
        let mut e = Envelope::new(ty::DATA, ZERO_DEST, 1_700_003_600, vec![0x5A; payload]);
        e.flags |= fl::FLOOD;
        e.wire()
    }

    #[test]
    fn a_frame_that_fits_is_not_touched() {
        let w = envelope_like(100);
        let parts = split(&w, 1400, 1);
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0], w, "a link that can carry it pays nothing");
        assert!(!is_fragment(&parts[0]));
    }

    #[test]
    fn a_frame_too_big_for_the_link_crosses_it_in_pieces() {
        let w = envelope_like(4000);
        let parts = split(&w, 237, 7);
        assert!(parts.len() > 1);
        assert!(parts.iter().all(|p| p.len() <= 237), "every piece fits the link");
        let mut r = Reassembler::default();
        let mut out = None;
        for p in &parts {
            out = r.accept(1, p, NOW);
        }
        assert_eq!(out.as_deref(), Some(&w[..]), "and arrives identical");
        assert_eq!(r.open_sets(), 0, "a completed set is not still held");
    }

    #[test]
    fn pieces_arriving_out_of_order_still_reassemble() {
        // A radio reorders, and a bridge may interleave two links.
        let w = envelope_like(2000);
        let mut parts = split(&w, 200, 3);
        parts.reverse();
        let mut r = Reassembler::default();
        let mut out = None;
        for p in &parts {
            out = r.accept(1, p, NOW);
        }
        assert_eq!(out.as_deref(), Some(&w[..]));
    }

    #[test]
    fn two_peers_fragmenting_at_once_do_not_mix() {
        // The same set id from two neighbours is not the same set. On a medium
        // with addresses this is what keeps them apart; on a broadcast medium
        // the key is the interface and they genuinely can collide, which is why
        // a mis-reassembly must be *safe* — see the next test.
        let a = envelope_like(900);
        let b = envelope_like(1500);
        let pa = split(&a, 300, 5);
        let pb = split(&b, 300, 5);
        let mut r = Reassembler::default();
        let mut got_a = None;
        let mut got_b = None;
        for i in 0..pa.len().max(pb.len()) {
            if let Some(f) = pa.get(i) {
                if let Some(w) = r.accept(1, f, NOW) {
                    got_a = Some(w);
                }
            }
            if let Some(f) = pb.get(i) {
                if let Some(w) = r.accept(2, f, NOW) {
                    got_b = Some(w);
                }
            }
        }
        assert_eq!(got_a.as_deref(), Some(&a[..]));
        assert_eq!(got_b.as_deref(), Some(&b[..]));
    }

    #[test]
    fn a_set_that_reassembles_into_nonsense_is_dropped_not_forwarded() {
        // Fragments are unsigned, and on a medium without addresses two senders
        // can collide on a set id. The result is bytes that are not an envelope.
        // Costing a buffer is acceptable; handing them to the parser is not.
        let mut junk = vec![0x99u8; 500]; // first byte is not VER
        junk[0] = 0x99;
        let parts = split(&junk, 100, 1);
        let mut r = Reassembler::default();
        let mut out = None;
        for p in &parts {
            out = r.accept(1, p, NOW);
        }
        assert_eq!(out, None, "reassembled non-envelope must not be passed up");
    }

    #[test]
    fn a_half_finished_set_is_forgotten_after_the_timeout() {
        // S-013's shape, one hop down: a sender that opens a set and walks away
        // must not be a permanent allocation.
        let w = envelope_like(3000);
        let parts = split(&w, 250, 11);
        let mut r = Reassembler::default();
        r.accept(1, &parts[0], NOW);
        assert_eq!(r.open_sets(), 1);
        // Any later frame drives the sweep; the set is long past its timeout.
        let later = NOW + LINK_PARTIAL_TIMEOUT_SECS + 1;
        r.accept(1, &parts[1], later);
        assert_eq!(r.open_sets(), 1, "the abandoned set went, the new one is held");
    }

    #[test]
    fn a_loud_neighbour_cannot_evict_a_quiet_one() {
        // The per-interface rule from `enforce_partial_budget`, applied one layer
        // down. Without it the noisiest link on a bridge empties the buffer.
        let mut r = Reassembler::new(4, 8192);
        let quiet = split(&envelope_like(2000), 300, 1);
        r.accept(7, &quiet[0], NOW);
        assert_eq!(r.open_sets_for(7), 1);

        for set in 0..20u16 {
            let loud = split(&envelope_like(2000), 300, set + 100);
            r.accept(1, &loud[0], NOW);
        }
        assert_eq!(r.open_sets_for(7), 1, "the quiet link kept its share");
        assert!(r.open_sets() <= 4, "and the cap held: {}", r.open_sets());
    }

    #[test]
    fn a_zero_count_set_is_refused_but_a_high_index_is_a_repair_symbol() {
        // `count` and `idx` come off a link a stranger can write to, so neither
        // is believed — but the rule changed when the code arrived, and it is
        // worth being explicit about which way. A set of zero can never
        // complete. An index at or past the count used to be nonsense and is now
        // how a repair symbol identifies itself.
        let mut r = Reassembler::default();

        let zero = vec![LINK_FRAG_MAGIC, 0, 1, 0, 0, 0, 0, b'x'];
        assert_eq!(r.accept(1, &zero, NOW), None, "a zero-count set");
        assert_eq!(r.open_sets(), 0, "and it opened nothing");

        let mut repair = vec![LINK_FRAG_MAGIC, 0, 1, 0, 9, 0, 2];
        repair.extend_from_slice(b"body");
        assert_eq!(r.accept(1, &repair, NOW), None, "not enough symbols yet");
        assert_eq!(r.open_sets(), 1, "but index 9 of a 2-set is a repair symbol, not junk");

        // There is no longer a size past which a high index is nonsense: the
        // code addresses as many chunks as the `u16` can name, so a 300-piece
        // set takes repair symbols exactly like a 2-piece one. That was not true
        // before `selection` hashed in blocks, and the sets it excluded — under
        // 12 kB on a Zigbee frame — were the ones least able to lose a piece.
        let mut big = vec![LINK_FRAG_MAGIC, 0, 2];
        big.extend_from_slice(&700u16.to_be_bytes());
        big.extend_from_slice(&300u16.to_be_bytes());
        big.extend_from_slice(b"body");
        assert_eq!(r.accept(2, &big, NOW), None, "not enough symbols yet");
        assert_eq!(r.open_sets_for(2), 1, "a 300-piece set takes repair too");
    }

    #[test]
    fn a_set_with_repair_survives_losing_a_piece() {
        // The point of the whole exercise. Without repair an envelope split into
        // n pieces needs all n; with it, any n of the n+r that were sent.
        let w = envelope_like(2000);
        let sent = split_with_repair(&w, 237, 5, 4);
        let plain = split(&w, 237, 5).len();
        assert_eq!(sent.len(), plain + 4, "four repair symbols went out");

        // Drop two data pieces — the ones a lossy link is most likely to eat.
        let arrived: Vec<&Vec<u8>> =
            sent.iter().enumerate().filter(|(i, _)| *i != 1 && *i != 3).map(|(_, f)| f).collect();
        let mut r = Reassembler::default();
        let mut out = None;
        for f in arrived {
            if let Some(w) = r.accept(1, f, NOW) {
                out = Some(w);
            }
        }
        assert_eq!(out.as_deref(), Some(&w[..]), "recovered from the repair symbols");
    }

    #[test]
    fn repair_symbols_cost_nothing_on_a_frame_that_fits() {
        // A link wide enough for the frame never fragments, so it never pays for
        // the code being available.
        let w = envelope_like(100);
        assert_eq!(split_with_repair(&w, 1400, 1, 8).len(), 1);
    }

    #[test]
    fn the_overhead_is_what_the_docs_claim() {
        // The whole argument for moving the header off the wire is the byte
        // count, so it is worth pinning.
        let w = envelope_like(1000);
        let parts = split(&w, 54, 1);
        let payload: usize = parts.iter().map(|p| p.len() - LINK_FRAG_OVERHEAD).sum();
        assert_eq!(payload, w.len());
        assert_eq!(parts[0].len(), 54, "a full fragment fills the frame");
        assert_eq!(54 - LINK_FRAG_OVERHEAD, 47, "47 usable bytes on a Zigbee frame, against 18 end-to-end");
    }
}
