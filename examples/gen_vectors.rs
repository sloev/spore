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
    let created_at: u32 = 1_700_000_000;
    let payload = b"the dam holds".to_vec();

    let mut unsigned = Envelope::new(ty::DATA, topic, created_at, payload.clone());
    unsigned.flags |= fl::FLOOD;

    let mut signed = Envelope::new(ty::DATA, topic, created_at, payload);
    signed.flags |= fl::FLOOD;
    signed.sign(&sk);

    // --- file layer: the chunk, and the content id that names it (M11-M) ------
    //
    // Frozen because it is the one derivation two implementations must agree on
    // *without* exchanging anything: an envelope id is the hash of a message, so
    // two nodes naturally produce different ones for the same bytes, while a
    // content id has to come out identical on both or a file published twice is
    // two unrelated files. A chunk payload is `[CHUNK_TAG][bytes]` and nothing
    // else — no file id, no index — which is exactly what makes that possible.
    let chunk_payload = {
        let mut v = vec![file::CHUNK_TAG];
        v.extend_from_slice(b"the dam holds");
        v
    };
    let chunk_content_id = file::content_id(&chunk_payload);

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
    println!("  \"tampered_wire\": \"{}\",", hex(&tampered_wire));
    println!("  \"chunk_tag\": \"{:02x}\",", file::CHUNK_TAG);
    println!("  \"chunk_bytes\": {},", file::CHUNK_BYTES);
    println!("  \"chunk_payload\": \"{}\",", hex(&chunk_payload));
    println!("  \"chunk_content_id\": \"{}\"", hex(&chunk_content_id));
    println!("}}");
}
