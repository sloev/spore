use super::*;
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use x25519_dalek::{PublicKey, StaticSecret};

const HEADER: usize = 36; // dh_pub(32) + n(2) + pn(2)
const MAX_SKIP: u16 = 512; // cap out-of-order gap we'll pre-compute keys for

/// Most skipped message keys held at once, across all receiving chains.
///
/// [`MAX_SKIP`] bounds a single gap; this bounds the *total*, which is a different
/// quantity. Skipped keys are stored under `(dh_pub, n)`, and a DH ratchet step
/// installs a new `dh_pub` and resets `nr` to zero — so every step opens a fresh
/// 512-key window, and nothing consumed the old ones except a message that
/// actually arrived to claim them. A peer that keeps ratcheting while leaving its
/// gaps unclaimed therefore grew this map without limit.
///
/// It takes an established session to do, so this is a peer you have already
/// agreed to talk to rather than anyone on the medium. Bounded anyway: a session
/// partner should not be able to decide how much memory you spend.
///
/// Losing a skipped key costs exactly what packet loss costs — an out-of-order
/// message that no longer opens — and the protocol already tolerates that, since
/// the ratchet's whole purpose is to survive gaps.
const MAX_SKIPPED_KEYS: usize = 4 * MAX_SKIP as usize;

/// A fresh X25519 keypair as `(secret, public)` raw bytes.
pub fn keypair() -> ([u8; 32], [u8; 32]) {
    let s = StaticSecret::random_from_rng(OsRng);
    let p = PublicKey::from(&s);
    (s.to_bytes(), p.to_bytes())
}

fn dh(sec: &[u8; 32], pubk: &[u8; 32]) -> [u8; 32] {
    StaticSecret::from(*sec).diffie_hellman(&PublicKey::from(*pubk)).to_bytes()
}

// Root KDF: (new_root, chain_key) = BLAKE2b(root ‖ dh_out).
fn kdf_rk(rk: &[u8; 32], dh_out: &[u8; 32]) -> ([u8; 32], [u8; 32]) {
    let mut h = Blake2bVar::new(64).unwrap();
    h.update(rk);
    h.update(dh_out);
    h.update(b"spore-ratchet-rk");
    let mut out = [0u8; 64];
    h.finalize_variable(&mut out).unwrap();
    let mut nrk = [0u8; 32];
    let mut ck = [0u8; 32];
    nrk.copy_from_slice(&out[..32]);
    ck.copy_from_slice(&out[32..]);
    (nrk, ck)
}
// Chain KDF: message key and next chain key from distinct constants.
fn kdf_ck(ck: &[u8; 32]) -> ([u8; 32], [u8; 32]) {
    let mk = blake2_32(ck, 0x01);
    let nck = blake2_32(ck, 0x02);
    (nck, mk)
}
fn blake2_32(ck: &[u8; 32], tag: u8) -> [u8; 32] {
    let mut h = Blake2bVar::new(32).unwrap();
    h.update(ck);
    h.update(&[tag]);
    let mut o = [0u8; 32];
    h.finalize_variable(&mut o).unwrap();
    o
}
fn nonce_bytes(n: u16) -> [u8; 12] {
    let mut nb = [0u8; 12];
    nb[10..].copy_from_slice(&n.to_be_bytes());
    nb
}

/// One party's ratchet state. Build with `init_alice` (initiator) or
/// `init_bob` (responder), then `encrypt` / `decrypt`.
pub struct Ratchet {
    dhs_sec: [u8; 32],
    dhs_pub: [u8; 32],
    dhr: Option<[u8; 32]>,
    rk: [u8; 32],
    cks: Option<[u8; 32]>,
    ckr: Option<[u8; 32]>,
    ns: u16,
    nr: u16,
    pn: u16,
    skipped: HashMap<([u8; 32], u16), [u8; 32]>,
}

