//! Node — construction, identity, the prekey ring, and sealed-open.
//!
//! Split out of the 3977-line `lib.rs` (task #23): `impl Node` in a descendant
//! module of the crate root, so it keeps full access to `Node`'s private fields
//! with no visibility change. A pure move — wire format and public API identical.

use crate::*;

impl Node {
    pub fn new(petname: &str, topics: &[&str]) -> Self {
        let mut seed = [0u8; 32];
        OsRng.fill_bytes(&mut seed);
        Self::from_seed(petname, topics, &seed)
    }

    /// The 32-byte signing seed that is this node's whole identity. Persist it
    /// (e.g. a browser's local storage) and [`Node::from_seed`] restores the same
    /// address and keys on the next start — so a node keeps its identity across
    /// restarts instead of becoming a stranger each time.
    pub fn seed(&self) -> [u8; 32] {
        self.sk.to_bytes()
    }

    /// Build a node with a fixed 32-byte signing seed.
    ///
    /// The seed restores the **identity** — address, signing key, and the ability
    /// to mint new prekeys. It does **not** restore the prekey ring: those secrets
    /// are random, which is what makes their deletion mean anything (§7, S-022).
    /// Persist [`Node::prekey_ring`] alongside [`Node::seed`] and restore both, or
    /// the node comes back able to sign but unable to open mail sealed to any
    /// prekey it had rotated to.
    ///
    /// A node that restores from the seed alone gets a single bootstrap prekey
    /// derived from that seed, which is exactly the pre-ring behaviour: it
    /// interoperates, and it has no forward secrecy.
    pub fn from_seed(petname: &str, topics: &[&str], seed: &[u8; 32]) -> Self {
        let sk = SigningKey::from_bytes(seed);
        let addr = addr_of(&sk.verifying_key().to_bytes());

        // Prekey seed = SHA-256(seed ‖ domain) — independent of the signing key
        // but reproducible from the same stored seed. (Built as one buffer to
        // avoid the blake2/sha2 `update` trait ambiguity in this crate.)
        let mut buf = seed.to_vec();
        buf.extend_from_slice(b"spore/prekey/v1");
        let pb: [u8; 32] = Sha256::digest(&buf).into();
        // `born: 0` marks this as the bootstrap entry: derived from the seed, so
        // its true age is unknowable and it cannot expire on a clock it never had.
        // The first `rotate_prekey` stamps it, and it ages out normally from there.
        let boot = Prekey::from_secret(pb, 0);
        let prekey_pub = boot.public;

        let mut addrs = HashSet::new();
        addrs.insert(addr);
        Node {
            sk,
            addr,
            ring: vec![boot],
            prekey_pub,
            petname: petname.to_string(),
            topics: topics.iter().map(|t| topic_of(t)).collect(),
            addrs,
            seen: HashMap::new(),
            store: store::Store::new(),
            paths: Paths::default(),
            peer_prekeys: HashMap::new(),
            peer_busy: HashMap::new(),
            peer_names: HashMap::new(),
            sessions: HashMap::new(),
            prekey_lifetime_secs: PREKEY_LIFETIME_SECS,
            max_store_bytes: 10 * 1024 * 1024,
            seq: 0,
            frags: HashMap::new(),
            mtu: DEFAULT_MTU,
            manifests: HashMap::new(),
            pending: HashMap::new(),
            acked: HashSet::new(),
            rpc_pending: HashSet::new(),
            rpc_responses: HashMap::new(),
            rpc_inbox: Vec::new(),
            feed_inbox: Vec::new(),
            quotas: congestion::Quotas::new(DEFAULT_SOURCE_QUOTA),
            pinned: HashSet::new(),
            gossip: HashMap::new(),
            gossip_rate: DEFAULT_GOSSIP_BUDGET,
            last_sweep: 0,
        }
    }

    /// Set how many bytes per second of stored envelopes each interface may pull
    /// out of this node with WANT. Defaults to [`DEFAULT_GOSSIP_BUDGET`]; raise it
    /// on a fast link, lower it on metered airtime.
    pub fn set_gossip_budget(&mut self, bytes_per_sec: u32) {
        self.gossip_rate = bytes_per_sec;
        self.gossip.clear();
    }

    /// Set the per-source flood quota (§10): the sustained bytes/second any single
    /// originating address may have this node store and relay. Stamped mail
    /// bypasses it. Defaults to [`DEFAULT_SOURCE_QUOTA`].
    pub fn set_source_quota(&mut self, bytes_per_sec: u32) {
        self.quotas = congestion::Quotas::new(bytes_per_sec);
    }

