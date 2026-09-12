//! FROZEN v1 CONTRACT — do not edit without bumping the *wire* version.
//!
//! This file pins SPORE's public API and the v1 wire format. The golden hex
//! below is the on-the-wire contract: any code change that alters it is a
//! backwards-incompatible break and must fail CI. The `_frozen_api_surface`
//! function pins the public *shape* (names + signatures) at compile time —
//! removing or changing a listed item stops this file compiling.
//!
//! The PR guard (`.github/workflows/pr-guard.yml`) refuses to let a pull request
//! modify this file, so the v1 contract can only change by a deliberate,
//! admin-level act. The values are reproduced by `cargo run --example gen_vectors`
//! and mirrored in `reference/vectors.json` and `docs/REBUILD.md`.

use ed25519_dalek::SigningKey;
use spore::*;

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

// The deterministic key the vectors are built from (fixed seed).
const SEED: [u8; 32] = [7u8; 32];
const PUBKEY: &str = "ea4a6c63e29c520abef5507b132ec5f9954776aebebe7b92421eea691446d22c";
const ADDR: &str = "fe812c12f3ab4ce6";
const TOPIC_NEWS: &str = "19fba0e995b9794f";
const UNSIGNED_WIRE: &str = "020010106553f10019fba0e995b9794f000d7468652064616d20686f6c6473";
const UNSIGNED_ID: &str = "b0862c14c3be84bc5df1bfa8ab5adacb";
const SIGNED_WIRE: &str = "020012106553f10019fba0e995b9794fea4a6c63e29c520abef5507b132ec5f9954776aebebe7b92421eea691446d22c000d7468652064616d20686f6c647332e3cabd4269155f2477873f84d1e34632d9e56574fb705a9950ea7b1a3ca1f757e2dbf82367d743c0107770faa42a84f4f5b4cd0966df6f4ad53ada8de89802";
const SIGNED_ID: &str = "36a89b143679d20640a6b9cd4d8e4f12";

#[test]
fn wire_format_and_identity_are_frozen() {
    let sk = SigningKey::from_bytes(&SEED);
    let pk = sk.verifying_key().to_bytes();
    assert_eq!(hex(&pk), PUBKEY, "public key derivation changed");
    assert_eq!(hex(&addr_of(&pk)), ADDR, "address = SHA-256(pubkey)[..8] changed");
    assert_eq!(hex(&topic_of("news")), TOPIC_NEWS, "topic derivation changed");

    let topic = topic_of("news");
    let mut e = Envelope::new(ty::DATA, topic, 1_700_000_000, b"the dam holds".to_vec());
    e.flags |= fl::FLOOD;
    assert_eq!(hex(&e.wire()), UNSIGNED_WIRE, "unsigned wire layout changed");
    assert_eq!(hex(&e.id()), UNSIGNED_ID, "message id (SHA-256, hops=0)[..16] changed");

    let mut s = Envelope::new(ty::DATA, topic, 1_700_000_000, b"the dam holds".to_vec());
    s.flags |= fl::FLOOD;
    s.sign(&sk);
    assert_eq!(hex(&s.wire()), SIGNED_WIRE, "signed wire (deterministic Ed25519) changed");
    assert_eq!(hex(&s.id()), SIGNED_ID, "signed message id changed");
    assert!(s.verify(), "signature must verify");

    // decode(wire) round-trips and still verifies.
    let (d, n) = Envelope::decode(&s.wire()).expect("decode");
    assert_eq!(n, s.wire().len(), "decode must consume the whole wire");
    assert!(d.verify(), "decoded envelope must verify");

    // A one-bit payload flip breaks the signature.
    let mut tampered = s.wire();
    let p = tampered.len() - 64 - 13;
    tampered[p] ^= 1;
    let (t, _) = Envelope::decode(&tampered).unwrap();
    assert!(!t.verify(), "tampered envelope must not verify");
}

#[test]
fn text_armor_is_frozen() {
    let sk = SigningKey::from_bytes(&SEED);
    let mut s = Envelope::new(ty::DATA, topic_of("news"), 1_700_000_000, b"the dam holds".to_vec());
    s.flags |= fl::FLOOD;
    s.sign(&sk);
    let a = armor::wrap(&s.wire());
    assert!(a.starts_with("~S1.") && a.ends_with('~'), "armor envelope format changed");
    assert_eq!(armor::unwrap(&a).as_deref(), Some(&s.wire()[..]), "armor did not round-trip");
}

#[test]
fn crypto_primitives_are_frozen() {
    // Sealed boxes.
    let (sec, pubk) = prekey_keypair();
    let sealed = seal(b"north pier midnight", &pubk);
    assert_eq!(open_sealed(&sealed, &sec).as_deref(), Some(&b"north pier midnight"[..]));

    // Encrypted topics.
    let psk = [0x42u8; 32];
    let ct = topic_seal(b"safehouse moved", &psk);
    assert_eq!(topic_open(&ct, &psk).as_deref(), Some(&b"safehouse moved"[..]));
    assert!(topic_open(&ct, &[0u8; 32]).is_none(), "wrong key must not open");

    // KEYROT: rotation is one-way; epoch keys derive forward.
    let root = [9u8; 32];
    assert_eq!(topic::epoch_key(&root, 2), topic::rotate(&topic::rotate(&root)));
    let (msec, mpub) = prekey_keypair();
    let boxed = topic::rekey_seal(&[1u8; 32], &mpub);
    assert_eq!(topic::rekey_open(&boxed, &msec), Some([1u8; 32]));

    // Contributory rotation: the derivation must not drift, or two nodes on
    // different releases stop converging on the same key.
    assert_eq!(
        hex(&topic::mix(&[3u8; 32], &[4u8; 32])),
        "d4a41fb52e20bdb893d009df4930d2e32cf1d94fa8b739ab696e3445ae0e50e0",
        "topic::mix derivation changed"
    );
    assert_eq!(hex(&topic::key_id(&[3u8; 32])), "a1cb46c1", "topic::key_id derivation changed");
    // And a contribution must survive the round trip it exists for.
    let (healed, msg) = topic::contribute(&[7u8; 32], &[mpub]);
    assert_eq!(topic::absorb(&[7u8; 32], &msg, &msec), Some(healed));
    assert!(topic::absorb(&[7u8; 32], &msg, &[0u8; 32]).is_none(), "a non-member cannot absorb");
}

