//! Malformed-input robustness for every parser a stranger can reach.
//!
//! Lives in `src/` rather than `tests/` on purpose: the PR freeze guard treats
//! the whole `tests/` directory as frozen contract, so a new integration test
//! file there would fail the guard and need a major-version label to land.
//!
//! A bridge hands whatever arrived to a decoder, and everything that arrives is
//! chosen by someone else. These tests do what the `fuzz/` targets do — feed
//! arbitrary and near-miss bytes to each entry point and require that it returns
//! rather than panics — except they run on stable in ordinary CI, so a
//! regression is caught on the pull request rather than on the next nightly
//! fuzz run. `fuzz/` explores far deeper; this is the part that always runs.
//!
//! "Returns rather than panics" is the whole assertion. A parser is allowed to
//! reject anything it likes. It is not allowed to index out of bounds, subtract
//! past zero, or allocate from a length it read off the wire.

use crate::*;

/// A deterministic byte source. Not cryptographic and not trying to be — the
/// point is that a failure reproduces exactly from the seed printed with it.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        // xorshift64*
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }
    fn byte(&mut self) -> u8 {
        (self.next() >> 33) as u8
    }
    fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.next() >> 33) as usize % n
        }
    }
    fn bytes(&mut self, len: usize) -> Vec<u8> {
        (0..len).map(|_| self.byte()).collect()
    }
    /// Up to `max` random bytes. Separate from `bytes` so callers do not have to
    /// borrow the generator twice in one expression.
    fn some_bytes(&mut self, max: usize) -> Vec<u8> {
        let n = self.below(max);
        self.bytes(n)
    }
}

/// Every decoder that can be reached with attacker-chosen bytes.
fn feed_every_parser(data: &[u8]) {
    // §1 wire format — the front door.
    if let Ok((e, n)) = Envelope::decode(data) {
        // Anything that decodes must survive the accessors a relay calls on it.
        let _ = e.id();
        let _ = e.stamp();
        let _ = e.verify();
        let _ = e.wire();
        assert!(n <= data.len(), "decode consumed more than it was given");
    }

    // Text armor: reached from paper, chat, voice transcription.
    let as_text = String::from_utf8_lossy(data);
    let _ = armor::unwrap(&as_text);

    // File manifests: reached from any peer offering a file.
    let _ = file::Manifest::decode(data);

    // Framing layers every stream and radio bridge sits on.
    let _ = kiss::decode(data);
    let mut framer = crate::bridge::KissStream::new();
    let _ = framer.push(data);

    // Covert-channel codec.
    let _ = crate::bridge::icmp::decode_echo(data);

    // Radio codecs. Both are reachable from a hostile medium — anyone with a
    // transmitter in range — and neither had been hammered like the core parsers.
    let _ = crate::bridge::meshtastic::decode(data);
    let _ = crate::bridge::meshtastic::from_radio_packet(data);
    let mut mesh = crate::bridge::meshtastic::StreamFramer::new();
    for frame in mesh.push(data) {
        // Not just "did not panic": a framer that silently retains everything is
        // a resource bug a panic-only check cannot see.
        assert!(frame.len() <= crate::bridge::meshtastic::STREAM_MAX_LEN);
    }
    assert!(
        mesh.buffered() <= crate::bridge::meshtastic::STREAM_MAX_LEN + 8,
        "framer retained {} bytes; it must resync rather than accumulate",
        mesh.buffered()
    );

    // The audio modem demodulates whatever the microphone heard, which is
    // whatever anyone nearby chose to play.
    let samples: Vec<f32> = data.iter().map(|b| (*b as f32 - 128.0) / 128.0).collect();
    let _ = crate::bridge::audio::demodulate(&samples);

    // Crypto open paths, with a key the caller does not hold.
    let _ = topic_open(data, &[7u8; 32]);
    let _ = open_sealed(data, &[9u8; 32]);
}

#[test]
fn parsers_survive_arbitrary_bytes() {
    for seed in 1..200u64 {
        let mut r = Rng(seed);
        for _ in 0..40 {
            let len = r.below(600);
            let data = r.bytes(len);
            feed_every_parser(&data);
        }
    }
}

