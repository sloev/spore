use super::*;
use blake2::digest::consts::{U32, U64};
use blake2::Blake2b;
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::ChaCha20Poly1305;
use x25519_dalek::{PublicKey, StaticSecret};
use zeroize::Zeroize;

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

/// A message key held aside during `decrypt`, keyed by the `(ratchet public,
/// message number)` it would be filed under. Held rather than inserted until the
/// message authenticates — see `decrypt`.
type Banked = (([u8; 32], u16), [u8; 32]);

/// A fresh X25519 keypair as `(secret, public)` raw bytes.
pub fn keypair() -> ([u8; 32], [u8; 32]) {
    // Bytes, not a generator. This was the one place the crate handed an RNG
    // object to a crypto crate, and it is what tied our `rand` version to
    // x25519-dalek's `rand_core`. `StaticSecret::from` takes the same 32 bytes
    // and asks nothing about where they came from.
    let mut sb = [0u8; 32];
    crate::fill_random(&mut sb);
    let s = StaticSecret::from(sb);
    let p = PublicKey::from(&s);
    (s.to_bytes(), p.to_bytes())
}

fn dh(sec: &[u8; 32], pubk: &[u8; 32]) -> [u8; 32] {
    StaticSecret::from(*sec).diffie_hellman(&PublicKey::from(*pubk)).to_bytes()
}

// Root KDF: (new_root, chain_key) = BLAKE2b(root ‖ dh_out).
fn kdf_rk(rk: &[u8; 32], dh_out: &[u8; 32]) -> ([u8; 32], [u8; 32]) {
    let mut h = Blake2b::<U64>::new();
    h.update(rk);
    h.update(dh_out);
    h.update(b"spore-ratchet-rk");
    let out: [u8; 64] = h.finalize().into();
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
    let mut h = Blake2b::<U32>::new();
    h.update(ck);
    h.update([tag]);
    h.finalize().into()
}
fn nonce_bytes(n: u16) -> [u8; 12] {
    let mut nb = [0u8; 12];
    nb[10..].copy_from_slice(&n.to_be_bytes());
    nb
}

/// A cached message key for an out-of-order position, with the time it was
/// banked so [`Ratchet::purge_skipped`] can expire it. Zeroized on drop, so a key
/// dropped by created_at, by the count bound, or by the whole ratchet going away does
/// not linger in freed memory.
struct SkippedKey {
    key: [u8; 32],
    inserted_at: u32,
}

impl Drop for SkippedKey {
    fn drop(&mut self) {
        self.key.zeroize();
    }
}

/// One party's ratchet state. Build with `init_alice` (initiator) or
/// `init_bob` (responder), then `encrypt` / `decrypt`.
///
/// Secret fields are zeroized on drop; skipped keys zeroize via [`SkippedKey`].
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
    skipped: HashMap<([u8; 32], u16), SkippedKey>,
    /// How long a skipped message key is retained before [`Ratchet::purge_skipped`]
    /// drops and zeroizes it (PR0 Part B: was a fixed `SKIP_TTL_SECS` const equal
    /// to `crate::PREKEY_LIFETIME_SECS`; now the caller-supplied value — the
    /// session bootstrap in `absorb_announce` passes `Node`'s own
    /// `prekey_lifetime_secs`, so the seal layer and this session layer keep
    /// promising the same window rather than drifting apart).
    skip_ttl_secs: u32,
}

impl Drop for Ratchet {
    fn drop(&mut self) {
        // Public halves (dhs_pub, dhr) are not secret; the root, chain keys and DH
        // secret are. The skipped map's entries zeroize through SkippedKey's own
        // Drop when the HashMap is dropped with this struct.
        self.rk.zeroize();
        self.cks.zeroize();
        self.ckr.zeroize();
        self.dhs_sec.zeroize();
    }
}

