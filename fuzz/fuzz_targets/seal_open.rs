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

    // The topic key schedule's parsers. `absorb` is the interesting one: a
    // 16-bit count field the attacker picks drives a loop of trial decryptions,
    // so a mismatch between the count and the body length must be rejected
    // rather than read past the buffer.
    let _ = topic::absorb(&key, data, &key);
    let _ = topic::open(data, &key);
    let _ = topic::peek_epoch(data);
    let _ = topic::rekey_open(data, &key);
});
