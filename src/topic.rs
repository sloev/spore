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
//!
//! # Healing after a compromise
//!
//! [`rotate`] alone gives forward secrecy and nothing else. It is a hash chain:
//! someone who copies the current key computes every key after it, so rotating
//! faster does not help — the attacker rotates too. A group in that state stays
//! compromised until a human notices and intervenes, which is exactly the thing
//! you cannot count on.
//!
//! [`contribute`] / [`absorb`] fix that. A member draws 32 fresh random bytes,
//! seals a copy to every member's prekey, and everyone folds it into the key with
//! [`mix`] — `new = SHA-256(domain ‖ current ‖ contribution)`. An attacker holding
//! the whole chain still cannot compute the result, because the contribution never
//! travels in a form it can open. The group *heals by operating*, with nobody
//! having detected anything.
//!
//! Two properties worth being precise about:
//!
//! - **Mixing, never replacing.** The new key depends on the old one as well as
//!   the contribution, so a contribution can only ever add entropy. An attacker
//!   who injects contributions of its own — it can, if it can sign as a member —
//!   cannot *undo* healing; it adds steps it knows to a chain that one honest
//!   contribution it cannot read makes unknowable again.
//! - **What it still does not heal.** The boxes are sealed to members' *prekeys*.
//!   Someone holding a member's prekey secret opens every contribution addressed
//!   to that member and follows along. Healing against that needs the prekey
//!   itself to move, which is [`crate::seal`]'s daily-rotation story, not this
//!   module's. Stated plainly: this recovers from a stolen *group key*, which is
//!   the copy that gets backed up, synced and left on old devices.

use super::*;

const KEYROT_DOMAIN: &[u8] = b"spore-keyrot-v1";
const MIX_DOMAIN: &[u8] = b"spore-topic-mix-v1";
const KEYID_DOMAIN: &[u8] = b"spore-topic-keyid-v1";

/// One sealed contribution: [`crate::seal`] of 32 bytes = 32-byte ephemeral
/// public key ‖ 32-byte ciphertext ‖ 16-byte tag.
const BOX_LEN: usize = 80;

/// Ceiling on members in one [`contribute`] message.
///
/// A recipient does not know which box is its own — there are no recipient hints,
/// deliberately, so the message does not enumerate the group to anyone who
/// intercepts it — so [`absorb`] trial-decrypts. That is one X25519 per box, and
/// an unbounded count would turn a single forged message into a CPU sink. 256 is
/// far past any group that a shared key is the right answer for.
pub const MAX_MEMBERS: usize = 256;

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

// ---------------------------------------------------------------------------
// Healing rotation — contributory, so the key stops being a function of itself.
// ---------------------------------------------------------------------------

/// Fold a contribution into a key: `SHA-256(domain ‖ current ‖ contribution)`.
///
/// Both halves are required, which is the whole point. Knowing `current` (the
/// attacker's position after copying a key) or `contribution` (a member's position
/// if it somehow saw only the new entropy) is not enough on its own.
pub fn mix(current: &[u8; 32], contribution: &[u8; 32]) -> [u8; 32] {
    let mut buf = Vec::with_capacity(MIX_DOMAIN.len() + 64);
    buf.extend_from_slice(MIX_DOMAIN);
    buf.extend_from_slice(current);
    buf.extend_from_slice(contribution);
    let d = Sha256::digest(&buf);
    let mut out = [0u8; 32];
    out.copy_from_slice(&d);
    out
}

/// A short public name for a key, so a receiver holding several candidate keys can
/// tell which one a message was sealed under.
///
/// This does **not** give the group agreement — nothing here does; see the module
/// docs on the missing roster. It makes disagreement survivable: two members who
/// have absorbed different contributions can still recognise, rather than silently
/// fail to open, a message sealed under a key they do hold. Four bytes of a hash
/// of the key leaks nothing usable about the key itself.
pub fn key_id(key: &[u8; 32]) -> [u8; 4] {
    let mut buf = Vec::with_capacity(KEYID_DOMAIN.len() + 32);
    buf.extend_from_slice(KEYID_DOMAIN);
    buf.extend_from_slice(key);
    let d = Sha256::digest(&buf);
    [d[0], d[1], d[2], d[3]]
}