impl Ratchet {
    /// Initiator. `my_sec` is our identity/prekey X25519 secret; `peer_pub`
    /// is the responder's prekey public. Root bootstraps from their static
    /// DH, then an immediate ratchet gives us a sending chain. `skip_ttl_secs`
    /// is how long a skipped message key survives (PR0 Part B) — pass the
    /// same value both sides use for their prekey ring, or the two "offline
    /// window" promises drift apart.
    pub fn init_alice(my_sec: [u8; 32], peer_pub: [u8; 32], skip_ttl_secs: u32) -> Self {
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
            skip_ttl_secs,
        }
    }

    /// Responder. Our ratchet key starts as our prekey `(my_sec, my_pub)`;
    /// `peer_pub` is the initiator's prekey public. No sending chain yet — it
    /// appears after we receive the initiator's first message. `skip_ttl_secs`:
    /// see [`Ratchet::init_alice`].
    pub fn init_bob(my_sec: [u8; 32], my_pub: [u8; 32], peer_pub: [u8; 32], skip_ttl_secs: u32) -> Self {
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
            skip_ttl_secs,
        }
    }

    /// Whether [`Ratchet::encrypt`] can be called right now — false for a
    /// freshly-`init_bob`'d responder that hasn't yet received anything from
    /// the initiator, since it has no sending chain until it does. A caller
    /// that originates messages (rather than only replying) should check this
    /// before `encrypt`, and fall back to another path when it's false.
    pub fn can_send(&self) -> bool {
        self.cks.is_some()
    }

    /// Encrypt `plaintext` into a ratchet message. Panics only if called on a
    /// responder before it has received the first message — check
    /// [`Ratchet::can_send`] first if that's possible for the caller.
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

        let ct = ChaCha20Poly1305::new(&mk.into())
            .encrypt(&(nonce_bytes(n)).into(), Payload { msg: plaintext, aad: &header })
            .expect("aead encrypt");
        let mut out = header;
        out.extend_from_slice(&ct);
        out
    }

    /// Decrypt a ratchet message, or `None` if it isn't decodable, is a
    /// replay, or falls outside the skip window.
    ///
    /// `now` is the caller's clock (unix seconds); it expires skipped keys older
    /// than this session's configured skip TTL before doing anything else, so a
    /// key banked for an out-of-order message that never came does not outlive
    /// the forward-secrecy window. Pass the same `now` the rest of the node runs
    /// on — do not invent a second clock.
    /// Open a message, **committing nothing until it authenticates**.
    ///
    /// The ordering here is the whole security property. This function used to
    /// turn the ratchet, bank skipped keys and advance `nr` on the way to
    /// deriving a message key, and authenticate last — so anyone able to put
    /// bytes on the medium could destroy a session with a single frame:
    ///
    ///   * flip one bit of the 32-byte ratchet public and the receiver performs
    ///     a DH step to an attacker-chosen key. The real chain is gone, and *no
    ///     genuine message ever opens again* — measured, not theorised.
    ///   * corrupt the ciphertext of a message whose key was banked, and the
    ///     banked key is consumed on the way to failing, so the real copy can
    ///     never be opened.
    ///
    /// Neither forgery opened anything, which is what made it easy to miss: the
    /// attacker cannot read, only permanently deafen. A ratchet message rides an
    /// ordinary envelope, so the attacker is anyone on the link.
    ///
    /// Now every candidate change is computed into locals and applied only after
    /// `open` succeeds. A forged frame costs one X25519 keypair generation and
    /// changes nothing, which is the right trade against losing the session.
    pub fn decrypt(&mut self, msg: &[u8], now: u32) -> Option<Vec<u8>> {
        // Time-based and not attacker-driven, so it is safe before the check.
        self.purge_skipped(now);
        if msg.len() < HEADER {
            return None;
        }
        let mut dh_pub = [0u8; 32];
        dh_pub.copy_from_slice(&msg[..32]);
        let n = u16::from_be_bytes([msg[32], msg[33]]);
        let pn = u16::from_be_bytes([msg[34], msg[35]]);
        let header = &msg[..HEADER];
        let ct = &msg[HEADER..];

        // A key we cached for an out-of-order message. **Peeked, not taken**: a
        // corrupt copy must not consume the key the real one needs.
        if let Some(sk) = self.skipped.get(&(dh_pub, n)) {
            let plain = Self::open(&sk.key, n, header, ct)?;
            self.skipped.remove(&(dh_pub, n));
            return Some(plain);
        }

        // --- everything below is a proposal until `open` says otherwise -------
        let mut rk = self.rk;
        let mut ckr = self.ckr;
        let mut nr = self.nr;
        let mut dhr = self.dhr;
        let mut dhs_sec = self.dhs_sec;
        let mut dhs_pub = self.dhs_pub;
        let mut cks = self.cks;
        let mut pn_out = self.pn;
        let mut ns = self.ns;
        // Keys this message *would* bank, held aside rather than inserted.
        let mut banked: Vec<Banked> = Vec::new();

        /// Advance a receiving chain to `until`, collecting the keys it passes.
        fn skip_into(
            until: u16,
            nr: &mut u16,
            ckr: &mut Option<[u8; 32]>,
            dhr: Option<[u8; 32]>,
            banked: &mut Vec<Banked>,
        ) -> Option<()> {
            if until > nr.saturating_add(MAX_SKIP) {
                return None; // absurd gap: refuse
            }
            if let (Some(mut c), Some(d)) = (*ckr, dhr) {
                while *nr < until {
                    let (nck, mk) = kdf_ck(&c);
                    banked.push(((d, *nr), mk));
                    c = nck;
                    *nr += 1;
                }
                *ckr = Some(c);
            }
            Some(())
        }

        if dhr.as_ref() != Some(&dh_pub) {
            skip_into(pn, &mut nr, &mut ckr, dhr, &mut banked)?;
            // The DH step, into locals. Generating a keypair for a frame that
            // may be forged is the cost of not trusting it yet.
            pn_out = ns;
            ns = 0;
            nr = 0;
            dhr = Some(dh_pub);
            let (r1, c1) = kdf_rk(&rk, &dh(&dhs_sec, &dh_pub));
            rk = r1;
            ckr = Some(c1);
            let (s, pbk) = keypair();
            dhs_sec = s;
            dhs_pub = pbk;
            let (r2, c2) = kdf_rk(&rk, &dh(&dhs_sec, &dh_pub));
            rk = r2;
            cks = Some(c2);
        }
        if n < nr {
            return None; // already consumed / replay
        }
        skip_into(n, &mut nr, &mut ckr, dhr, &mut banked)?;
        let chain = ckr?;
        let (nck, mk) = kdf_ck(&chain);

        // **The authentication.** Nothing above has touched `self`.
        let plain = Self::open(&mk, n, header, ct)?;

        // --- authentic: commit -----------------------------------------------
        self.rk = rk;
        self.ckr = Some(nck);
        self.nr = nr + 1;
        self.dhr = dhr;
        self.dhs_sec = dhs_sec;
        self.dhs_pub = dhs_pub;
        self.cks = cks;
        self.pn = pn_out;
        self.ns = ns;
        for (k, key) in banked {
            self.skipped.insert(k, SkippedKey { key, inserted_at: now });
        }
        self.bound_skipped();
        Some(plain)
    }

    /// Drop and zeroize skipped keys older than this session's skip TTL.
    ///
    /// `saturating_sub` so a clock that went backwards between bank and read keeps
    /// the key rather than treating it as infinitely old and dropping it early.
    /// Removed entries zeroize through [`SkippedKey`]'s `Drop`.
    fn purge_skipped(&mut self, now: u32) {
        self.skipped.retain(|_, sk| now.saturating_sub(sk.inserted_at) < self.skip_ttl_secs);
    }

    fn open(mk: &[u8; 32], n: u16, header: &[u8], ct: &[u8]) -> Option<Vec<u8>> {
        ChaCha20Poly1305::new(&(*mk).into())
            .decrypt(&(nonce_bytes(n)).into(), Payload { msg: ct, aad: header })
            .ok()
    }

    // Cache message keys for positions self.nr .. until in the current
    // receiving chain (so their out-of-order messages still open later). `now`
    // stamps each banked key for age-based created_at in `purge_skipped`.

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
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: u32 = 1_700_000_000;
    // PR0 Part B: skip_ttl_secs is now a caller-supplied value rather than a
    // fixed const; these tests just want the historical 7-day default.
    const TEST_SKIP_TTL: u32 = 7 * 24 * 3600;

    #[test]
    fn skipped_keys_expire_after_ttl() {
        // Alice sends 0,1,2,3; Bob receives 3 first, which banks skipped keys for
        // 0..3. If more than TEST_SKIP_TTL passes before the stragglers arrive,
        // their keys are gone and they no longer open — the forward-secrecy window
        // has closed, which is the whole point.
        let (a_sec, a_pub) = keypair();
        let (b_sec, b_pub) = keypair();
        let mut alice = Ratchet::init_alice(a_sec, b_pub, TEST_SKIP_TTL);
        let mut bob = Ratchet::init_bob(b_sec, b_pub, a_pub, TEST_SKIP_TTL);
        let m0 = alice.encrypt(b"zero");
        let m1 = alice.encrypt(b"one");
        let _m2 = alice.encrypt(b"two");
        let m3 = alice.encrypt(b"three");

        assert_eq!(bob.decrypt(&m3, T0).as_deref(), Some(&b"three"[..]), "newest arrives");
        assert!(!bob.skipped.is_empty(), "0..3 should be banked as skipped");

        // The stragglers arrive one second past the window.
        let expired = T0 + TEST_SKIP_TTL + 1;
        assert!(bob.decrypt(&m0, expired).is_none(), "expired skipped key must not open");
        assert!(bob.decrypt(&m1, expired).is_none(), "expired skipped key must not open");
        assert!(bob.skipped.is_empty(), "purge must have emptied the cache");
    }

    #[test]
    fn skipped_keys_live_inside_ttl() {
        // Same gap, but the stragglers arrive one minute inside the window: they
        // still open. Forward secrecy is a deadline, not a hair trigger.
        let (a_sec, a_pub) = keypair();
        let (b_sec, b_pub) = keypair();
        let mut alice = Ratchet::init_alice(a_sec, b_pub, TEST_SKIP_TTL);
        let mut bob = Ratchet::init_bob(b_sec, b_pub, a_pub, TEST_SKIP_TTL);
        let m0 = alice.encrypt(b"zero");
        let _m1 = alice.encrypt(b"one");
        let m2 = alice.encrypt(b"two");

        assert_eq!(bob.decrypt(&m2, T0).as_deref(), Some(&b"two"[..]));
        let still_inside = T0 + TEST_SKIP_TTL - 60;
        assert_eq!(
            bob.decrypt(&m0, still_inside).as_deref(),
            Some(&b"zero"[..]),
            "a skipped key inside the window still opens"
        );
    }

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
        // Driven through `decrypt` rather than by calling the internals: the
        // bound has to hold against what a peer can actually send, and since the
        // ratchet stopped committing state before authenticating, the internals
        // are no longer reachable any other way.
        let (a_sec, a_pub) = keypair();
        let (b_sec, b_pub) = keypair();
        let mut alice = Ratchet::init_alice(a_sec, b_pub, TEST_SKIP_TTL);
        let mut bob = Ratchet::init_bob(b_sec, b_pub, a_pub, TEST_SKIP_TTL);

        // Each round: Alice sends a burst, Bob opens only the last of it — which
        // banks the whole gap — and then both ratchet, opening a fresh window.
        for step in 0..20u16 {
            let mut burst = Vec::new();
            for i in 0..200u16 {
                burst.push(alice.encrypt(format!("{step}:{i}").as_bytes()));
            }
            let last = burst.pop().expect("non-empty");
            assert!(bob.decrypt(&last, T0).is_some(), "step {step}: the newest opens");
            assert!(
                bob.skipped.len() <= MAX_SKIPPED_KEYS,
                "step {step}: held {} skipped keys against a cap of {MAX_SKIPPED_KEYS}",
                bob.skipped.len()
            );
            // Turn both ratchets, so the next round files under a new `dh_pub`
            // and `nr` restarts — the condition that used to grow this forever.
            let from_bob = bob.encrypt(b"turn");
            assert!(alice.decrypt(&from_bob, T0).is_some());
        }
        assert!(bob.skipped.len() <= MAX_SKIPPED_KEYS);
    }

    #[test]
    fn an_absurd_gap_is_still_refused_outright() {
        // The per-gap bound is the cheaper guard and must keep working: a single
        // request for an enormous jump is refused rather than pre-computed.
        // Stated as what a peer can send, and checked by what it costs: a header
        // claiming an enormous jump must be refused *without* pre-computing the
        // keys, so the observable is that nothing was banked.
        let (a_sec, a_pub) = keypair();
        let (b_sec, b_pub) = keypair();
        let mut alice = Ratchet::init_alice(a_sec, b_pub, TEST_SKIP_TTL);
        let mut bob = Ratchet::init_bob(b_sec, b_pub, a_pub, TEST_SKIP_TTL);

        let mut msg = alice.encrypt(b"hello");
        let absurd = MAX_SKIP + 1;
        msg[32..34].copy_from_slice(&absurd.to_be_bytes()); // n
        assert!(bob.decrypt(&msg, T0).is_none(), "a gap past MAX_SKIP must be refused");
        assert!(bob.skipped.is_empty(), "and refused before computing a single key for it");
    }
}

