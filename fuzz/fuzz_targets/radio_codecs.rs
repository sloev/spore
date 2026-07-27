//! The radio codecs, which anyone with a transmitter in range can feed.
//!
//! Meshtastic framing and the audio modem sit at the very edge: they parse before
//! anything has been authenticated, on media where the sender is whoever is
//! nearby. `src/robustness.rs` covers them on stable with random and corrupted
//! input; this explores far deeper.
//!
//! The oracle here is deliberately stronger than "did not panic". A parser that
//! quietly retains everything it is fed is a resource-exhaustion bug that no
//! panic-only harness can see, so this asserts the framer stays bounded and that
//! every frame it emits is within the declared maximum.
#![no_main]
use libfuzzer_sys::fuzz_target;
use spore::bridge::meshtastic::{self, StreamFramer, STREAM_MAX_LEN};

/// Cap one input. Without this a single enormous case turns into an
/// out-of-memory in the *fuzzer* — which looks like a finding and is not one.
const MAX_INPUT: usize = 8192;

fuzz_target!(|data: &[u8]| {
    let data = &data[..data.len().min(MAX_INPUT)];

    let _ = meshtastic::decode(data);
    let _ = meshtastic::from_radio_packet(data);

    // The stream framer keeps state across reads, which is how a serial line
    // delivers — so feed it in pieces rather than all at once, and hold it to its
    // bounds the whole way.
    let mut framer = StreamFramer::new();
    for piece in data.chunks(5) {
        for frame in framer.push(piece) {
            assert!(
                frame.len() <= STREAM_MAX_LEN,
                "emitted a {}-byte frame past STREAM_MAX_LEN",
                frame.len()
            );
            // Whatever it hands out must itself survive the packet parsers.
            let _ = meshtastic::decode(&frame);
            let _ = meshtastic::from_radio_packet(&frame);
        }
        // A framer that never resyncs would grow here without ever panicking.
        assert!(
            framer.buffered() <= STREAM_MAX_LEN + 8,
            "framer retained {} bytes; it must resync rather than accumulate",
            framer.buffered()
        );
    }

    // Bytes reinterpreted as PCM: whatever the microphone heard. Sample count is
    // bounded by MAX_INPUT above, which keeps demodulation from becoming a
    // memory test rather than a correctness one.
    let samples: Vec<f32> = data.iter().map(|b| (*b as f32 - 128.0) / 128.0).collect();
    let _ = spore::bridge::audio::demodulate(&samples);
});