impl Ratchet {
    /// Initiator. `my_sec` is our identity/prekey X25519 secret; `peer_pub`
    /// is the responder's prekey public. Root bootstraps from their static
    /// DH, then an immediate ratchet gives us a sending chain.
    pub fn init_alice(my_sec: [u8; 32], peer_pub: [u8; 32]) -> Self {
        let sk = dh(&my_sec, &peer_pub);
        let (dhs_sec, dhs_pub) = keypair();
        let (rk, cks) = kdf_rk(&sk, &dh(&dhs_sec, &peer_pub));
        Ratchet {
            dhs_sec,
            dhs_pub,
            dhr: Some(peer_pub),
            rk,
            cks: Some(cks),
            ckr: None,
            ns: 0,
            nr: 0,
            pn: 0,
            skipped: HashMap::new(),
        }
    }

    /// Responder. Our ratchet key starts as our prekey `(my_sec, my_pub)`;
    /// `peer_pub` is the initiator's prekey public. No sending chain yet — it
    /// appears after we receive the initiator's first message.
    pub fn init_bob(my_sec: [u8; 32], my_pub: [u8; 32], peer_pub: [u8; 32]) -> Self {
        let sk = dh(&my_sec, &peer_pub);
        Ratchet {
            dhs_sec: my_sec,
            dhs_pub: my_pub,
            dhr: None,
            rk: sk,
            cks: None,
            ckr: None,
            ns: 0,
            nr: 0,
            pn: 0,
            skipped: HashMap::new(),
        }
    }

    /// Encrypt `plaintext` into a ratchet message. Panics only if called on a
    /// responder before it has received the first message.
    pub fn encrypt(&mut self, plaintext: &[u8]) -> Vec<u8> {
        let cks = self.cks.expect("no sending chain yet (responder must receive first)");
        let (nck, mk) = kdf_ck(&cks);
        self.cks = Some(nck);
        let n = self.ns;
        self.ns += 1;

        let mut header = Vec::with_capacity(HEADER);
        header.extend_from_slice(&self.dhs_pub);
        header.extend_from_slice(&n.to_be_bytes());
        header.extend_from_slice(&self.pn.to_be_bytes());

        let ct = ChaCha20Poly1305::new(Key::from_slice(&mk))
            .encrypt(Nonce::from_slice(&nonce_bytes(n)), Payload { msg: plaintext, aad: &header })
            .expect("aead encrypt");
        let mut out = header;
        out.extend_from_slice(&ct);
        out
    }

    /// Decrypt a ratchet message, or `None` if it isn't decodable, is a
    /// replay, or falls outside the skip window.
    pub fn decrypt(&mut self, msg: &[u8]) -> Option<Vec<u8>> {
        if msg.len() < HEADER {
            return None;
        }
        let mut dh_pub = [0u8; 32];
        dh_pub.copy_from_slice(&msg[..32]);
        let n = u16::from_be_bytes([msg[32], msg[33]]);
        let pn = u16::from_be_bytes([msg[34], msg[35]]);
        let header = &msg[..HEADER];
        let ct = &msg[HEADER..];

        // A key we cached for an out-of-order message?
        if let Some(mk) = self.skipped.remove(&(dh_pub, n)) {
            return Self::open(&mk, n, header, ct);
        }
        // New ratchet public -> turn the ratchet (after banking the tail of
        // the current receiving chain).
        if self.dhr.as_ref() != Some(&dh_pub) {
            self.skip(pn)?;
            self.dh_ratchet(&dh_pub);
        }
        if n < self.nr {
            return None; // already consumed / replay
        }
        self.skip(n)?;
        let ckr = self.ckr?;
        let (nck, mk) = kdf_ck(&ckr);
        self.ckr = Some(nck);
        self.nr += 1;
        Self::open(&mk, n, header, ct)
    }

    fn open(mk: &[u8; 32], n: u16, header: &[u8], ct: &[u8]) -> Option<Vec<u8>> {
        ChaCha20Poly1305::new(Key::from_slice(mk))
            .decrypt(Nonce::from_slice(&nonce_bytes(n)), Payload { msg: ct, aad: header })
            .ok()
    }

