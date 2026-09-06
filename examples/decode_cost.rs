//! What reassembly costs, per hop (M11-C).
//!
//!     cargo run --release --example decode_cost
//!
//! M11-C's delivery measurements say a real erasure code is worth far more than
//! repetition — 97% against 70% for the same redundancy. They say nothing about
//! **CPU**, and that omission matters now in a way it did not before: the
//! fountain used to run once, at the destination, and link fragmentation moved
//! reassembly to *every hop*. On an ESP32 relaying LoRa, every fragmented frame
//! would pay whatever this costs.
//!
//! So this measures the thing the bandwidth numbers cannot see: plain
//! reassembly, which is a map insert and a concatenation, against fountain
//! decode, which is Gaussian elimination over GF(2).
//!
//! Reported as a **ratio**, because that is the part that survives leaving this
//! machine. Absolute microseconds here are a desktop core with a warm cache and
//! say little about a 240 MHz Xtensa; the ratio between two algorithms on the
//! same data says a great deal. Per this repo's rules, no claim is made about
//! device performance that was not run on a device.

use spore::linkfrag;
use spore::*;
use std::time::Instant;

/// Enough repetitions that a single scheduling hiccup does not become the
/// result, and few enough that this stays a few seconds.
const REPS: usize = 2000;

fn envelope_of(payload: usize) -> Vec<u8> {
    let mut e = Envelope::new(ty::DATA, ZERO_DEST, 1_700_003_600, vec![0x5A; payload]);
    e.flags |= fl::FLOOD;
    e.wire()
}

/// Plain link reassembly: what ships today.
fn plain(wire: &[u8], mtu: usize) -> (usize, f64) {
    let pieces = linkfrag::split(wire, mtu, 1);
    let n = pieces.len();
    let t = Instant::now();
    for _ in 0..REPS {
        let mut r = linkfrag::Reassembler::default();
        let mut out = None;
        for p in &pieces {
            out = r.accept(0, p, 1_700_000_000);
        }
        std::hint::black_box(&out);
    }
    (n, t.elapsed().as_secs_f64() / REPS as f64 * 1e6)
}

/// Fountain decode over the same wire, split the same way — the candidate.
fn fountain_decode(wire: &[u8], mtu: usize) -> (usize, f64) {
    // Same usable payload per frame as the link fragmenter, so the two are
    // compared on the same number of pieces rather than on different splits.
    let chunk = mtu - linkfrag::LINK_FRAG_OVERHEAD;
    let count = wire.len().div_ceil(chunk);
    let orig: Id = [0x11; 16];
    // **The realistic case, not the flattering one.** Feeding only data symbols
    // measures nothing: each is a unit vector, the elimination has no work to
    // do, and the fountain looks free. On a lossy link — the only reason to want
    // a code at all — some data symbols are missing and repair symbols stand in,
    // and *that* is when Gaussian elimination runs.
    //
    // So: emit count + lost repair symbols, drop `lost` of the data symbols, and
    // decode from what is left. `lost` scales with the set, because a longer set
    // loses more pieces at the same frame loss rate.
    // Enough symbols that the decode actually resolves. A fountain needs `count`
    // *linearly independent* symbols, and repair symbols are random combinations
    // — so exactly `count` of them is usually rank-deficient. The crate's own
    // docs say count+2 typically suffices; a margin of 8 keeps every row here
    // solving. Getting this wrong is not a small error: an under-supplied
    // decoder gives up early and times *fast*, which is how the first version of
    // this measurement concluded the fountain was free.
    let lost = (count / 5).max(1);
    let margin = 8;
    let indices: Vec<u8> = (0..(count + lost + margin) as u8).collect();
    let frags = fragment(wire, chunk, 0, 0, ZERO_DEST, orig, &indices);
    let bodies: Vec<(u8, Vec<u8>)> = frags
        .iter()
        .map(|f| (f.payload[16], f.payload[18..].to_vec()))
        .filter(|(idx, _)| (*idx as usize) >= lost) // the first `lost` never arrived
        .collect();

    // Prove the decode actually resolves before timing it. A decoder that gives
    // up early is fast and useless, and timing one would be the second flattering
    // measurement in a row.
    {
        let mut f = Fountain::new();
        let mut out = None;
        for (idx, body) in &bodies {
            out = f.add(&orig, *idx, count as u8, body.clone());
        }
        let got = out.expect("the fountain must recover the wire from the symbols that arrived");
        // The fountain pads to `count * chunk` on the way out and trims on the
        // way back, so what returns is the wire, not the padded set.
        assert_eq!(&got[..wire.len()], wire, "and recover it byte for byte");
    }

    let t = Instant::now();
    for _ in 0..REPS {
        let mut f = Fountain::new();
        let mut out = None;
        for (idx, body) in &bodies {
            out = f.add(&orig, *idx, count as u8, body.clone());
        }
        std::hint::black_box(&out);
    }
    (count, t.elapsed().as_secs_f64() / REPS as f64 * 1e6)
}

fn main() {
    println!("Reassembly cost per envelope, {REPS} reps, release build.");
    println!("Ratios are the portable part; the microseconds are this machine.\n");
    println!(
        "{:>7} {:>6} {:>7} {:>10} {:>11} {:>7} {:>14}",
        "payload", "mtu", "pieces", "plain µs", "fountain µs", "ratio", "ppm of airtime"
    );

    for (payload, mtu) in
        [(900usize, 237usize), (4000, 237), (900, 1400), (16_000, 237), (16_000, 1400), (900, 54)]
    {
        let wire = envelope_of(payload);
        let (n, p) = plain(&wire, mtu);
        let (nf, f) = fountain_decode(&wire, mtu);
        assert_eq!(n, nf, "both must split into the same number of pieces to be comparable");
        // The ratio is the wrong question on a radio. What matters is decode
        // time against the time the link takes to *deliver* the pieces: a
        // LoRa-class link moves roughly 1 kB/s, so an envelope that takes half a
        // minute to arrive can afford a great deal of arithmetic.
        const LINK_BYTES_PER_SEC: f64 = 1000.0;
        let airtime_us = (n * mtu) as f64 / LINK_BYTES_PER_SEC * 1e6;
        println!(
            "{payload:>7} {mtu:>6} {n:>7} {p:>10.1} {f:>11.1} {:>6.1}x {:>14.1}",
            f / p,
            f / airtime_us * 1e6
        );
    }

    println!(
        "\nBoth are per *hop* now, not per delivery: link fragmentation moved\n\
         reassembly below the node, so a relay pays this for every fragmented\n\
         frame it carries — including an ESP32 whose whole job is relaying.\n\n\
         The last column is decode time in parts per million of the airtime\n\
         needed to receive the same pieces at 1 kB/s. Single digits: the radio\n\
         costs five to six orders of magnitude more than the arithmetic. Even\n\
         scaled by the ~30-50x an Xtensa core is slower than this one, decoding\n\
         stays lost in the noise of receiving.\n\n\
         That scaling is an argument, not a measurement — nothing here ran on a\n\
         device, and this repo does not claim hardware numbers it did not take."
    );
}
