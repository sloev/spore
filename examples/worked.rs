//! Worked examples for the reimplementation guide (`docs/REBUILD.md`).
//!
//!   cargo run --example worked
//!
//! Every number this prints is computed by the real library, so the guide's hex
//! is authoritative: a reimplementation in any language should reproduce it byte
//! for byte. Keys are derived from fixed seeds so the output is deterministic.

use ed25519_dalek::{Signature, Signer, SigningKey};
use sha2::{Digest, Sha256};
use spore::*;

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn main() {
    println!("SPORE worked examples — reproduce with `cargo run --example worked`\n");

    // ---- 1. Address = SHA-256(pubkey)[..8] --------------------------------
    let seed = [7u8; 32]; // fixed so the output is reproducible
    let sk = SigningKey::from_bytes(&seed);
    let pk = sk.verifying_key().to_bytes();
    let full = Sha256::digest(pk);
    let addr = addr_of(&pk);
    println!("[1] Address derivation");
    println!("    signing seed   : {}", hex(&seed));
    println!("    public key (32): {}", hex(&pk));
    println!("    SHA-256(pubkey): {}", hex(&full));
    println!("    address  [..8] : {}", hex(&addr));
    println!("    (address = first 8 bytes of SHA-256 of the Ed25519 public key)\n");

    // ---- 2. Topic address = SHA-256(name)[..8] ----------------------------
    let topic = topic_of("news");
    println!("[2] Topic address");
    println!(
        "    SHA-256(\"news\")[..8] = {}   (topics and addresses share one 8-byte space)\n",
        hex(&topic)
    );

    // ---- 3. Envelope wire bytes -------------------------------------------
    // A public DATA message to the "news" topic, expiring at a fixed time.
    let expiry: u32 = 1_700_000_000;
    let mut e = Envelope::new(ty::DATA, topic, expiry, b"the dam holds".to_vec());
    e.flags |= fl::FLOOD;
    e.hops = 16;
    let wire_unsigned = e.wire();
    println!("[3] Envelope (unsigned) wire layout");
    println!("    ver   typ flags hops  expiry(4 BE)  dest(8)            plen(2)  payload");
    println!(
        "    01    00  10    10    {}     {}   {}   {}",
        hex(&expiry.to_be_bytes()),
        hex(&topic),
        hex(&(b"the dam holds".len() as u16).to_be_bytes()),
        hex(b"the dam holds")
    );
    println!("    full wire      : {}", hex(&wire_unsigned));
    println!("    id = SHA-256(wire, hops=0)[..16] = {}\n", hex(&e.id()));

    // ---- 4. Sign + verify --------------------------------------------------
    let mut s = Envelope::new(ty::DATA, topic, expiry, b"the dam holds".to_vec());
    s.flags |= fl::FLOOD;
    s.sign(&sk);
    // The signature covers the body with hops zeroed (so relays can decrement
    // hops without breaking it). Recompute it by hand to show the pre-image.
    let preimage = {
        // ver,typ,flags(with SIGNED),hops=0,expiry,dest,srcpubkey,plen,payload
        let mut b = vec![VER, ty::DATA, fl::FLOOD | fl::SIGNED, 0];
        b.extend_from_slice(&expiry.to_be_bytes());
        b.extend_from_slice(&topic);
        b.extend_from_slice(&pk);
        b.extend_from_slice(&(b"the dam holds".len() as u16).to_be_bytes());
        b.extend_from_slice(b"the dam holds");
        b
    };
    let sig: Signature = sk.sign(&preimage);
    println!("[4] Sign + verify");
    println!("    signature pre-image (hex): {}", hex(&preimage));
    println!("    signature (64)           : {}", hex(&sig.to_bytes()));
    println!("    library verify()         : {}", s.verify());
    println!("    hand-built pre-image matches library: {}\n", s.sig == Some(sig.to_bytes()));

    // ---- 5. Text armor (paper / SMS / voice) ------------------------------
    let armored = armor::wrap(&s.wire());
    let recovered = armor::unwrap(&armored).unwrap();
    println!("[5] Text armor (type this off paper into any node)");
    println!("    {}", armored);
    println!("    round-trips back to the same {} wire bytes: {}\n", recovered.len(), recovered == s.wire());

    println!("Done. A from-scratch implementation that reproduces sections 1–4 is wire-compatible.");
}