/// Draw fresh entropy and seal it to every member.
///
/// Returns the key everyone will arrive at once they absorb it, and the message to
/// send. Send it as an ordinary signed topic message: authority to rotate is
/// "signed by a member", the same rule the rest of §7 uses.
///
/// Wire shape: `[0x01 version][count:2 BE][80-byte sealed box] × count`. The boxes
/// carry no recipient hints, so the message does not tell an interceptor who is in
/// the group or how to find its own box — everyone tries every box.
pub fn contribute(current: &[u8; 32], member_prekeys: &[[u8; 32]]) -> ([u8; 32], Vec<u8>) {
    let mut c = [0u8; 32];
    OsRng.fill_bytes(&mut c);

    let n = member_prekeys.len().min(MAX_MEMBERS);
    let mut out = Vec::with_capacity(3 + n * BOX_LEN);
    out.push(1);
    out.extend_from_slice(&(n as u16).to_be_bytes());
    for pk in &member_prekeys[..n] {
        let b = seal_box(&c, pk);
        debug_assert_eq!(b.len(), BOX_LEN, "sealed box is a fixed size");
        out.extend_from_slice(&b);
    }
    (mix(current, &c), out)
}

/// Absorb a [`contribute`] message: recover the contribution with your prekey
/// secret and return the key it produces from `current`.
///
/// `None` if the message is malformed, or if no box in it is addressed to you —
/// which is what a removed member sees, and what anyone outside the group sees.
///
/// Only call this on a message you have already authenticated as coming from a
/// member. Anyone can hand you bytes; this function is careful with them (bounded,
/// no panics, no unbounded work) but it cannot tell you whether the sender was
/// entitled to rotate your group's key.
pub fn absorb(current: &[u8; 32], msg: &[u8], my_prekey_sec: &[u8; 32]) -> Option<[u8; 32]> {
    if msg.len() < 3 || msg[0] != 1 {
        return None;
    }
    let count = u16::from_be_bytes([msg[1], msg[2]]) as usize;
    if count > MAX_MEMBERS || msg.len() != 3 + count * BOX_LEN {
        return None;
    }
    for i in 0..count {
        let off = 3 + i * BOX_LEN;
        if let Some(c) = open_sealed(&msg[off..off + BOX_LEN], my_prekey_sec) {
            if c.len() == 32 {
                let mut arr = [0u8; 32];
                arr.copy_from_slice(&c);
                return Some(mix(current, &arr));
            }
        }
    }
    None
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

    // -- healing -----------------------------------------------------------

    /// The property `rotate` does not have, stated as a test: an attacker who
    /// copies the group key follows the hash chain forever, and stops being able
    /// to follow it the moment one contribution it cannot open lands.
    #[test]
    fn a_contribution_locks_out_an_attacker_holding_the_key() {
        let (alice_sec, alice_pub) = prekey_keypair();
        let (bob_sec, bob_pub) = prekey_keypair();
        let members = [alice_pub, bob_pub];

        // The group key, and the attacker's copy of it.
        let mut group = [0x11u8; 32];
        let mut attacker = group;

        // Plain rotation does not help: the attacker rotates too.
        for _ in 0..10 {
            group = rotate(&group);
            attacker = rotate(&attacker);
        }
        assert_eq!(group, attacker, "a hash chain never heals on its own");
        assert!(
            open(&seal(b"still readable", &group, 0), &attacker).is_some(),
            "attacker reads the group's traffic"
        );

        // Alice contributes. The attacker sees the message and cannot open it,
        // holding no member's prekey secret.
        let (healed, msg) = contribute(&group, &members);
        assert_eq!(absorb(&group, &msg, &alice_sec), Some(healed), "alice converges");
        assert_eq!(absorb(&group, &msg, &bob_sec), Some(healed), "bob converges");

        let (outsider_sec, _) = prekey_keypair();
        assert!(absorb(&group, &msg, &outsider_sec).is_none(), "attacker cannot absorb");

        // And the attacker's best guesses are all wrong.
        assert_ne!(healed, attacker);
        assert_ne!(healed, rotate(&attacker));
        let ct = seal(b"the safehouse moved again", &healed, 0);
        assert!(open(&ct, &attacker).is_none(), "attacker is locked out");
        assert!(open(&ct, &rotate(&attacker)).is_none());
        assert_eq!(open(&ct, &healed).unwrap().1, b"the safehouse moved again");
    }

    /// An attacker that can also *inject* — sign as a member, or replay — can add
    /// steps it knows, but cannot cancel an honest one. Mixing is what buys this:
    /// the new key always depends on the old.
    #[test]
    fn injected_contributions_cannot_undo_healing() {
        let (alice_sec, alice_pub) = prekey_keypair();
        let (mal_sec, mal_pub) = prekey_keypair();
        let start = [0x22u8; 32];

        // Alice heals first: the attacker cannot open her box.
        let (after_alice, alice_msg) = contribute(&start, &[alice_pub]);
        assert!(absorb(&start, &alice_msg, &mal_sec).is_none());

        // Now the attacker contributes something it knows, to a key it does not.
        let (after_mal, mal_msg) = contribute(&after_alice, &[alice_pub, mal_pub]);
        assert_eq!(absorb(&after_alice, &mal_msg, &alice_sec), Some(after_mal));

        // It knows its own contribution, but not what it was mixed into, so the
        // result stays out of reach.
        let ct = seal(b"still private", &after_mal, 0);
        assert!(open(&ct, &start).is_none());
        assert!(open(&ct, &rotate(&start)).is_none());
        assert_eq!(open(&ct, &after_mal).unwrap().1, b"still private");
    }

    #[test]
    fn mix_needs_both_halves_and_key_id_distinguishes() {
        let k = [3u8; 32];
        let c = [4u8; 32];
        let m = mix(&k, &c);
        assert_eq!(m, mix(&k, &c), "deterministic");
        assert_ne!(m, mix(&c, &k), "not symmetric — order is part of the derivation");
        assert_ne!(m, k);
        assert_ne!(m, c);
        assert_ne!(key_id(&k), key_id(&m), "a rotated key announces itself differently");
        assert_eq!(key_id(&k), key_id(&[3u8; 32]), "same key, same id");
    }

    /// `absorb` takes attacker-chosen bytes. It must reject them, not panic and
    /// not do unbounded work — the same bar `robustness.rs` holds every other
    /// parser to.
    #[test]
    fn absorb_rejects_malformed_input_without_panicking() {
        let (sec, pubk) = prekey_keypair();
        let key = [5u8; 32];
        let (_, good) = contribute(&key, &[pubk]);

        assert!(absorb(&key, &[], &sec).is_none(), "empty");
        assert!(absorb(&key, &[1, 0], &sec).is_none(), "truncated header");
        assert!(absorb(&key, &[9, 0, 1], &sec).is_none(), "unknown version");
        // A count that does not match the body, in both directions.
        assert!(absorb(&key, &[1, 0, 1], &sec).is_none(), "claims a box, carries none");
        assert!(absorb(&key, &[1, 0xff, 0xff], &sec).is_none(), "absurd count");
        let mut lying = good.clone();
        lying[1] = 0x00;
        lying[2] = 0x02; // says two boxes, carries one
        assert!(absorb(&key, &lying, &sec).is_none());
        // Every truncation of a well-formed message.
        for n in 0..good.len() {
            assert!(absorb(&key, &good[..n], &sec).is_none(), "truncated at {n}");
        }
        // Corrupt each box byte: it must fail to open, never panic.
        for i in 3..good.len() {
            let mut bad = good.clone();
            bad[i] ^= 0xff;
            let _ = absorb(&key, &bad, &sec);
        }
        assert!(absorb(&key, &good, &sec).is_some(), "the intact one still works");
    }

    #[test]
    fn contribute_caps_the_member_count() {
        let key = [6u8; 32];
        let many: Vec<[u8; 32]> = (0..MAX_MEMBERS + 50).map(|_| prekey_keypair().1).collect();
        let (_, msg) = contribute(&key, &many);
        assert_eq!(msg.len(), 3 + MAX_MEMBERS * BOX_LEN, "capped at MAX_MEMBERS");
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