    // Cache message keys for positions self.nr .. until in the current
    // receiving chain (so their out-of-order messages still open later).
    fn skip(&mut self, until: u16) -> Option<()> {
        if until > self.nr.saturating_add(MAX_SKIP) {
            return None; // absurd gap: refuse
        }
        if let (Some(mut ckr), Some(dhr)) = (self.ckr, self.dhr) {
            while self.nr < until {
                let (nck, mk) = kdf_ck(&ckr);
                self.skipped.insert((dhr, self.nr), mk);
                ckr = nck;
                self.nr += 1;
            }
            self.bound_skipped();
            self.ckr = Some(ckr);
        }
        Some(())
    }

    /// Keep the skipped-key cache inside [`MAX_SKIPPED_KEYS`].
    ///
    /// Drops arbitrary entries rather than oldest-first: the map is keyed by
    /// `(dh_pub, n)` with no recoverable ordering across chains, and every entry is
    /// equally a key for a message that may never arrive. `std`'s hasher is seeded
    /// per map, so a peer cannot steer which of its own gaps survive.
    fn bound_skipped(&mut self) {
        if self.skipped.len() <= MAX_SKIPPED_KEYS {
            return;
        }
        let excess = self.skipped.len() - MAX_SKIPPED_KEYS;
        let victims: Vec<([u8; 32], u16)> = self.skipped.keys().take(excess).copied().collect();
        for k in victims {
            self.skipped.remove(&k);
        }
    }

    fn dh_ratchet(&mut self, dh_pub: &[u8; 32]) {
        self.pn = self.ns;
        self.ns = 0;
        self.nr = 0;
        self.dhr = Some(*dh_pub);
        let (rk, ckr) = kdf_rk(&self.rk, &dh(&self.dhs_sec, dh_pub));
        self.rk = rk;
        self.ckr = Some(ckr);
        let (dhs_sec, dhs_pub) = keypair();
        self.dhs_sec = dhs_sec;
        self.dhs_pub = dhs_pub;
        let (rk2, cks) = kdf_rk(&self.rk, &dh(&self.dhs_sec, dh_pub));
        self.rk = rk2;
        self.cks = Some(cks);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_skipped_key_cache_cannot_grow_without_bound() {
        // MAX_SKIP bounds one gap; this is about the total. Skipped keys are held
        // under `(dh_pub, n)`, and a DH ratchet step installs a new `dh_pub` and
        // resets `nr`, so every step opens a fresh window. A peer that keeps
        // ratcheting while leaving its gaps unclaimed grew this map forever.
        //
        // It takes an established session to reach, so this is a partner you
        // already agreed to talk to — but a partner should not get to decide how
        // much memory you spend.
        let (a_sec, a_pub) = keypair();
        let (b_sec, b_pub) = keypair();
        let mut bob = Ratchet::init_bob(b_sec, b_pub, a_pub);
        let _ = a_sec;

        // Drive many receiving chains, each leaving a large unclaimed gap.
        for step in 0..40u16 {
            let (_, fresh_pub) = keypair();
            bob.dh_ratchet(&fresh_pub);
            // Ask for keys up to a wide gap without ever claiming them.
            let _ = bob.skip(MAX_SKIP.min(400));
            assert!(
                bob.skipped.len() <= MAX_SKIPPED_KEYS,
                "step {step}: held {} skipped keys against a cap of {MAX_SKIPPED_KEYS}",
                bob.skipped.len()
            );
        }
    }

    #[test]
    fn an_absurd_gap_is_still_refused_outright() {
        // The per-gap bound is the cheaper guard and must keep working: a single
        // request for an enormous jump is refused rather than pre-computed.
        let (_a_sec, a_pub) = keypair();
        let (b_sec, b_pub) = keypair();
        let mut bob = Ratchet::init_bob(b_sec, b_pub, a_pub);
        let (_, fresh) = keypair();
        bob.dh_ratchet(&fresh);
        assert!(bob.skip(MAX_SKIP + 1).is_none(), "a gap past MAX_SKIP must be refused");
    }
}