#[test]
fn constants_are_frozen() {
    // M12: the header kept its shape and changed the meaning of `created_at`,
    // which is precisely what a version byte is for.
    assert_eq!(VER, 0x02);
    assert_eq!((ty::DATA, ty::INV, ty::WANT, ty::ANNOUNCE), (0, 1, 2, 3));
    assert_eq!(fl::ENCRYPTED, 1);
    assert_eq!(fl::SIGNED, 2);
    assert_eq!(fl::FRAGMENT, 4);
    assert_eq!(fl::ACKREQ, 8);
    assert_eq!(fl::FLOOD, 16);
    assert_eq!(fl::SRC8, 32);
    assert_eq!(ZERO_DEST, [0u8; 8]);
    assert_eq!(DEFAULT_MTU, 1400);

    // --- the file layer's two frozen facts (M11-M) -------------------------
    //
    // A chunk's name must come out identical on two implementations that have
    // never spoken, or a file published twice is two unrelated files. That needs
    // both halves pinned: the payload shape, and how it is hashed.
    assert_eq!(file::CHUNK_TAG, 0x07);
    assert_eq!(file::CHUNK_BYTES, 4096, "a static size, and not derived from any MTU");

    let chunk_payload = {
        let mut v = vec![file::CHUNK_TAG];
        v.extend_from_slice(b"the dam holds");
        v
    };
    assert_eq!(hex(&chunk_payload), "077468652064616d20686f6c6473", "[CHUNK_TAG][bytes], nothing else");
    assert_eq!(
        hex(&file::content_id(&chunk_payload)),
        "17679394b9bac1cf0bdea0ef71815f51",
        "content id = SHA-256(payload)[..16] — the payload alone, never the envelope"
    );
}

// ---------------------------------------------------------------------------
// Compile-time public API surface freeze. Never run; it exists so the compiler
// rejects any change to these names or signatures. Coerce free functions to
// typed fn pointers, and pin method signatures by taking them as values.
// ---------------------------------------------------------------------------
// Named so the coercions below read as contracts rather than as type puzzles. The
// signatures, not the names, are what is frozen.
type Contribute = fn(&[u8; 32], &[[u8; 32]]) -> ([u8; 32], Vec<u8>);
type Absorb = fn(&[u8; 32], &[u8], &[u8; 32]) -> Option<[u8; 32]>;

#[test]
fn public_api_surface_is_frozen() {
    // Free functions (exact signatures).
    let _: fn(&[u8; 32]) -> Addr = addr_of;
    let _: fn(&str) -> Addr = topic_of;
    let _: fn(&[u8], &[u8; 32]) -> Vec<u8> = seal;
    let _: fn(&[u8], &[u8; 32]) -> Option<Vec<u8>> = open_sealed;
    let _: fn(&[u8], &[u8; 32]) -> Vec<u8> = topic_seal;
    let _: fn(&[u8], &[u8; 32]) -> Option<Vec<u8>> = topic_open;
    let _: fn() -> ([u8; 32], [u8; 32]) = prekey_keypair;
    let _: fn(&[u8]) -> String = armor::wrap;
    let _: fn(&str) -> Option<Vec<u8>> = armor::unwrap;
    let _: fn(&[u8; 32]) -> [u8; 32] = topic::rotate;
    let _: fn(&[u8; 32], &[u8; 32]) -> [u8; 32] = topic::mix;
    let _: fn(&[u8; 32]) -> [u8; 4] = topic::key_id;
    let _: Contribute = topic::contribute;
    let _: Absorb = topic::absorb;

    // Envelope methods.
    let _: fn(u8, Addr, u32, Vec<u8>) -> Envelope = Envelope::new;
    let _: fn(&Envelope) -> Vec<u8> = Envelope::wire;
    let _: fn(&Envelope) -> Id = Envelope::id;
    let _: fn(&Envelope) -> u8 = Envelope::stamp;
    let _: fn(&Envelope) -> bool = Envelope::verify;
    let _: fn(&mut Envelope, &SigningKey) = Envelope::sign;

    // Node constructor + core method signatures (pinned by never-run helpers).
    let _: fn(&str, &[&str]) -> Node = Node::new;
}

// Never called — exists only so the compiler pins these method signatures.
#[allow(dead_code)]
fn _frozen_node_api(n: &mut Node) {
    let _: Rx = n.on_rx(&[], 0 as Iface, None::<Addr>, 0u32);
    let _: Vec<Forward> = n.originate([0u8; 8], vec![], 0u32);
    let _: Result<Vec<Forward>, spore::TooLarge> = n.send([0u8; 8], vec![], 0u32);
    n.subscribe("t");
    n.set_source_quota(0u32);
    n.set_store_budget(0usize);
    let _: Addr = n.addr;
}
