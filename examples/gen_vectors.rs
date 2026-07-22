//! Emit cross-language test vectors as JSON — the source of truth for the
//! reference decoders under `reference/`. Deterministic (fixed key seed).
//!
//!   cargo run --example gen_vectors > reference/vectors.json

use ed25519_dalek::SigningKey;
use sha2::{Digest, Sha256};
use spore::*;

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn main() {
    let seed = [7u8; 32];
    let sk = SigningKey::from_bytes(&seed);
    let pk = sk.verifying_key().to_bytes();
    let addr = addr_of(&pk);
    let topic = topic_of("news");
    let expiry: u32 = 1_700_000_000;
    let payload = b"the dam holds".to_vec();

    let mut unsigned = Envelope::new(ty::DATA, topic, expiry, payload.clone());
    unsigned.flags |= fl::FLOOD;

    let mut signed = Envelope::new(ty::DATA, topic, expiry, payload);
    signed.flags |= fl::FLOOD;
    signed.sign(&sk);

    // A tampered copy (flip one payload byte) — verification must fail on it.
    let mut tampered_wire = signed.wire();
    let plen_pos = tampered_wire.len() - 64 - 13; // start of the 13-byte payload
    tampered_wire[plen_pos] ^= 0x01;

    println!("{{");
    println!("  \"seed\": \"{}\",", hex(&seed));
    println!("  \"pubkey\": \"{}\",", hex(&pk));
    println!("  \"addr\": \"{}\",", hex(&addr));
    println!("  \"topic_news\": \"{}\",", hex(&topic));
    println!("  \"sha256_pubkey\": \"{}\",", hex(&Sha256::digest(pk)));
    println!("  \"unsigned_wire\": \"{}\",", hex(&unsigned.wire()));
    println!("  \"unsigned_id\": \"{}\",", hex(&unsigned.id()));
    println!("  \"signed_wire\": \"{}\",", hex(&signed.wire()));
    println!("  \"signed_id\": \"{}\",", hex(&signed.id()));
    println!("  \"armor\": \"{}\",", armor::wrap(&signed.wire()));
    println!("  \"tampered_wire\": \"{}\"", hex(&tampered_wire));
    println!("}}");
}
