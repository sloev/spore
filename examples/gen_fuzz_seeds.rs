//! Write a seed corpus for the `radio_codecs` fuzz target.
//!
//! libFuzzer starting from nothing must *discover* a valid Meshtastic frame
//! before it can explore what happens past one — a lot of budget spent
//! rediscovering a format we already implement. Seeding it with real
//! `encode`/`stream_encode` output starts it at the interesting boundary.
//!
//! Run: `cargo run --example gen_fuzz_seeds` (regenerate if the framing changes).
use spore::bridge::meshtastic;
use spore::*;

fn main() -> std::io::Result<()> {
    let dir = std::path::Path::new("fuzz/seeds/radio_codecs");
    std::fs::create_dir_all(dir)?;

    let mut n = 0;
    let mut write = |name: &str, bytes: &[u8]| -> std::io::Result<()> {
        std::fs::write(dir.join(name), bytes)?;
        n += 1;
        Ok(())
    };

    // Envelopes across the size range a fragment actually takes.
    for (tag, len) in [("empty", 0usize), ("small", 24), ("mtu", 200)] {
        let env = Envelope::new(ty::DATA, ZERO_DEST, 1_700_003_600, vec![0x5A; len]).wire();
        let pkt = meshtastic::encode(&env, 0x1234_5678, 0xFFFF_FFFF, 42);
        write(&format!("pkt-{tag}.bin"), &pkt)?;
        write(&format!("stream-{tag}.bin"), &meshtastic::stream_encode(&pkt))?;
    }

    // Two frames back to back — the state the framer only reaches mid-stream.
    let env = Envelope::new(ty::DATA, ZERO_DEST, 1_700_003_600, vec![7; 32]).wire();
    let one = meshtastic::stream_encode(&meshtastic::encode(&env, 1, 2, 3));
    let mut pair = one.clone();
    pair.extend_from_slice(&one);
    write("stream-pair.bin", &pair)?;

    // Log noise before a real frame: what a serial line actually delivers.
    let mut noisy = b"INFO | booting\n".to_vec();
    noisy.extend_from_slice(&one);
    write("stream-after-noise.bin", &noisy)?;

    // A modulated audio burst, so the demodulator has one real signal to start from.
    let pcm = spore::bridge::audio::modulate(b"seed");
    let bytes: Vec<u8> = pcm.iter().map(|s| ((s * 128.0) + 128.0).clamp(0.0, 255.0) as u8).collect();
    write("audio-burst.bin", &bytes)?;

    println!("wrote {n} seeds to {}", dir.display());
    Ok(())
}