#[cfg(test)]
mod properties {
    //! Property tests for the ratchet's **state machine** (#278).
    //!
    //! The parsers here are well fuzzed; the state machines were not tested at
    //! all for the transitions that would actually hurt. A parser bug is a crash;
    //! a ratchet bug is a message decrypting twice, or a key surviving a rotation
    //! it should not have, and neither shows up in a fuzz target that only feeds
    //! bytes to `decode`.
    //!
    //! So these drive two real `Ratchet`s across a channel that loses, reorders
    //! and duplicates, with both sides sending, and assert the properties rather
    //! than a transcript. Seeded, so a failure is reproducible from its number.
    use super::*;

    const T0: u32 = 1_700_000_000;
    const TTL: u32 = 7 * 24 * 3600;

    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            self.0 >> 33
        }
        fn below(&mut self, n: usize) -> usize {
            if n == 0 {
                0
            } else {
                self.next() as usize % n
            }
        }
        fn chance(&mut self, pct: u64) -> bool {
            pct > 0 && self.next() % 100 < pct
        }
    }

    fn pair() -> (Ratchet, Ratchet) {
        let (a_sec, a_pub) = keypair();
        let (b_sec, b_pub) = keypair();
        (Ratchet::init_alice(a_sec, b_pub, TTL), Ratchet::init_bob(b_sec, b_pub, a_pub, TTL))
    }

    /// One message in flight: who it is for, the bytes, and what it should say.
    struct InFlight {
        to_bob: bool,
        wire: Vec<u8>,
        plain: Vec<u8>,
    }

    /// Drive a session over a hostile channel and check the invariants hold.
    ///
    /// `loss`, `dup` and `reorder` are percentages. Returns how many messages the
    /// far side successfully opened, so a caller can assert the channel was
    /// actually exercised rather than silently doing nothing.
    fn run(seed: u64, steps: usize, loss: u64, dup: u64, reorder: u64) -> usize {
        let mut rng = Rng(seed);
        let (mut alice, mut bob) = pair();
        let mut wire: Vec<InFlight> = Vec::new();
        let mut opened: Vec<Vec<u8>> = Vec::new(); // ciphertexts already accepted
        let mut delivered = 0usize;

        for step in 0..steps {
            // Send, from whichever side can. Bob has no sending chain until he
            // has heard from Alice, which is the protocol's own rule and worth
            // exercising rather than working around.
            if rng.chance(60) {
                let from_alice = rng.chance(50) || !bob.can_send();
                let plain = format!("step {step}").into_bytes();
                if from_alice {
                    wire.push(InFlight { to_bob: true, wire: alice.encrypt(&plain), plain });
                } else if bob.can_send() {
                    wire.push(InFlight { to_bob: false, wire: bob.encrypt(&plain), plain });
                }
            }

            if wire.is_empty() {
                continue;
            }
            // Deliver something — not necessarily the oldest, which is the
            // reordering.
            let idx = if rng.chance(reorder) { rng.below(wire.len()) } else { 0 };
            let msg = wire.remove(idx);

            if rng.chance(loss) {
                continue; // the channel ate it
            }
            let copies = if rng.chance(dup) { 2 } else { 1 };
            for _ in 0..copies {
                let side: &mut Ratchet = if msg.to_bob { &mut bob } else { &mut alice };
                let got = side.decrypt(&msg.wire, T0);
                let already = opened.iter().any(|c| c == &msg.wire);
                match got {
                    Some(p) => {
                        assert!(!already, "a ciphertext opened twice — replay (seed {seed}, step {step})");
                        assert_eq!(p, msg.plain, "opened, but to the wrong plaintext (seed {seed})");
                        opened.push(msg.wire.clone());
                        delivered += 1;
                    }
                    None => {
                        // Refusing is always allowed: a key may have been ratcheted
                        // past, or skipped-key space exhausted. What is not allowed
                        // is opening it a second time, which the branch above checks.
                    }
                }
            }
        }
        delivered
    }

    #[test]
    fn a_clean_channel_delivers_everything_it_carries() {
        // The control. If this fails the harness is wrong, not the ratchet.
        for seed in 0..8u64 {
            let n = run(seed, 60, 0, 0, 0);
            assert!(n > 10, "seed {seed} delivered only {n} on a clean channel");
        }
    }

    #[test]
    fn no_ciphertext_ever_opens_twice() {
        // Replay is the failure that matters most here, and a duplicating channel
        // is how it would be found. Both the in-order path and the skipped-key
        // path are exercised, since they bank keys differently.
        for seed in 0..24u64 {
            let n = run(seed, 120, 0, 60, 40);
            assert!(n > 0, "seed {seed} exercised nothing");
        }
    }

    #[test]
    fn a_session_survives_loss_reordering_and_duplication_together() {
        // All three at once, which is the realistic case on a radio and the one
        // no single-property test covers.
        for seed in 0..24u64 {
            let n = run(seed, 200, 30, 30, 50);
            assert!(n > 5, "seed {seed} delivered only {n} — the session stopped working");
        }
    }

    #[test]
    fn a_corrupted_message_never_opens() {
        // Authenticity, stated as a property rather than one example: flipping any
        // single bit must make it fail, not merely usually fail.
        let (mut alice, mut bob) = pair();
        let msg = alice.encrypt(b"the dam holds");
        for bit in 0..msg.len() * 8 {
            let mut bad = msg.clone();
            bad[bit / 8] ^= 1 << (bit % 8);
            assert!(bob.decrypt(&bad, T0).is_none(), "a message with bit {bit} flipped opened");
        }
        // ...and the untouched original still does, so the flips did not simply
        // break the session for everything that followed.
        assert_eq!(bob.decrypt(&msg, T0).as_deref(), Some(&b"the dam holds"[..]));
    }

    #[test]
    fn both_sides_sending_at_once_still_converge() {
        // A simultaneous transition: each side sends before hearing the other, so
        // both are advancing their own chain with no knowledge of the peer's.
        let (mut alice, mut bob) = pair();
        let a1 = alice.encrypt(b"from alice");
        assert!(
            !bob.can_send(),
            "bob has no sending chain until he has heard alice — the protocol's own rule"
        );
        assert_eq!(bob.decrypt(&a1, T0).as_deref(), Some(&b"from alice"[..]));

        // Now both send, neither having seen the other's latest.
        let a2 = alice.encrypt(b"alice again");
        let b1 = bob.encrypt(b"from bob");
        assert_eq!(alice.decrypt(&b1, T0).as_deref(), Some(&b"from bob"[..]));
        assert_eq!(bob.decrypt(&a2, T0).as_deref(), Some(&b"alice again"[..]));

        // And the session keeps working afterwards, in both directions.
        let a3 = alice.encrypt(b"after the crossover");
        let b2 = bob.encrypt(b"likewise");
        assert_eq!(bob.decrypt(&a3, T0).as_deref(), Some(&b"after the crossover"[..]));
        assert_eq!(alice.decrypt(&b2, T0).as_deref(), Some(&b"likewise"[..]));
    }
}

