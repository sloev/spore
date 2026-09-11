//! Content-defined chunking (M11-M) — FastCDC with a derived gear table.
//!
//! **Why boundaries must not come from byte offsets.** Cutting a file every N
//! bytes means a single inserted byte shifts every boundary after it, so a
//! one-byte edit re-cuts the whole file and shares nothing with the version
//! before it. Content-defined boundaries are chosen by a rolling hash of the
//! bytes themselves, so an edit only disturbs the chunks it actually touches.
//!
//! **Why the parameters are protocol constants and not derived from the MTU.**
//! They used to be: `chunk_size = mtu - 64`, from the era before link
//! fragmentation, when a sender had to cut for the narrowest hop it might meet.
//! M11-D ended that — a bridge splits what its own link cannot carry — so the
//! only thing publisher MTU still decided was that a Wi-Fi publisher and a LoRa
//! publisher cut the same file differently and therefore shared nothing, which
//! defeats content addressing just as thoroughly as the random `file_id` did.
//! Identical bytes must produce identical chunks *for everyone*, so the cut
//! points cannot depend on who is doing the cutting.
//!
//! Sizes are chosen against the smallest node rather than the largest file:
//! `Limits::for_budget` floors link-reassembly at 16 KiB, so an 8 KiB ceiling
//! lets the tightest MCU profile hold two chunks in flight. Below that, a
//! 4 KiB average keeps manifests small — a 1 MB file is ~256 ids rather than
//! the ~6000 a LoRa-sized chunk would have produced.

use sha2::{Digest, Sha256};
use std::sync::OnceLock;

/// Smallest chunk. Also the run-up the rolling hash is not consulted over,
/// which is what stops a pathological file from producing single-byte chunks.
pub const CHUNK_MIN_BYTES: usize = 1024;
/// Target average chunk size. The cut masks are derived from this.
pub const CHUNK_AVG_BYTES: usize = 4096;
/// Hard ceiling. A chunk must fit a node's reassembly budget, not a link's MTU.
pub const CHUNK_MAX_BYTES: usize = 8192;

/// Bits in the average, i.e. `log2(CHUNK_AVG_BYTES)`.
const AVG_BITS: u32 = CHUNK_AVG_BYTES.trailing_zeros();
/// FastCDC normalisation: a stricter mask before the average size and a looser
/// one after it. That pulls the distribution towards the average instead of the
/// long exponential tail a single mask gives, so far fewer chunks are cut at the
/// hard ceiling — and a chunk cut at the ceiling is one whose boundary is an
/// offset again, which is exactly what this file exists to avoid.
const NORMALISATION: u32 = 2;

fn mask(bits: u32) -> u64 {
    // Test the *top* bits. `h = (h << 1) + gear[b]` shifts history upward, so the
    // high bits are the well-mixed ones; masking the low bits would key the cut
    // on the last byte or two.
    ((1u64 << bits) - 1) << (64 - bits)
}

/// 256 random-looking words, derived rather than tabulated so the rule fits in
/// one line of spec: `GEAR[i] = first 8 bytes of SHA-256("spore-cdc-gear-v1" ‖ i)`.
fn gear() -> &'static [u64; 256] {
    static GEAR: OnceLock<[u64; 256]> = OnceLock::new();
    GEAR.get_or_init(|| {
        let mut g = [0u64; 256];
        for (i, slot) in g.iter_mut().enumerate() {
            let mut h = Sha256::new();
            h.update(b"spore-cdc-gear-v1");
            h.update([i as u8]);
            let d = h.finalize();
            *slot = u64::from_be_bytes(d[..8].try_into().expect("8 bytes"));
        }
        g
    })
}