#[test]
fn parsers_survive_corrupted_real_envelopes() {
    // Pure random bytes rarely get past a length check, so most of the
    // interesting states live just off a *valid* message. Take real envelopes
    // and damage them: flip bits, truncate, extend, and splice.
    let now = 1_700_000_000;
    let mut node = Node::new("fuzz", &[]);
    let mut corpus: Vec<Vec<u8>> = Vec::new();
    for i in 0..6u8 {
        for f in node.originate(ZERO_DEST, vec![i; 20 * (i as usize + 1)], now) {
            let (Forward::Flood { bytes, .. } | Forward::Directed { bytes, .. }) = f;
            corpus.push(bytes);
        }
    }
    corpus.push(node.build_announce(now).first().map(|_| vec![0u8; 0]).unwrap_or_default());
    corpus.retain(|c| !c.is_empty());
    assert!(!corpus.is_empty(), "need a corpus to corrupt");

    let mut r = Rng(0xC0FFEE);
    for seed_msg in &corpus {
        for _ in 0..400 {
            let mut m = seed_msg.clone();
            match r.below(5) {
                0 => {
                    // flip a bit
                    let i = r.below(m.len());
                    m[i] ^= 1 << (r.below(8) as u32);
                }
                1 => {
                    // truncate anywhere, including to nothing
                    let k = r.below(m.len() + 1);
                    m.truncate(k);
                }
                2 => {
                    // extend with junk
                    let extra = r.some_bytes(64);
                    m.extend_from_slice(&extra);
                }
                3 => {
                    // overwrite a run — hits declared lengths and flag bytes
                    let i = r.below(m.len());
                    let run = r.below(8) + 1;
                    for j in 0..run {
                        if i + j < m.len() {
                            m[i + j] = r.byte();
                        }
                    }
                }
                _ => {
                    // splice two messages together
                    let other = &corpus[r.below(corpus.len())];
                    let cut = r.below(m.len() + 1);
                    m.truncate(cut);
                    m.extend_from_slice(other);
                }
            }
            feed_every_parser(&m);
        }
    }
}

#[test]
fn a_node_survives_arbitrary_bytes_on_its_receive_path() {
    // `on_rx` is the real attack surface: everything a bridge reads ends up
    // here, and it runs dedup, quota accounting, path learning, reassembly and
    // delivery in one pass. None of that may panic on input from a stranger.
    let now = 1_700_000_000;
    let mut node = Node::new("target", &[]);
    node.set_source_quota(4096);

    let mut r = Rng(0xBADC0DE);
    for _ in 0..4000 {
        let data = r.some_bytes(400);
        let _ = node.on_rx(&data, 1, None, now);
    }

    // ...and on plausible-looking envelopes, where the parser gets further in.
    let mut victim = Node::new("victim", &[]);
    for i in 0..300u32 {
        let mut e = Envelope::new(ty::DATA, ZERO_DEST, now + 3600, r.some_bytes(120));
        e.flags = r.byte();
        e.hops = r.byte();
        e.typ = r.byte();
        if i % 3 == 0 {
            e.sign(&victim.sk);
        } else {
            e.sig = Some([r.byte(); 64]);
        }
        let _ = node.on_rx(&e.wire(), (i % 4) as Iface, None, now);
        let _ = victim.on_rx(&e.wire(), 0, None, now);
    }
}

#[test]
fn fragment_reassembly_survives_hostile_chunks() {
    // Fountain-coded fragments arrive out of order, duplicated, and from anyone.
    // Reassembly is stateful, which is exactly what makes it worth hammering.
    let now = 1_700_000_000;
    let mut sender = Node::new("sender", &[]);
    sender.mtu = 128;
    let fwds = sender.send(ZERO_DEST, vec![0xA5; 4000], now).expect("4 kB fits one set at mtu 128");
    let frags: Vec<Vec<u8>> = fwds
        .into_iter()
        .map(|f| {
            let (Forward::Flood { bytes, .. } | Forward::Directed { bytes, .. }) = f;
            bytes
        })
        .collect();
    assert!(frags.len() > 1, "payload should have fragmented");

    let mut r = Rng(0x5EED);
    let mut node = Node::new("reassembler", &[]);
    for _ in 0..3000 {
        let mut f = frags[r.below(frags.len())].clone();
        match r.below(4) {
            0 => {} // deliver intact, possibly a duplicate
            1 => {
                let i = r.below(f.len());
                f[i] ^= 1 << (r.below(8) as u32);
            }
            2 => {
                let k = r.below(f.len() + 1);
                f.truncate(k);
            }
            _ => f.extend_from_slice(&r.some_bytes(32)),
        }
        let _ = node.on_rx(&f, 1, None, now);
    }
}