#[cfg(test)]
mod unauthenticated_input {
    //! A frame anyone on the link can write must not be able to change a
    //! session's state (#278).
    //!
    //! Both of these failed before `decrypt` was reordered to authenticate
    //! first, and neither is subtle once seen: the attacker cannot read
    //! anything, so the forgery "fails" — and permanently deafens the receiver
    //! on its way out.
    use super::*;
    const T0: u32 = 1_700_000_000;
    const TTL: u32 = 7 * 24 * 3600;

    fn pair() -> (Ratchet, Ratchet) {
        let (a_sec, a_pub) = keypair();
        let (b_sec, b_pub) = keypair();
        (Ratchet::init_alice(a_sec, b_pub, TTL), Ratchet::init_bob(b_sec, b_pub, a_pub, TTL))
    }

    #[test]
    fn a_forged_ratchet_header_cannot_destroy_the_session() {
        // One flipped bit in the 32-byte ratchet public used to make the receiver
        // perform a DH step to an attacker-chosen key, discarding the real
        // receiving chain. Nothing genuine opened again — not the message already
        // in flight, and not anything sent afterwards.
        let (mut alice, mut bob) = pair();
        let good = alice.encrypt(b"genuine");

        let mut forged = good.clone();
        forged[0] ^= 1;
        assert!(bob.decrypt(&forged, T0).is_none(), "the forgery must not open");

        assert_eq!(
            bob.decrypt(&good, T0).as_deref(),
            Some(&b"genuine"[..]),
            "and must not have taken the genuine message down with it"
        );
        let later = alice.encrypt(b"later");
        assert_eq!(bob.decrypt(&later, T0).as_deref(), Some(&b"later"[..]), "the session continues");
    }