/// Length of the next chunk of `data`, in `[CHUNK_MIN_BYTES, CHUNK_MAX_BYTES]`
/// unless `data` is shorter than the minimum, in which case all of it.
pub fn next_cut(data: &[u8]) -> usize {
    let n = data.len();
    if n <= CHUNK_MIN_BYTES {
        return n;
    }
    let gear = gear();
    let mask_strict = mask(AVG_BITS + NORMALISATION);
    let mask_loose = mask(AVG_BITS - NORMALISATION);
    let normal = n.min(CHUNK_AVG_BYTES);
    let max = n.min(CHUNK_MAX_BYTES);

    let mut h: u64 = 0;
    let mut i = CHUNK_MIN_BYTES;
    while i < normal {
        h = (h << 1).wrapping_add(gear[data[i] as usize]);
        if h & mask_strict == 0 {
            return i + 1;
        }
        i += 1;
    }
    while i < max {
        h = (h << 1).wrapping_add(gear[data[i] as usize]);
        if h & mask_loose == 0 {
            return i + 1;
        }
        i += 1;
    }
    max
}

/// Every cut length for `data`, in order.
pub fn chunk_lengths(data: &[u8]) -> Vec<usize> {
    let mut out = Vec::new();
    let mut off = 0;
    while off < data.len() {
        let len = next_cut(&data[off..]).max(1);
        out.push(len);
        off += len;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(n: usize) -> Vec<u8> {
        // Pseudo-random but deterministic, and non-repeating over the range used.
        (0..n as u32).map(|i| (i.wrapping_mul(2654435761) >> 13) as u8).collect()
    }

    #[test]
    fn cuts_respect_the_stated_bounds() {
        let d = body(400_000);
        let lens = chunk_lengths(&d);
        assert_eq!(lens.iter().sum::<usize>(), d.len(), "the cuts cover the file exactly");
        for (i, l) in lens.iter().enumerate() {
            assert!(*l <= CHUNK_MAX_BYTES, "chunk {i} is {l}");
            // Every chunk but the last must clear the minimum.
            if i + 1 < lens.len() {
                assert!(*l >= CHUNK_MIN_BYTES, "chunk {i} is {l}");
            }
        }
    }

    #[test]
    fn the_average_lands_near_the_target() {
        let d = body(2_000_000);
        let lens = chunk_lengths(&d);
        let avg = d.len() / lens.len();
        // Generous bounds: this pins the parameters as *chosen*, not the exact
        // statistics of one input.
        assert!(
            avg > CHUNK_AVG_BYTES / 3 && avg < CHUNK_MAX_BYTES,
            "average chunk {avg} against a {CHUNK_AVG_BYTES} target"
        );
    }

    #[test]
    fn an_inserted_byte_disturbs_only_its_own_neighbourhood() {
        // The property the whole file exists for. Fixed-size chunking would
        // share **nothing** after this edit: every boundary shifts by one.
        let a = body(500_000);
        let mut b = a.clone();
        b.insert(0, 0xFF);

        let cut = |d: &[u8]| -> Vec<Vec<u8>> {
            let mut out = Vec::new();
            let mut off = 0;
            for l in chunk_lengths(d) {
                out.push(d[off..off + l].to_vec());
                off += l;
            }
            out
        };
        let (ca, cb) = (cut(&a), cut(&b));
        let shared = ca.iter().filter(|c| cb.contains(c)).count();
        assert!(
            shared * 10 >= ca.len() * 9,
            "only {shared} of {} chunks survived a one-byte prepend",
            ca.len()
        );
    }

    #[test]
    fn the_gear_table_is_reproducible_and_well_spread() {
        // It is derived, so a second implementation must be able to rebuild it.
        let g = gear();
        assert_eq!(
            g[0],
            u64::from_be_bytes(
                Sha256::digest([b"spore-cdc-gear-v1".as_slice(), &[0u8]].concat())[..8].try_into().unwrap()
            )
        );
        let distinct: std::collections::HashSet<u64> = g.iter().copied().collect();
        assert_eq!(distinct.len(), 256, "no collisions in the table");
    }
}