    /// Set the store's byte budget. When exceeded, low-priority envelopes are
    /// evicted (lowest stamp → largest → oldest), but chunks of a file still being
    /// assembled are pinned and never dropped. Defaults to 10 MiB.
    pub fn set_store_budget(&mut self, bytes: usize) {
        self.max_store_bytes = bytes.max(1);
        self.enforce_budget();
    }

    /// Keep the store's bytes in `dir`, holding only a budget of them resident.
    ///
    /// Past that budget the coldest wires spill to disk and only their length
    /// stays in memory, so what a node can carry stops being bounded by its RAM.
    /// This is what lets a file actually run to the sizes a manifest tree allows.
    ///
    /// Anything already in the directory from a previous run is **adopted**, and
    /// any signed manifest among it re-learned — so a transfer a restart
    /// interrupted resumes instead of starting over. Adoption is safe because an
    /// id *is* the hash of its bytes: a file whose name does not match its
    /// content is discarded, so a tampered spill directory cannot inject
    /// anything. Returns how many envelopes were adopted.
    ///
    /// Without this a node is memory-only, which is the right answer on the web
    /// and anywhere else with no filesystem.
    pub fn set_spill_dir(&mut self, dir: &std::path::Path, now: u32) -> std::io::Result<usize> {
        let adopted = self.store.set_spill_dir(dir, now)?;
        // We held these before, so we have already relayed them — don't flood
        // them again just because the process restarted.
        Ok(self.absorb_adopted(adopted))
    }

    /// Spill to storage that is not a filesystem — the same contract as
    /// [`Node::set_spill_dir`], for a runtime whose storage nutrient is browser
    /// IndexedDB, MCU flash, or anything else (see `docs/DESIGN.md`).
    ///
    /// Adoption and its verification are identical either way: a backend is
    /// never trusted to have kept the bytes it was handed, because an id *is*
    /// the hash of its content. Returns how many envelopes were adopted.
    pub fn set_spill_backend(&mut self, backend: Box<dyn store::SpillBackend>, now: u32) -> usize {
        let adopted = self.store.set_spill_backend(backend, now);
        self.absorb_adopted(adopted)
    }

    /// Re-learn what a set of adopted wires implies: we held them before, so we
    /// have already relayed them, and any manifest among them should resume the
    /// transfer a restart interrupted.
    fn absorb_adopted(&mut self, adopted: Vec<Vec<u8>>) -> usize {
        let n = adopted.len();
        for wire in adopted {
            let Ok((e, _)) = Envelope::decode(&wire) else { continue };
            self.mark_seen(&e);
            if e.typ == ty::DATA
                && matches!(
                    e.payload.first(),
                    Some(&file::MANIFEST_TAG) | Some(&file::TREE_TAG) | Some(&file::SEALED_TAG)
                )
            {
                self.absorb_manifest(&e);
            }
        }
        n
    }

    /// How many bytes of the store stay in memory before the rest spills to the
    /// directory set by [`Node::set_spill_dir`]. Defaults to 5 MiB. Without a
    /// spill directory this has no effect — there is nowhere for bytes to go.
    pub fn set_mem_budget(&mut self, bytes: usize) {
        self.store.set_mem_budget(bytes);
    }

    /// Bytes the store is holding, in memory and on disk together.
    pub fn store_bytes(&self) -> usize {
        self.store.bytes()
    }

    pub fn peer_prekey(&self, a: &Addr) -> Option<[u8; 32]> {
        self.peer_prekeys.get(a).copied()
    }

    /// Would a DM to `a` right now be sealed, rather than going in the clear?
    ///
    /// [`Node::send_direct`] seals with a live §7 session if there is one, else a
    /// one-shot seal to a known prekey, else sends **plaintext** — a node that has
    /// never heard the peer's ANNOUNCE has no key to seal to. That fallback is
    /// correct, and invisible, which is the problem: a UI that draws a padlock on
    /// every DM would be lying on exactly the sends that need saying. This asks
    /// the same two questions without sending anything, so a caller can tell the
    /// truth before the user commits.
    pub fn can_seal_to(&self, a: &Addr) -> bool {
        self.sessions.get(a).is_some_and(|s| s.can_send()) || self.peer_prekeys.contains_key(a)
    }
    // ---- prekey ring (§7) -------------------------------------------------