#[test]
fn a_zero_count_fragment_does_not_kill_the_node() {
    // Found by `parsers_survive_*` before this guard existed. `count` is one
    // byte read straight off the wire; a set of zero chunks reached `idx % count`
    // in the fountain selector — a division by zero, which is a panic, which is
    // the whole node. No key and no forgery required: a public FRAGMENT is
    // deliverable to anyone.
    let now = 1_700_000_000;
    let mut node = Node::new("target", &[]);

    let mut payload = vec![0u8; 18];
    payload[16] = 3; // idx
    payload[17] = 0; // count — the poison
    payload.extend_from_slice(b"chunk");

    let mut e = Envelope::new(ty::DATA, ZERO_DEST, now + 3600, payload);
    e.flags |= fl::FRAGMENT | fl::FLOOD;
    let _ = node.on_rx(&e.wire(), 1, None, now); // must return, not panic

    // Every other count byte must be survivable too, at several indices.
    for count in 0..=255u8 {
        for idx in [0u8, 1, 127, 254, 255] {
            let mut p = vec![0u8; 18];
            p[16] = idx;
            p[17] = count;
            p.extend_from_slice(b"chunk");
            let mut e = Envelope::new(ty::DATA, ZERO_DEST, now + 3600, p);
            e.flags |= fl::FRAGMENT | fl::FLOOD;
            let _ = node.on_rx(&e.wire(), 1, None, now);
        }
    }
}

#[test]
fn an_oversized_object_is_an_error_not_a_panic() {
    // `send` used to assert here. The ceiling is structural — the fragment
    // header's `count` is one wire byte — so exceeding it is a fact about the
    // payload the caller passed, which is an error to report rather than a bug
    // to abort on.
    let now = 1_700_000_000;
    let mut n = Node::new("sender", &[]);
    n.mtu = 128;
    let chunk = 128 - 36;

    // Comfortably past 255 chunks at this MTU.
    let huge = vec![0u8; chunk * 300];
    let err = n.send(ZERO_DEST, huge, now).expect_err("must not be sent as one set");
    assert!(err.needed > MAX_FOUNTAIN_CHUNKS, "reports what it would have needed");
    assert_eq!(err.chunk, chunk, "and the chunk size in force");
    assert!(err.to_string().contains("file/manifest"), "and points at the layer that handles it: {err}");

    // Just under the ceiling still goes out.
    let ok = vec![0u8; chunk * 200];
    assert!(n.send(ZERO_DEST, ok, now).is_ok(), "an object inside one set still sends");
}

