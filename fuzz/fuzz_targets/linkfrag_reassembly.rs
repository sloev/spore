//! Link-fragment reassembly, fed hostile sets from several neighbours at once.
//!
//! fuzz-covers: link fragmentation
//!
//! The security matrix (`docs/SECURITY_MATRIX.md`) is what surfaced this gap.
//! `fragment_reassembly` covers the *fountain*, which is the end-to-end form; the
//! link reassembler is a different module with a different header, and until this
//! target it had no adversarial input at all — despite being, in its own words,
//! "the one place a neighbour allocates memory here without asking".
//!
//! What is worth breaking here:
//!
//!   - `count` and `idx` are attacker-chosen `u16`s straight off the link. A set
//!     claiming 65 535 pieces, or an index past its own count, must cost bounded
//!     memory and never index out of bounds.
//!   - The bounds are *per key*, so the interesting input is several keys
//!     interleaved: one loud neighbour must not evict another's half-finished
//!     frame, and the accounting that prevents it is the part with the arithmetic.
//!   - A piece whose set already completed, a second copy of the same index with
//!     different bytes, and a set that never completes all have to age out rather
//!     than accumulate.
//!
//! Completion is not the property under test — a random stream rarely completes a
//! set. Bounded memory, no panic, and no index out of range are.
#![no_main]
use libfuzzer_sys::fuzz_target;
use spore::*;

fuzz_target!(|data: &[u8]| {
    // Small caps so the fuzzer reaches the eviction paths in a short run rather
    // than only the happy one. These are the same knobs a constrained bridge
    // would pick, so the arithmetic under test is the arithmetic that ships.
    let mut r = linkfrag::Reassembler::new(4, 4096);
    let mut now: u32 = 1_700_000_000;

    for (i, piece) in data.chunks(48).enumerate() {
        if piece.is_empty() {
            continue;
        }
        // Spread pieces across a handful of neighbour keys: per-key accounting is
        // the property, and one key exercises none of it.
        let key = (piece[0] % 5) as u32;

        // Half the input is offered raw, so malformed and truncated headers are
        // reached; the other half is given a well-formed header with hostile
        // count/index fields, which is the shape that gets past the first check
        // and into the arithmetic.
        let frame: Vec<u8> = if i % 2 == 0 {
            piece.to_vec()
        } else {
            let mut f = vec![linkfrag::LINK_FRAG_MAGIC];
            f.extend_from_slice(&[piece[0], piece.get(1).copied().unwrap_or(0)]); // set
            f.extend_from_slice(&[
                piece.get(2).copied().unwrap_or(0),
                piece.get(3).copied().unwrap_or(0),
            ]); // idx
            f.extend_from_slice(&[
                piece.get(4).copied().unwrap_or(0),
                piece.get(5).copied().unwrap_or(0),
            ]); // count
            f.extend_from_slice(piece);
            f
        };

        if let Some(whole) = r.accept(key, &frame, now) {
            // Anything it hands back claims to be a whole frame. It need not be a
            // *valid* envelope — the reassembler is below the parser and does not
            // know what it is carrying — but it must be something the parser can
            // be handed without panicking, which is the contract the bridge relies
            // on one line later.
            let _ = Envelope::decode(&whole);
        }

        // Advance time unevenly so partial sets expire mid-stream rather than
        // only at the end, which is when the expiry accounting is least exercised.
        now = now.wrapping_add((piece[0] % 20) as u32);

        // The bound the module promises, checked every step rather than once at
        // the end: a stream of hostile pieces must not open unbounded sets.
        assert!(r.open_sets() <= 4, "open sets exceeded the cap");
    }
});
