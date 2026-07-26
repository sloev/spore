//! The whole receive path in one call.
//!
//! `on_rx` is where everything a bridge read ends up, and it runs dedup, quota
//! accounting, path learning, reassembly and delivery in a single pass over
//! bytes a stranger chose. It is the highest-value target here: a panic in any
//! of those stages is a remotely triggered node death.
#![no_main]
use libfuzzer_sys::fuzz_target;
use spore::*;

fuzz_target!(|data: &[u8]| {
    // A fresh node per input keeps runs reproducible; the stateful cross-input
    // case is covered by `fragment_reassembly`.
    let mut node = Node::new("fuzz", &[]);
    let _ = node.on_rx(data, 1, None, 1_700_000_000);
});