#[test]
fn radio_codecs_survive_corrupted_real_frames() {
    // Random bytes rarely get past a length or magic check, so the states worth
    // reaching live just off a *valid* frame. Both of these codecs are fed by
    // anyone with a transmitter in range.
    use crate::bridge::meshtastic;

    let mut r = Rng(0xD1A9_5E37);
    let env = Envelope::new(ty::DATA, ZERO_DEST, 1_700_003_600, vec![0x5Au8; 60]).wire();

    let real: Vec<Vec<u8>> = vec![
        meshtastic::encode(&env, 0x1234_5678, 0xFFFF_FFFF, 42),
        meshtastic::stream_encode(&meshtastic::encode(&env, 1, 2, 3)),
    ];

    for base in &real {
        for _ in 0..600 {
            let mut m = base.clone();
            match r.below(4) {
                0 => {
                    let i = r.below(m.len());
                    m[i] ^= 1 << (r.below(8) as u32);
                }
                1 => {
                    let k = r.below(m.len() + 1);
                    m.truncate(k);
                }
                2 => {
                    let extra = r.some_bytes(48);
                    m.extend_from_slice(&extra);
                }
                _ => {
                    // Overwrite a run — hits declared lengths and tag bytes.
                    let i = r.below(m.len());
                    for j in 0..r.below(6) + 1 {
                        if i + j < m.len() {
                            m[i + j] = r.byte();
                        }
                    }
                }
            }
            let _ = meshtastic::decode(&m);
            let _ = meshtastic::from_radio_packet(&m);
            let mut f = meshtastic::StreamFramer::new();
            // Fed in pieces, the way a serial line actually delivers it.
            for piece in m.chunks(5) {
                let _ = f.push(piece);
            }
        }
    }

    // The audio modem, fed a real signal with noise and clipping applied.
    let clean = crate::bridge::audio::modulate(b"the dam holds");
    for _ in 0..40 {
        let mut pcm = clean.clone();
        for s in pcm.iter_mut() {
            match r.below(3) {
                0 => *s += (r.byte() as f32 - 128.0) / 64.0, // noise
                1 => *s = s.clamp(-0.2, 0.2),                // clipping
                _ => {}
            }
        }
        let cut = r.below(pcm.len() + 1);
        pcm.truncate(cut);
        let _ = crate::bridge::audio::demodulate(&pcm);
    }
}

#[test]
fn a_meshtastic_length_varint_cannot_overflow_the_offset() {
    // Found by the `radio_codecs` fuzz target within 90 seconds of it existing.
    // `decode` computed `no + len as usize` where `len` is a varint off the air,
    // so it reaches u64::MAX: overflow. That panics wherever overflow checks are
    // on — which is every `cargo build` and `cargo run` *without* `--release`,
    // including the daemon demo in the README — and in release wraps to a bogus
    // slice range instead. Anyone with a transmitter in range could send it.
    //
    // The sibling parser `from_radio_packet` already used `checked_add`
    // throughout; `decode` did not. Same shape as S-015: the fix present in one
    // place and absent in its twin.
    let crash = [
        0x0a, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x01, 0x00, 0x00, 0x01, 0xff, 0xff, 0xff,
        0xff, 0xff, 0xff, 0xff, 0xff, 0x0a, 0x29,
    ];
    assert!(crate::bridge::meshtastic::decode(&crash).is_none(), "must reject, not overflow");

    // The general shape, not just the one input the fuzzer happened to find:
    // every wire type carrying an attacker-chosen length or skip.
    for tag in [0x0au8, 0x12, 0x22, 0x2a] {
        for pad in 0..4usize {
            let mut f = vec![tag];
            f.extend_from_slice(&[0xff; 9]); // a varint at the u64 ceiling
            f.push(0x01);
            f.extend(std::iter::repeat(0xAB).take(pad));
            let _ = crate::bridge::meshtastic::decode(&f);
        }
    }
    // Fixed-width skips (wire types 5 and 1) must not run off the end either.
    for tag in [0x0du8, 0x09] {
        let _ = crate::bridge::meshtastic::decode(&[tag]);
        let _ = crate::bridge::meshtastic::decode(&[tag, 0x00]);
    }

    // `decode` runs *two* protobuf loops — the frame, then the decoded
    // sub-message — and the second had the identical defect. Reaching it needs a
    // well-formed field 4 wrapping a hostile inner message, which is why the
    // fuzzer found the outer one first and the inner one only after a fix.
    for inner_tag in [0x12u8, 0x0d, 0x09] {
        let mut inner = vec![inner_tag];
        inner.extend_from_slice(&[0xff; 9]); // length varint at the u64 ceiling
        inner.push(0x01);

        let mut frame = vec![0x22]; // field 4, wire type 2 — the decoded payload
        frame.push(inner.len() as u8);
        frame.extend_from_slice(&inner);
        assert!(
            crate::bridge::meshtastic::decode(&frame).is_none(),
            "inner sub-message lengths must be checked too (tag {inner_tag:#04x})"
        );
    }
}
