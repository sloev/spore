//! §7 crypto — sealing to a recipient prekey, encrypted topics, and sealed files.
//!
//! Anonymous sealed boxes in the libsodium `crypto_box_seal` shape: an ephemeral
//! keypair per message, so the ciphertext names no sender.
//!
//! **This path has no forward secrecy today, and this comment used to claim it
//! did.** §7 describes rotating prekeys daily and deleting the private half after
//! seven days, concluding that "a seized device cannot read week-old mail". No such
//! rotation exists: [`Node::from_seed`] derives one prekey for the life of the
//! identity, `SHA-256(seed ‖ "spore/prekey/v1")`, and ANNOUNCE carries that same
//! key forever. Worse, deleting it would achieve nothing — it is a pure function of
//! the identity seed that [`Node::seed`] persists, so anyone holding the seed
//! re-derives it and opens every message ever sealed to that node. See S-022 in
//! `docs/SECURITY_FINDINGS.md`. Sessions ([`crate::ratchet`]) do have forward
//! secrecy; the one-shot seal does not.
//!
//! Extracted from `lib.rs` unchanged: same primitives, same bytes on the wire,
//! re-exported so `spore::seal`, `spore::open_sealed`, `spore::topic_seal`,
//! `spore::topic_open` and `spore::prekey_keypair` keep resolving as before.
//!
//! Every `open` here takes attacker-chosen ciphertext and returns `Option`, never
//! panicking on a short buffer or a wrong key — the property `src/robustness.rs`
//! and the `seal_open` fuzz target both hammer.

use crate::*;

// ---------------------------------------------------------------------------
// §7 Crypto — seal to a recipient prekey (libsodium crypto_box_seal shape).
// No forward secrecy: one prekey per identity, derived from the seed. See the
// module docs and S-022.
// ---------------------------------------------------------------------------

pub(crate) fn seal_nonce(eph_pub: &[u8; 32], recip_pub: &[u8; 32]) -> [u8; 24] {
    let mut h = Blake2bVar::new(24).unwrap();
    h.update(eph_pub);
    h.update(recip_pub);
    let mut n = [0u8; 24];
    h.finalize_variable(&mut n).unwrap();
    n
}

/// Anonymous sealed box: output = ephemeral_pubkey(32) ‖ ciphertext.
pub fn seal(msg: &[u8], recip_prekey: &[u8; 32]) -> Vec<u8> {
    use crypto_box::aead::{generic_array::GenericArray, Aead};
    use crypto_box::{PublicKey, SalsaBox, SecretKey};
    let mut sb = [0u8; 32];
    OsRng.fill_bytes(&mut sb);
    let eph = SecretKey::from(sb);
    let eph_pub = eph.public_key();
    let their = PublicKey::from(*recip_prekey);
    let nonce = seal_nonce(eph_pub.as_bytes(), their.as_bytes());
    let ct = SalsaBox::new(&their, &eph).encrypt(GenericArray::from_slice(&nonce), msg).expect("seal");
    let mut out = Vec::with_capacity(32 + ct.len());
    out.extend_from_slice(eph_pub.as_bytes());
    out.extend_from_slice(&ct);
    out
}

/// Encrypted topic (§7): seal `msg` under a 32-byte pre-shared key with
/// XChaCha20-Poly1305. Output = 24-byte random nonce ‖ ciphertext. Everyone on
/// the topic shares the key; rotate it by flooding a `KEYROT` signed by the old.
pub fn topic_seal(msg: &[u8], psk: &[u8; 32]) -> Vec<u8> {
    use chacha20poly1305::aead::{Aead, KeyInit};
    use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
    let mut nonce = [0u8; 24];
    OsRng.fill_bytes(&mut nonce);
    let ct = XChaCha20Poly1305::new(Key::from_slice(psk))
        .encrypt(XNonce::from_slice(&nonce), msg)
        .expect("topic seal");
    let mut out = Vec::with_capacity(24 + ct.len());
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ct);
    out
}

/// Open an encrypted-topic payload; `None` if the key is wrong or it's corrupt.
pub fn topic_open(ct: &[u8], psk: &[u8; 32]) -> Option<Vec<u8>> {
    use chacha20poly1305::aead::{Aead, KeyInit};
    use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
    if ct.len() < 24 {
        return None;
    }
    XChaCha20Poly1305::new(Key::from_slice(psk)).decrypt(XNonce::from_slice(&ct[..24]), &ct[24..]).ok()
}

/// The manifest name a [`Node::publish_file_sealed`] file advertises. The real
/// name is encrypted inside the sealed content, so a relay carrying the chunks
/// learns nothing but "some sealed file".
pub const SEALED_FILE_NAME: &str = "sealed";

/// Encrypt one chunk of a sealed file under that file's key.
///
/// The key is freshly generated per file, so the chunk index is a safe nonce —
/// no two chunks can ever share one, and a counter costs 24 fewer bytes per
/// chunk than a random nonce would. Adds only the 16-byte tag, which is what
/// lets a sealed chunk keep riding the same frame an unsealed one does.
pub(crate) fn chunk_seal(plain: &[u8], key: &[u8; 32], index: u32) -> Vec<u8> {
    use chacha20poly1305::aead::{Aead, KeyInit};
    use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
    let mut nonce = [0u8; 24];
    nonce[20..].copy_from_slice(&index.to_be_bytes());
    XChaCha20Poly1305::new(Key::from_slice(key))
        .encrypt(XNonce::from_slice(&nonce), plain)
        .expect("chunk seal")
}

/// Open one chunk of a sealed file. `None` if the key or index is wrong, or the
/// bytes were tampered with — the tag makes every chunk self-checking.
pub(crate) fn chunk_open(ct: &[u8], key: &[u8; 32], index: u32) -> Option<Vec<u8>> {
    use chacha20poly1305::aead::{Aead, KeyInit};
    use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
    let mut nonce = [0u8; 24];
    nonce[20..].copy_from_slice(&index.to_be_bytes());
    XChaCha20Poly1305::new(Key::from_slice(key)).decrypt(XNonce::from_slice(&nonce), ct).ok()
}

/// Fresh encryption **prekey** keypair `(secret, public)` for `seal`/`open_sealed`
/// (X25519, the same kind a `Node` rotates in its ANNOUNCE).
pub fn prekey_keypair() -> ([u8; 32], [u8; 32]) {
    let mut b = [0u8; 32];
    OsRng.fill_bytes(&mut b);
    let sec = crypto_box::SecretKey::from(b);
    let pubk = *sec.public_key().as_bytes();
    (b, pubk)
}

/// Open a sealed box (`seal`) with a prekey secret. Standalone twin of
/// `Node::open`, for callers holding the secret directly (bindings, tests).
pub fn open_sealed(sealed: &[u8], prekey_sec: &[u8; 32]) -> Option<Vec<u8>> {
    use crypto_box::aead::{generic_array::GenericArray, Aead};
    use crypto_box::{PublicKey, SalsaBox, SecretKey};
    if sealed.len() < 32 {
        return None;
    }
    let sec = SecretKey::from(*prekey_sec);
    let recip_pub = *sec.public_key().as_bytes();
    let mut ep = [0u8; 32];
    ep.copy_from_slice(&sealed[..32]);
    let eph_pub = PublicKey::from(ep);
    let nonce = seal_nonce(&ep, &recip_pub);
    SalsaBox::new(&eph_pub, &sec).decrypt(GenericArray::from_slice(&nonce), &sealed[32..]).ok()
}