    #[test]
    fn a_corrupt_copy_cannot_consume_a_banked_key() {
        // The skipped-key path had the same shape: the key was taken from the
        // cache on the way to failing, so a corrupted copy of an out-of-order
        // message destroyed the real one's only chance of opening.
        let (mut alice, mut bob) = pair();
        let m0 = alice.encrypt(b"zero");
        let m1 = alice.encrypt(b"one");
        assert!(bob.decrypt(&m1, T0).is_some(), "m1 arrives first, banking a key for m0");

        let mut corrupt = m0.clone();
        let last = corrupt.len() - 1;
        corrupt[last] ^= 1;
        assert!(bob.decrypt(&corrupt, T0).is_none(), "the corrupt copy must not open");

        assert_eq!(
            bob.decrypt(&m0, T0).as_deref(),
            Some(&b"zero"[..]),
            "and the banked key must still be there for the real one"
        );
    }

    #[test]
    fn a_flood_of_forgeries_changes_nothing() {
        // The property rather than two examples: no sequence of unauthenticated
        // frames may leave the session in a state a genuine message cannot use.
        let (mut alice, mut bob) = pair();
        let good = alice.encrypt(b"still here");
        let mut rng = 0x5EEDu64;
        for _ in 0..400 {
            rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1);
            let mut junk = good.clone();
            let byte = (rng >> 33) as usize % junk.len();
            junk[byte] ^= 1 << ((rng >> 20) % 8);
            assert!(bob.decrypt(&junk, T0).is_none(), "no forgery opens");
        }
        assert_eq!(bob.decrypt(&good, T0).as_deref(), Some(&b"still here"[..]));
    }
}