    /// Mint a fresh prekey, advertise it, and drop any secret past its lifetime.
    ///
    /// This is what gives the one-shot seal forward secrecy, and the reason it
    /// works is that the new secret is **random** — not derived from the identity
    /// seed. Restoring from a seed can re-create the identity and mint new
    /// prekeys; it cannot resurrect a secret that has been swept. That asymmetry
    /// is the whole feature (S-022), and it has a price: mail sealed to an expired
    /// prekey is unreadable by anyone, including you.
    ///
    /// Persist the ring with [`Node::prekey_ring`] or the property is theatre — a
    /// node that reloads from its seed alone comes back with only the bootstrap
    /// key and has rotated nothing.
    pub fn rotate_prekey(&mut self, now: u32) {
        // The bootstrap entry has no birthday until the first rotation gives it
        // one; from here it ages like every other entry.
        for pk in &mut self.ring {
            if pk.born == 0 {
                pk.born = now;
            }
        }
        self.ring.push(Prekey::fresh(now));
        self.prekey_pub = self.ring[self.ring.len() - 1].public;
        self.sweep_prekeys(now);
    }

    /// Rotate only if the newest prekey is older than [`PREKEY_PERIOD_SECS`].
    ///
    /// Called from the router's periodic sweep, so rotation happens by operating
    /// rather than by an embedder remembering to ask.
    pub fn maybe_rotate_prekey(&mut self, now: u32) {
        let newest = self.ring.last().map(|p| p.born).unwrap_or(0);
        // A bootstrap-only ring (born 0) rotates on the first sweep that sees a
        // clock, which is what upgrades an existing node onto the ring.
        if newest == 0 || now.saturating_sub(newest) >= PREKEY_PERIOD_SECS {
            self.rotate_prekey(now);
        }
    }

    /// Delete prekey secrets past [`PREKEY_LIFETIME_SECS`], and cap the ring.
    ///
    /// The newest entry always survives, so the ring is never empty and a node can
    /// always be sealed to. A bootstrap entry (`born == 0`) is exempt until a
    /// rotation stamps it: its real age is unknown, and guessing would either
    /// discard a live key or claim an expiry it cannot honour.
    pub fn sweep_prekeys(&mut self, now: u32) {
        let lifetime = self.prekey_lifetime_secs;
        let newest = self.ring.len().saturating_sub(1);
        let mut i = 0;
        self.ring.retain(|pk| {
            let keep = i == newest || pk.born == 0 || now.saturating_sub(pk.born) < lifetime;
            i += 1;
            keep
        });
        // Oldest-first order means excess comes off the front.
        while self.ring.len() > MAX_PREKEY_RING {
            self.ring.remove(0);
        }
        if let Some(p) = self.ring.last() {
            self.prekey_pub = p.public;
        }
    }

    /// How many prekey secrets are held right now (for tests and status UIs).
    pub fn prekey_count(&self) -> usize {
        self.ring.len()
    }

    /// The current "offline window" (PR0 Part B): how long a prekey secret —
    /// and, via session bootstrap in `absorb_announce`, a ratchet session's
    /// skipped-key cache — survives before deletion. Defaults to
    /// [`PREKEY_LIFETIME_SECS`] (7 days).
    pub fn offline_window_secs(&self) -> u32 {
        self.prekey_lifetime_secs
    }

    /// Set the offline window. Clamped to `[PREKEY_PERIOD_SECS, 365 days]` —
    /// the floor so the window is never shorter than the daily rotation
    /// cadence (a shorter window would delete a prekey before it ever got a
    /// chance to matter), the ceiling a sanity bound. Takes effect for future
    /// prekey sweeps and for any ratchet session bootstrapped from here on;
    /// it does not retroactively rewrite an already-running session's own
    /// skip TTL, or already-swept prekeys.
    pub fn set_offline_window_secs(&mut self, secs: u32) {
        self.prekey_lifetime_secs = secs.clamp(PREKEY_PERIOD_SECS, 365 * 86_400);
    }

    /// A snapshot for a ring-health status UI: `(count, oldest secret's age in
    /// seconds, seconds until the next scheduled rotation)`.
    ///
    /// The oldest age is `None` for an unstamped bootstrap entry (`born == 0`) —
    /// its true age is unknowable, exactly as [`Node::sweep_prekeys`] treats it.
    /// "Next rotation" mirrors [`Node::maybe_rotate_prekey`]'s own test — an
    /// unrotated ring (newest `born == 0`) is due immediately, reported as `0`
    /// rather than a made-up duration.
    pub fn prekey_health(&self, now: u32) -> (usize, Option<u32>, u32) {
        let count = self.ring.len();
        let oldest_age =
            self.ring.first().and_then(
                |pk| {
                    if pk.born == 0 {
                        None
                    } else {
                        Some(now.saturating_sub(pk.born))
                    }
                },
            );
        let newest_born = self.ring.last().map(|pk| pk.born).unwrap_or(0);
        let next_mint_in =
            if newest_born == 0 { 0 } else { (newest_born + PREKEY_PERIOD_SECS).saturating_sub(now) };
        (count, oldest_age, next_mint_in)
    }

