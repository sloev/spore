//! The decrypt paths, driven with a key the sender did not use.
//!
//! Opening is the side an attacker controls the input to: they choose the
//! ciphertext, we choose the key. Failure must be a `None`, never a panic on a
//! truncated nonce or a length read out of the buffer.
#![no_main]
use libfuzzer_sys::fuzz_target;
use spore::*;

fuzz_target!(|data: &[u8]| {
    // Derive a key from the input so the fuzzer can explore key/ciphertext
    // combinations rather than one fixed pairing.
    let mut key = [0u8; 32];
    for (i, b) in data.iter().take(32).enumerate() {
        key[i] = *b;
    }
    let _ = topic_open(data, &key);
    let _ = open_sealed(data, &key);
});
