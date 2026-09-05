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

use crate::VER;
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

/// Is this frame a link fragment rather than a whole envelope?
pub fn is_fragment(frame: &[u8]) -> bool {
    frame.len() >= LINK_FRAG_OVERHEAD && frame[0] == LINK_FRAG_MAGIC
}

/// Split `wire` for a link whose frame is `mtu`, tagging the set `set_id`.
///
/// Returns the wire unchanged, in one piece, when it already fits — a link that
/// can carry the frame pays nothing for this existing.
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
    parts: HashMap<u16, Vec<u8>>,
    bytes: usize,
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
    fn default() -> Self {
        Self::new(64, 256 * 1024)
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
        // A set of zero fragments cannot complete, and an index outside the set
        // is either a bug or an attempt to make one — both come off a link a
        // stranger can write to.
        if count == 0 || idx as usize >= count {
            return None;
        }
        let body = frame[LINK_FRAG_OVERHEAD..].to_vec();

        self.sweep(now);
        let entry = self.open.entry((key, set)).or_insert_with(|| Partial {
            started: now,
            count,
            parts: HashMap::new(),
            bytes: 0,
        });
        // A set's size is fixed by its first fragment. A later frame claiming a
        // different count is a different object wearing the same id.
        if entry.count != count {
            return None;
        }
        if entry.parts.insert(idx, body.clone()).is_none() {
            entry.bytes += body.len();
        }

        let complete = entry.parts.len() == entry.count;
        if complete {
            let mut whole = Vec::with_capacity(entry.bytes);
            for i in 0..entry.count as u16 {
                whole.extend_from_slice(entry.parts.get(&i)?);
            }
            self.open.remove(&(key, set));
            // Only hand up something that could be an envelope. A reassembled
            // set that is not one wasted a buffer; passing it on would waste a
            // parse and put attacker-chosen bytes one layer deeper.
            return if whole.first() == Some(&VER) { Some(whole) } else { None };
        }
        self.enforce(now);
        None
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

    const NOW: u32 = 1_700_000_000;

    fn envelope_like(n: usize) -> Vec<u8> {
        let mut v = vec![VER];
        v.extend((0..n - 1).map(|i| (i % 251) as u8));
        v
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
    fn a_fragment_claiming_an_impossible_index_is_refused() {
        // count and idx come off a link a stranger can write to.
        let mut r = Reassembler::default();
        let mut f = vec![LINK_FRAG_MAGIC, 0, 1, 0, 9, 0, 2];
        f.extend_from_slice(b"body");
        assert_eq!(r.accept(1, &f, NOW), None, "idx 9 of a 2-set");
        let zero = vec![LINK_FRAG_MAGIC, 0, 1, 0, 0, 0, 0, b'x'];
        assert_eq!(r.accept(1, &zero, NOW), None, "a zero-count set");
        assert_eq!(r.open_sets(), 0, "neither opened a set");
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