    /// Serialise the ring so a restart keeps it: `[0x01][n:1][(pub:32, sec:32, born:4 BE)]×n`.
    ///
    /// **This is secret material** — every byte of it opens mail. Store it where
    /// you would store the identity seed, and understand that a *backup* of it
    /// defeats the seven-day window exactly as a backup of any forward-secret
    /// keystore would. That is not a flaw in the design; it is the design being
    /// honest about what deletion means.
    pub fn prekey_ring(&self) -> Vec<u8> {
        let n = self.ring.len().min(MAX_PREKEY_RING);
        let mut out = Vec::with_capacity(2 + n * 68);
        out.push(1);
        out.push(n as u8);
        for pk in &self.ring[self.ring.len() - n..] {
            out.extend_from_slice(&pk.public);
            out.extend_from_slice(&pk.secret);
            out.extend_from_slice(&pk.born.to_be_bytes());
        }
        out
    }

    /// Restore a ring from [`Node::prekey_ring`], **replacing** the current one.
    ///
    /// Replacing rather than merging is deliberate: the stored blob is the
    /// authority on what secrets still exist, so a bootstrap key that has already
    /// aged out of it does not come back to life just because `from_seed` can
    /// still derive it. Returns `false` and changes nothing if the blob is
    /// malformed.
    pub fn restore_prekey_ring(&mut self, blob: &[u8]) -> bool {
        if blob.len() < 2 || blob[0] != 1 {
            return false;
        }
        let n = blob[1] as usize;
        if n == 0 || n > MAX_PREKEY_RING || blob.len() != 2 + n * 68 {
            return false;
        }
        let mut ring = Vec::with_capacity(n);
        for i in 0..n {
            let o = 2 + i * 68;
            let mut public = [0u8; 32];
            let mut secret = [0u8; 32];
            public.copy_from_slice(&blob[o..o + 32]);
            secret.copy_from_slice(&blob[o + 32..o + 64]);
            let born = u32::from_be_bytes([blob[o + 64], blob[o + 65], blob[o + 66], blob[o + 67]]);
            // Recompute the public half rather than trusting it: a corrupted or
            // hostile blob must not make us advertise a key we cannot open.
            let derived = Prekey::from_secret(secret, born);
            if derived.public != public {
                return false;
            }
            ring.push(derived);
        }
        ring.sort_by_key(|p| p.born);
        self.prekey_pub = ring[ring.len() - 1].public;
        self.ring = ring;
        true
    }

    /// Open a box sealed to any prekey we still hold, newest first.
    ///
    /// Trying the whole live ring is what lets a rotation happen without dropping
    /// mail: a sender who last heard an older ANNOUNCE sealed to an older prekey,
    /// and that is fine until the secret expires. Once it has been swept
    /// ([`Node::sweep_prekeys`]) this returns `None` forever — that is the point,
    /// not a bug.
    ///
    /// The nonce is derived from the *recipient's* public key, so a secret must be
    /// tried against its own public half; the ring stores both.
    pub fn open(&self, sealed: &[u8]) -> Option<Vec<u8>> {
        use crypto_box::aead::{generic_array::GenericArray, Aead};
        use crypto_box::{PublicKey, SalsaBox};
        if sealed.len() < 32 {
            return None;
        }
        let mut ep = [0u8; 32];
        ep.copy_from_slice(&sealed[..32]);
        let eph_pub = PublicKey::from(ep);
        for pk in self.ring.iter().rev() {
            let nonce = seal_nonce(&ep, &pk.public);
            if let Ok(m) = SalsaBox::new(&eph_pub, &crypto_box::SecretKey::from(pk.secret))
                .decrypt(GenericArray::from_slice(&nonce), &sealed[32..])
            {
                return Some(m);
            }
        }
        None
    }

    /// Open a direct message (PR0b): via the established §7 ratchet session
    /// when the envelope was flagged `RATCHET`, otherwise via the prekey ring
    /// exactly as [`Node::open`] always has. `sender` is the envelope's
    /// authenticated source address — which session (if any) to use.
    ///
    /// A `RATCHET`-flagged message with no matching session simply doesn't
    /// open (`None`) — this is the case where the sender's session state
    /// outlived ours (e.g. we restarted and haven't re-bootstrapped from
    /// their next ANNOUNCE yet). It self-heals from there; this call does not
    /// retry or fall back on their behalf.
    pub fn open_dm(&mut self, sender: Addr, sealed: &[u8], ratcheted: bool, now: u32) -> Option<Vec<u8>> {
        if ratcheted {
            self.sessions.get_mut(&sender)?.decrypt(sealed, now)
        } else {
            self.open(sealed)
        }
    }

    // ---- origination -----------------------------------------------------
}
