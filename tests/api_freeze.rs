//! FROZEN 1.0 CONTRACT — do not edit without a major-version bump.
//!
//! This file pins SPORE's public API and wire format for v1.x. The golden hex
//! below is the on-the-wire contract: any code change that alters it is a
//! backwards-incompatible break and must fail CI. The `_frozen_api_surface`
//! function pins the public *shape* (names + signatures) at compile time —
//! removing or changing a listed item stops this file compiling.
//!
//! The PR guard (`.github/workflows/pr-guard.yml`) refuses to let a pull request
//! modify this file, so the 1.0 contract can only change by a deliberate,
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
const UNSIGNED_WIRE: &str = "010010106553f10019fba0e995b9794f000d7468652064616d20686f6c6473";
const UNSIGNED_ID: &str = "1ff3a7d10b117b007309f1164c3998f7";
const SIGNED_WIRE: &str = "010012106553f10019fba0e995b9794fea4a6c63e29c520abef5507b132ec5f9954776aebebe7b92421eea691446d22c000d7468652064616d20686f6c6473daa7ab3bd3c46dda41fd7d95800b91e242f95e43185e4cd1f394bfda7b00cac8065ecb4c63af711aa2462b950a933215a3234c6ef6b14fc55d495d179cdf3906";
const SIGNED_ID: &str = "460865a659a4a9bd3bcd0728c5f18d5e";

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
}

#[test]
fn constants_are_frozen() {
    assert_eq!(VER, 0x01);
    assert_eq!((ty::DATA, ty::INV, ty::WANT, ty::ANNOUNCE), (0, 1, 2, 3));
    assert_eq!(fl::ENCRYPTED, 1);
    assert_eq!(fl::SIGNED, 2);
    assert_eq!(fl::FRAGMENT, 4);
    assert_eq!(fl::ACKREQ, 8);
    assert_eq!(fl::FLOOD, 16);
    assert_eq!(fl::SRC8, 32);
    assert_eq!(ZERO_DEST, [0u8; 8]);
    assert_eq!(DEFAULT_MTU, 1400);
}

// ---------------------------------------------------------------------------
// Compile-time public API surface freeze. Never run; it exists so the compiler
// rejects any change to these names or signatures. Coerce free functions to
// typed fn pointers, and pin method signatures by taking them as values.
// ---------------------------------------------------------------------------
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
    let _: Vec<Forward> = n.send([0u8; 8], vec![], 0u32);
    n.subscribe("t");
    n.set_source_quota(0u32);
    n.set_store_budget(0usize);
    let _: Addr = n.addr;
}
