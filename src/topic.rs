//! Encrypted-topic key management — **KEYROT** (§7).
//!
//! An encrypted topic is a channel everyone shares a 32-byte key for
//! (`topic_seal`/`topic_open`). Two things eventually force a key change, and
//! this module handles both:
//!
//! 1. **Forward secrecy over time.** [`rotate`] advances the key by hashing it,
//!    `next = SHA-256(key ‖ domain)`. Keep only the current key and a leak of it
//!    can't decrypt *past* traffic (hashing isn't reversible). [`seal`]/[`open`]
//!    tag each message with its epoch so receivers know which key applies and
//!    reject stale epochs.
//! 2. **Membership change.** To remove someone, pick a fresh random key and hand
//!    it to each *remaining* member with [`rekey_seal`] (a sealed box to their
//!    prekey); [`rekey_open`] recovers it. The departed member, lacking a prekey
//!    that opens any box, never learns the new key.
//!
//! The rotation itself travels as an ordinary signed topic message, so authority
//! to rotate is just "signed by a current member" — no new envelope type.

use super::*;

const KEYROT_DOMAIN: &[u8] = b"spore-keyrot-v1";

/// Advance a topic key one epoch. Deterministic and one-way: given `next` you
/// cannot recover the key it came from, so deleting the old key protects the
/// traffic sent under it.
pub fn rotate(key: &[u8; 32]) -> [u8; 32] {
    let mut buf = Vec::with_capacity(32 + KEYROT_DOMAIN.len());
    buf.extend_from_slice(key);
    buf.extend_from_slice(KEYROT_DOMAIN);
    let d = Sha256::digest(&buf);
    let mut out = [0u8; 32];
    out.copy_from_slice(&d);
    out
}

/// The key `n` epochs after `root` (n applications of [`rotate`]).
pub fn epoch_key(root: &[u8; 32], epoch: u32) -> [u8; 32] {
    let mut k = *root;
    for _ in 0..epoch {
        k = rotate(&k);
    }
    k
}

/// Seal `msg` under `key` and tag it with `epoch`: `[epoch:4 BE] ‖ topic_seal`.
pub fn seal(msg: &[u8], key: &[u8; 32], epoch: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + msg.len() + 40);
    out.extend_from_slice(&epoch.to_be_bytes());
    out.extend_from_slice(&topic_seal(msg, key));
    out
}

/// Read the epoch tag without decrypting (so a receiver can pick the right key).
pub fn peek_epoch(ct: &[u8]) -> Option<u32> {
    if ct.len() < 4 {
        return None;
    }
    Some(u32::from_be_bytes([ct[0], ct[1], ct[2], ct[3]]))
}

/// Open an epoch-tagged message with the key for its epoch; returns
/// `(epoch, plaintext)`. `key` must be the key for the message's own epoch.
pub fn open(ct: &[u8], key: &[u8; 32]) -> Option<(u32, Vec<u8>)> {
    let epoch = peek_epoch(ct)?;
    let msg = topic_open(&ct[4..], key)?;
    Some((epoch, msg))
}

/// Seal a fresh topic `new_key` to one member's `prekey` (membership rekey). Only
/// the holder of the matching prekey secret can recover it.
pub fn rekey_seal(new_key: &[u8; 32], member_prekey: &[u8; 32]) -> Vec<u8> {
    seal_box(new_key, member_prekey)
}

/// Recover a rekeyed topic key from a [`rekey_seal`] box with your prekey secret.
pub fn rekey_open(sealed: &[u8], my_prekey_sec: &[u8; 32]) -> Option<[u8; 32]> {
    let k = open_sealed(sealed, my_prekey_sec)?;
    if k.len() != 32 {
        return None;
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&k);
    Some(out)
}

// `seal` is the topic module's own name, so reach the crate-level sealed-box
// primitive explicitly.
fn seal_box(msg: &[u8], recip_prekey: &[u8; 32]) -> Vec<u8> {
    crate::seal(msg, recip_prekey)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotate_is_one_way_and_deterministic() {
        let root = [7u8; 32];
        let k1 = rotate(&root);
        assert_eq!(k1, rotate(&root), "deterministic");
        assert_ne!(k1, root, "advances");
        assert_eq!(epoch_key(&root, 3), rotate(&rotate(&rotate(&root))), "n-fold");
    }

    #[test]
    fn epoch_tagged_seal_open_roundtrips() {
        let root = [9u8; 32];
        let k5 = epoch_key(&root, 5);
        let ct = seal(b"safehouse moved", &k5, 5);
        assert_eq!(peek_epoch(&ct), Some(5));
        assert_eq!(open(&ct, &k5).unwrap(), (5, b"safehouse moved".to_vec()));
        // The wrong epoch's key can't open it.
        assert!(open(&ct, &epoch_key(&root, 6)).is_none());
    }

    #[test]
    fn membership_rekey_reaches_only_the_holder() {
        let (member_sec, member_pub) = prekey_keypair();
        let (outsider_sec, _outsider_pub) = prekey_keypair();
        let new_key = [0x42u8; 32];

        let boxed = rekey_seal(&new_key, &member_pub);
        assert_eq!(rekey_open(&boxed, &member_sec), Some(new_key), "member recovers it");
        assert!(rekey_open(&boxed, &outsider_sec).is_none(), "outsider cannot");
    }
}
