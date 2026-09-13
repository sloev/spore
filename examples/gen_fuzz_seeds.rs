//! Write seed corpora for the fuzz targets whose input has a format worth
//! starting from: `radio_codecs` and `linkfrag_reassembly`.
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

    seed_linkfrag()?;
    Ok(())
}

/// Seeds for `linkfrag_reassembly`.
///
/// The target reads its input in 48-byte chunks and offers alternate chunks raw,
/// so a seed is most useful as a *concatenation* of real link fragments: that
/// gives the fuzzer a well-formed 7-byte header and a plausible set to mutate
/// rather than 65 535 random counts, which is the part it would otherwise spend
/// its whole budget failing to guess.
fn seed_linkfrag() -> std::io::Result<()> {
    let dir = std::path::Path::new("fuzz/seeds/linkfrag_reassembly");
    std::fs::create_dir_all(dir)?;
    let mut n = 0;
    let mut write = |name: &str, bytes: &[u8]| -> std::io::Result<()> {
        std::fs::write(dir.join(name), bytes)?;
        n += 1;
        Ok(())
    };

    let env = Envelope::new(ty::DATA, ZERO_DEST, 1_700_003_600, vec![0x5A; 300]).wire();

    // A complete set, in order: the one input that reaches the reassembly path
    // all the way to a returned frame.
    let pieces = linkfrag::split(&env, 64, 0x0001);
    write("set-complete.bin", &pieces.concat())?;

    // The same set missing its middle piece — the shape that has to age out
    // rather than accumulate, and the one a random stream almost never produces.
    let mut holed: Vec<Vec<u8>> = pieces.clone();
    if holed.len() > 2 {
        holed.remove(1);
    }
    write("set-incomplete.bin", &holed.concat())?;

    // Two sets from different senders interleaved. Per-key accounting is the
    // property the module actually promises, and one set exercises none of it.
    let other = linkfrag::split(&env, 96, 0x0002);
    let mut mixed = Vec::new();
    for i in 0..pieces.len().max(other.len()) {
        if let Some(p) = pieces.get(i) {
            mixed.extend_from_slice(p);
        }
        if let Some(p) = other.get(i) {
            mixed.extend_from_slice(p);
        }
    }
    write("sets-interleaved.bin", &mixed)?;

    // A set carrying repair symbols, whose indices sit at or past `count` — the
    // boundary an over-eager `idx < count` check gets wrong in either direction.
    write("set-with-repair.bin", &linkfrag::split_for_link(&env, 64, 0x0003).concat())?;

    println!("wrote {n} seeds to {}", dir.display());
    Ok(())
}
