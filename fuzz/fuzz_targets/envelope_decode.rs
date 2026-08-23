//! The §1 wire format: the first thing any bridge hands to the core.
//!
//! Decoding must reject anything it dislikes without indexing past the buffer or
//! trusting a length it just read. Anything that *does* decode must then survive
//! the accessors a relay calls on it before deciding what to do.
#![no_main]
use libfuzzer_sys::fuzz_target;
use spore::*;

fuzz_target!(|data: &[u8]| {
    // `probe` is the cheap pre-filter a promiscuous receiver runs before
    // committing to a decode, so it faces the same hostile bytes and must agree
    // with `decode` exactly — accepting no buffer `decode` would reject, and
    // never reporting a different length.
    let probed = Envelope::probe(data);
    if let Some(n) = probed {
        assert!(n <= data.len(), "probe reported more than it was given");
    }

    if let Ok((e, n)) = Envelope::decode(data) {
        assert_eq!(probed, Some(n), "probe disagreed with a successful decode");
        assert!(n <= data.len(), "decode reported consuming more than it was given");
        let _ = e.id();
        let _ = e.stamp();
        let _ = e.verify();
        // Re-encoding a decoded envelope must not panic either; the relay does
        // it on every forward.
        let _ = e.wire();
    }
});
