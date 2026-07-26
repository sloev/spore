//! Fountain reassembly, which is stateful and therefore worth feeding a *stream*
//! of hostile chunks rather than one.
//!
//! The count and index bytes come straight off the wire. A zero count once
//! reached `idx % count` in the selector and took the process with it; this
//! target exists so the next one of those is found here rather than in the field.
#![no_main]
use libfuzzer_sys::fuzz_target;
use spore::*;

fuzz_target!(|data: &[u8]| {
    let now = 1_700_000_000;
    let mut node = Node::new("fuzz", &[]);
    // Split the input into successive fragment envelopes so one run drives the
    // reassembler through many states.
    for piece in data.chunks(64) {
        if piece.len() < 3 {
            continue;
        }
        let mut payload = vec![0u8; 18];
        payload[..16].copy_from_slice(&[piece[0]; 16]); // orig_id, few distinct sets
        payload[16] = piece[1]; // idx
        payload[17] = piece[2]; // count
        payload.extend_from_slice(&piece[3..]);
        let mut e = Envelope::new(ty::DATA, ZERO_DEST, now + 3600, payload);
        e.flags |= fl::FRAGMENT | fl::FLOOD;
        let _ = node.on_rx(&e.wire(), 1, None, now);
    }
});
