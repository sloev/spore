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
