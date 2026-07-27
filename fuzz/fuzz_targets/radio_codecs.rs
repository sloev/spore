//! The radio codecs, which anyone with a transmitter in range can feed.
//!
//! Meshtastic framing and the audio modem sit at the very edge: they parse before
//! anything has been authenticated, on media where the sender is whoever is
//! nearby. `src/robustness.rs` covers them on stable with random and corrupted
//! input; this explores far deeper.
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = spore::bridge::meshtastic::decode(data);
    let _ = spore::bridge::meshtastic::from_radio_packet(data);

    // The stream framer keeps state across reads, which is how a serial line
    // delivers — so feed it in pieces rather than all at once.
    let mut framer = spore::bridge::meshtastic::StreamFramer::new();
    for piece in data.chunks(5) {
        let _ = framer.push(piece);
    }

    // Bytes reinterpreted as PCM: whatever the microphone heard.
    let samples: Vec<f32> = data.iter().map(|b| (*b as f32 - 128.0) / 128.0).collect();
    let _ = spore::bridge::audio::demodulate(&samples);
});
