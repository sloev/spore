//! Node — ANNOUNCE/HELLO, the on_rx entry point, bounds enforcement, ingest, forwarding.
//!
//! Split out of the 3977-line `lib.rs` (task #23): `impl Node` in a descendant
//! module of the crate root, so it keeps full access to `Node`'s private fields
//! with no visibility change. A pure move — wire format and public API identical.

use crate::*;

impl Node {
    /// Build+sign this node's ANNOUNCE (prekey + busy + topics), ready to flood (§4).
    ///
    /// Mesh-wide: `hops = 16`, so every node that hears it relays it. §5.4b caps
    /// this at roughly one per hour per node — see [`ANNOUNCE_FLOOD_MIN_SECS`] and
    /// [`Node::build_hello`], which is the cheap link-local form meant for the
    /// frequent beacon.
    pub fn build_announce(&mut self, now: u32) -> Vec<Forward> {
        self.build_announce_at_hops(now, 16)
    }

    /// The **link HELLO** of §4: the same ANNOUNCE payload at `hops = 0`.
    ///
    /// §5 stops forwarding at `hops == 0`, so this reaches direct neighbours on
    /// every interface and goes no further. That is what makes it affordable to
    /// send often — it teaches neighbours our prekey, topics and `busy` byte, and
    /// carries the §5.4c backpressure signal, without costing the whole mesh a
    /// flood. §4 specified this form from the start; nothing built it until
    /// S-023, so the daemon was beaconing the *flooded* variant every few seconds.
    pub fn build_hello(&mut self, now: u32) -> Vec<Forward> {
        self.build_announce_at_hops(now, 0)
    }

    fn build_announce_at_hops(&mut self, now: u32, hops: u8) -> Vec<Forward> {
        // Settle any due rotation *before* advertising, not after (PR0b). Without
        // this, a fresh node could announce its bootstrap prekey, then rotate as
        // a side effect of the next `on_rx` (any node's first-ever `on_rx`
        // unconditionally rotates its born=0 bootstrap entry — see
        // `maybe_rotate_prekey`), and bootstrap its own §7 ratchet session
        // against that *new* prekey while every peer still believes the old one
        // is current — a permanent, silent session mismatch, since a session is
        // never re-seeded once bootstrapped. Calling this here and in
        // `absorb_announce` before either reads `self.prekey_pub`/`ring.last()`
        // means both "what I just told the world" and "what I use for myself"
        // are always the same settled value, regardless of send/receive order.
        self.maybe_rotate_prekey(now);
        let mut p = Vec::new();
        p.extend_from_slice(&self.prekey_pub);
        p.push(self.busy()); // §5.4c backpressure
        p.push(self.topics.len() as u8);
        for t in &self.topics {
            p.extend_from_slice(t);
        }
        p.push(0); // np: we advertise no distant paths in this reference build
        p.extend_from_slice(self.petname.as_bytes());
        let mut e = Envelope::new(ty::ANNOUNCE, ZERO_DEST, now + 3600, p);
        e.flags |= fl::FLOOD;
        e.hops = hops;
        e.sign(&self.sk);
        self.mark_seen(&e);
        // A HELLO is link-local and immediately superseded by the next one, so it
        // is not worth a store slot or an INV entry; a flooded ANNOUNCE is.
        if hops > 0 {
            self.store_put(&e, now);
        }
        self.forward_intents(&e, NO_IFACE, now)
    }

    fn absorb_announce(&mut self, e: &Envelope, now: u32) {
        if !e.verify() {
            return;
        }
        let Src::Full(pk) = &e.src else { return };
        if e.payload.len() < 33 {
            return;
        }
        let src_addr = addr_of(pk);
        let mut prekey = [0u8; 32];
        prekey.copy_from_slice(&e.payload[..32]);
        self.peer_prekeys.insert(src_addr, prekey);
        self.peer_busy.insert(src_addr, e.payload[32]); // §5.4c busy byte

        // §7 Double Ratchet (PR0b): the moment both sides know each other's
        // current prekey, both can derive the same session root independently
        // via a static-static X25519 DH — no message exchange needed. The
        // lower address is always Alice for a pair, decided identically on
        // both ends regardless of who sends first, so two peers who each
        // message the other before hearing back still converge on one
        // session rather than two dangling halves. Never re-seeded from a
        // later ANNOUNCE (prekeys rotate daily) — once bootstrapped, the
        // ratchet's own evolution is what's trusted, exactly like a
        // Signal-style session decouples from identity keys after X3DH.
        //
        // Settled defensively here too (see `build_announce_at_hops`'s comment)
        // even though `ingest`'s `enforce_bounds` already ran this call above —
        // this function must never read `ring.last()`/`prekey_pub` for the
        // bootstrap without knowing rotation has already settled for `now`.
        self.maybe_rotate_prekey(now);
        if !self.sessions.contains_key(&src_addr) {
            if let Some(my_sec) = self.ring.last().map(|pk| pk.secret) {
                // PR0 Part B: the session's skip TTL is this Node's own
                // configured offline window, not a fixed const — this is
                // what keeps the seal layer and the ratchet's skip window
                // promising the same thing without being two settings that
                // could drift apart.
                let session = if self.addr < src_addr {
                    ratchet::Ratchet::init_alice(my_sec, prekey, self.prekey_lifetime_secs)
                } else {
                    ratchet::Ratchet::init_bob(my_sec, self.prekey_pub, prekey, self.prekey_lifetime_secs)
                };
                self.sessions.insert(src_addr, session);
            }
        }

        // The rest is `ntopics · topics[8×n] · np · petname`. The petname is the
        // name the peer *claims*; it is a display hint only — never identity.
        // Anyone may announce any name, so a UI must treat it as a suggestion
        // and let the user assign the local petname that it actually trusts.
        let ntopics = e.payload[33] as usize;
        let after_topics = 34 + ntopics * 8;
        if after_topics < e.payload.len() {
            let name_start = after_topics + 1; // skip the `np` byte
            if name_start <= e.payload.len() {
                let claimed = String::from_utf8_lossy(&e.payload[name_start..]);
                let claimed: String = claimed.chars().filter(|c| !c.is_control()).take(32).collect();
                if !claimed.is_empty() {
                    self.peer_names.insert(src_addr, claimed);
                }
            }
        }
    }

    // ---- receive (the entire router, §5) --------------------------------

    pub(crate) fn mark_seen(&mut self, e: &Envelope) {
        let retain = e.expiry.max(0u32.wrapping_add(SEEN_MIN_SECS)); // >= expiry
        self.seen.insert(e.id(), retain);
    }

    pub fn on_rx(&mut self, raw: &[u8], iface: Iface, nbr: Option<Addr>, now: u32) -> Rx {
        let Ok((e, _)) = Envelope::decode(raw) else {
            return Rx::default();
        };

        // INV/WANT are per-link, hops=0, consumed on receipt — never stored,
        // never deduped, never relayed (§6).
        match e.typ {
            ty::INV => return self.on_inv(&e, iface, nbr),
            ty::WANT => return self.on_want(&e, iface, nbr, now),
            _ => {}
        }
        self.ingest(&e, iface, nbr, now, true)
    }

    /// Keep every table a peer can grow inside its bound.
    ///
    /// Two mechanisms, because the tables differ in kind. Expiry is *time*-based
    /// and does not need to be immediate, so it runs at most every
    /// [`SWEEP_INTERVAL_SECS`]. Hard caps are checked on every ingest, since a
    /// flood can cross a cap in far less than a minute — but they are only ever
    /// *paid for* when a table is actually over, so the common case is a handful of
    /// length comparisons.
    ///
    /// Eviction always degrades a capability rather than breaking one: a forgotten
    /// peer re-announces, a dropped partial object is re-fetched, an evicted dedup
    /// entry costs one duplicate relay. Nothing here can make the node *wrong*,
    /// only forgetful.
    /// Bound `frags` by both count and bytes.
    ///
    /// Separate from `enforce_bounds` because it has to run *after* a fragment
    /// is inserted, not only at the start of the next ingest — otherwise the
    /// budget is exceeded by up to one chunk for as long as no further traffic
    /// arrives, which on a link an attacker controls is indefinitely.
    ///
    /// Bounded twice on purpose. A set holds up to `count` rows of `chunk`
    /// bytes and both come off the wire, so `MAX_PARTIAL_OBJECTS` alone bounds
    /// cardinality while permitting gigabytes on a desktop and several times the
    /// whole heap on an MCU (audit F-3, #189). Oldest goes first either way: it
    /// is the set least likely to still have chunks coming.
    pub(crate) fn enforce_partial_budget(&mut self) {
        let held: usize = self.frags.values().map(|f| f.held_bytes()).sum();
        if self.frags.len() <= self.limits.partial_objects && held <= self.limits.partial_bytes {
            return;
        }
        let mut by_age: Vec<(Id, u32, usize)> =
            self.frags.iter().map(|(k, f)| (*k, f.started, f.held_bytes())).collect();
        by_age.sort_unstable_by_key(|(_, started, _)| *started);

        let (mut n, mut bytes) = (self.frags.len(), held);
        for (id, _, sz) in by_age {
            if n <= self.limits.partial_objects && bytes <= self.limits.partial_bytes {
                break;
            }
            self.frags.remove(&id);
            n -= 1;
            bytes = bytes.saturating_sub(sz);
        }
    }

    pub(crate) fn enforce_bounds(&mut self, now: u32) {
        if now.saturating_sub(self.last_sweep) >= SWEEP_INTERVAL_SECS {
            self.last_sweep = now;
            // Dedup entries carry their own retain-until (§5).
            self.seen.retain(|_, until| *until > now);
            // A set that has heard nothing for the timeout is abandoned or fake.
            self.frags.retain(|_, f| now.saturating_sub(f.started) < PARTIAL_TIMEOUT_SECS);
            // §7 prekey ring. Driven from the sweep rather than left to the
            // embedder, because a forward-secrecy property that each platform has
            // to remember to switch on is one that most platforms will not have.
            self.maybe_rotate_prekey(now);
            // §4's path purge. On the sweep rather than on every learn, because
            // it is a scan of the whole table and the table only goes stale with
            // the clock.
            self.paths.purge(now);
        }

        // Dedup: prefer evicting whatever is nearest to expiring, so the ids most
        // likely to still be in flight survive. Sorting is O(n log n) but only
        // happens on the ingest that crosses the cap.
        if self.seen.len() > self.limits.seen {
            let excess = self.seen.len() - self.limits.seen;
            let mut by_expiry: Vec<(Id, u32)> = self.seen.iter().map(|(k, v)| (*k, *v)).collect();
            by_expiry.sort_unstable_by_key(|(_, until)| *until);
            for (id, _) in by_expiry.into_iter().take(excess) {
                self.seen.remove(&id);
            }
        }

        self.enforce_partial_budget();

        let lim = self.limits;
        self.paths.trim(lim.peers);
        trim_map(&mut self.peer_prekeys, lim.peers);
        trim_map(&mut self.peer_busy, lim.peers);
        trim_map(&mut self.peer_names, lim.peers);
        trim_map(&mut self.sessions, lim.peers);
        trim_map(&mut self.manifests, lim.manifests);
        trim_set(&mut self.acked, lim.acked);

        // Inboxes are queues the application drains. If it has not, drop from the
        // front: the oldest request or event is the one most likely to be stale.
        if self.rpc_inbox.len() > lim.inbox {
            let excess = self.rpc_inbox.len() - lim.inbox;
            self.rpc_inbox.drain(..excess);
        }
        if self.feed_inbox.len() > lim.inbox {
            let excess = self.feed_inbox.len() - lim.inbox;
            self.feed_inbox.drain(..excess);
        }
    }

    /// The router core (§5). `allow_forward` is true for envelopes off the wire
    /// and false for an original recombined from fragments — its chunks already
    /// propagate on their own, so the giant reassembled copy must not re-flood.
    fn ingest(&mut self, e: &Envelope, iface: Iface, nbr: Option<Addr>, now: u32, allow_forward: bool) -> Rx {
        let id = e.id();
        if self.seen.contains_key(&id) || e.expiry < now {
            return Rx::default(); // duplicate or expired -> drop
        }
        // Retain the id for at least the §11 floor, and never past the §2 store
        // horizon: holding an *id* longer than we hold its *bytes* is backwards,
        // and `MAX_SEEN` evicts nearest-expiry first — so an unclamped
        // far-future expiry would make junk the last thing evicted and let a
        // flood of it pin the dedup table. Both bounds are the same 30 days.
        let retain = e.expiry.clamp(now + SEEN_MIN_SECS, now + MAX_EXPIRY_HORIZON_SECS);
        self.seen.insert(id, retain);
        self.enforce_bounds(now);

        // Path learning: the first copy of a signed envelope raced every route
        // and won, so its src is reachable via the interface that delivered it.
        //
        // The signature is verified rather than believed from the flag, for the
        // same reason `Neighbors::snoop` does it: the flag is attacker-chosen, so
        // a forgery carrying a victim's public key would bind that victim's
        // address to whatever interface the forgery arrived on. The cheap checks
        // (dedup, expiry) have already run above, so a verify only happens for a
        // frame that is new and still live — and it happens once, here, for both
        // the path table and the quota attribution below.
        let verified_src = match &e.src {
            Src::Full(pk) if e.flags & fl::SIGNED != 0 && e.verify() => Some(addr_of(pk)),
            _ => None,
        };
        if let Some(a) = verified_src {
            self.paths.learn(a, iface, nbr, now);
        }

        // Per-source flood quota (§10): charge this envelope against its origin's
        // byte budget. Over budget, we still deliver it locally if it's for us,
        // but we do not amplify it — no reassembly hoarding, no store, no relay.
        // Attribution has to be *earned*, not read off the frame. An address in
        // `src` is a claim: `Src::Short` is 8 bytes with no key attached, and a
        // `Src::Full` whose signature does not check out is no better. Charging
        // either to the address it names lets anyone drain a chosen victim's
        // budget until that victim's own mail stops being stored or relayed —
        // a denial of service against a third party, bought with junk. So only a
        // verified signature spends a named budget; everything else shares one
        // bucket, which is still bounded but cannot be aimed.
        // An unsigned envelope is the *most* unattributable of the three, so it
        // shares that same bucket rather than skipping the quota entirely, which
        // is what it used to do (audit F-2, #189). The old reasoning was that
        // "dedup/expiry already bound them", and it does not hold: dedup is keyed
        // on content id and an attacker varies content for free — a different
        // byte is a different id, every time — while expiry bounds lifetime, not
        // rate or volume. Nothing SPORE sends is unsigned (every path in
        // `node::send` signs), so this paces foreign unsigned traffic without
        // touching anything this node originates.
        let src_addr = match &e.src {
            Src::Full(_) | Src::Short(_) => verified_src.unwrap_or(congestion::UNATTRIBUTED),
            Src::None => congestion::UNATTRIBUTED,
        };
        let within_quota = self.quotas.admit(src_addr, e.wire().len() as u32, e.stamp(), now);

        let mut rx = Rx::default();

        if e.typ == ty::ANNOUNCE {
            self.absorb_announce(e, now);
        }

        let deliverable =
            self.addrs.contains(&e.dest) || self.topics.contains(&e.dest) || e.dest == ZERO_DEST;

        // Deliver to the app — but a raw FRAGMENT is transport plumbing, never
        // app data; delivery of a fragmented object happens on reassembly below.
        if deliverable && e.flags & fl::FRAGMENT == 0 {
            rx.delivered.push(e.clone());
        }

        // Auto-learn manifests addressed to us (endpoint demux on the app tag).
        // Either root tag counts; interior nodes never reach here, since they
        // ride a per-file topic nobody subscribes to, and are unsigned besides.
        if deliverable
            && e.typ == ty::DATA
            && matches!(
                e.payload.first(),
                Some(&file::MANIFEST_TAG) | Some(&file::TREE_TAG) | Some(&file::SEALED_TAG)
            )
        {
            self.absorb_manifest(e);
        }

        // L4/L5 endpoint demux: queue requests/feed events, match responses.
        if deliverable && e.typ == ty::DATA && e.flags & fl::FRAGMENT == 0 {
            match e.payload.first().copied() {
                Some(rpc::REQUEST_TAG) => {
                    if let Src::Full(pk) = &e.src {
                        if let Some((id, req)) = rpc::decode_request(&e.payload) {
                            self.rpc_inbox.push((addr_of(pk), id, req));
                        }
                    }
                }
                Some(rpc::RESPONSE_TAG) => {
                    // Only an authenticated (signed) reply can be trusted to a
                    // sender; an unsigned one names nobody, so it can't be checked
                    // against the service that was asked and is dropped.
                    if let Src::Full(pk) = &e.src {
                        if let Some((id, resp)) = rpc::decode_response(&e.payload) {
                            if self.rpc_pending.remove(&id) {
                                self.rpc_responses.insert(id, (addr_of(pk), resp));
                            }
                        }
                    }
                }
                Some(feed::FEED_TAG) => {
                    let from = match &e.src {
                        Src::Full(pk) => Some(addr_of(pk)),
                        _ => None,
                    };
                    self.feed_inbox.push(feed::Event { topic: e.dest, from, data: e.payload[1..].to_vec() });
                }
                _ => {}
            }
        }

        // Receipts (§8), only for mail addressed specifically to one of our
        // addresses (never for topic/public floods).
        if e.typ == ty::DATA && self.addrs.contains(&e.dest) {
            // A receipt for something we sent -> record the delivery, but only
            // from the address we actually sent it to. `verified_src` is `Some`
            // only for a signature that checks out, so an unsigned receipt and
            // one signed by a stranger are both rejected here: the referenced
            // id is not a secret (it rides in the clear inside every INV, §6),
            // so possession of it proves nothing about delivery.
            if e.flags & fl::ACKREQ == 0 && e.payload.first() == Some(&RECEIPT_TAG) && e.payload.len() >= 17 {
                let mut oid = [0u8; 16];
                oid.copy_from_slice(&e.payload[1..17]);
                if verified_src.is_some_and(|s| self.pending.get(&oid).is_some_and(|p| p.dest == s)) {
                    self.acked.insert(oid);
                    self.pending.remove(&oid);
                }
            }
            // A message that asked for a receipt -> flood one back to its src.
            if e.flags & fl::ACKREQ != 0 && e.flags & fl::FRAGMENT == 0 {
                if let Src::Full(pk) = &e.src {
                    let mut p = Vec::with_capacity(17);
                    p.push(RECEIPT_TAG);
                    p.extend_from_slice(&e.id());
                    let mut ack = Envelope::new(ty::DATA, addr_of(pk), e.expiry, p);
                    ack.flags |= fl::FLOOD; // receipts flood and teach reverse paths
                    ack.sign(&self.sk);
                    self.mark_seen(&ack);
                    self.store_put(&ack, now);
                    rx.forwards.append(&mut self.forward_intents(&ack, iface, now));
                }
            }
        }

        // Reassemble only objects bound for us; a pure relay just forwards the
        // fragments (each is an ordinary envelope) without hoarding chunks. A
        // source over its quota can't make us hoard its chunks either.
        if within_quota && deliverable && e.flags & fl::FRAGMENT != 0 && e.payload.len() >= 18 {
            let mut oid = [0u8; 16];
            oid.copy_from_slice(&e.payload[..16]);
            let idx = e.payload[16];
            let count = e.payload[17];
            let chunk = e.payload[18..].to_vec();
            if let Some(orig) = self
                .frags
                .entry(oid)
                .or_insert_with(|| Fountain::started_at(now))
                .add(&oid, idx, count, chunk)
            {
                if let Ok((oe, _)) = Envelope::decode(&orig) {
                    // Deliver the recombined original; do not re-forward it.
                    let mut inner = self.ingest(&oe, iface, nbr, now, false);
                    rx.delivered.append(&mut inner.delivered);
                    rx.forwards.append(&mut inner.forwards);
                }
            }
            // After the insert, not only on the next ingest — see the method.
            self.enforce_partial_budget();
        }

        // Store + relay only within the source's quota — this is the mesh-load
        // that §10 caps. Local delivery above already happened regardless.
        if within_quota {
            // Store for later opportunistic sync.
            self.store_put(e, now);

            // Relay.
            if allow_forward && e.hops > 0 {
                let mut f = e.clone();
                f.hops -= 1;
                rx.forwards.append(&mut self.forward_intents(&f, iface, now));
            }
        }
        rx
    }

    pub(crate) fn forward_intents(&self, e: &Envelope, except: Iface, now: u32) -> Vec<Forward> {
        let bytes = e.wire();
        // FLOOD flag or public dest -> epidemic flood. Otherwise unicast: use a
        // fresh path if we have one, else stay silent (discovery is the
        // originator's job, §5.6).
        if e.flags & fl::FLOOD != 0 || e.dest == ZERO_DEST {
            vec![Forward::Flood { except, bytes }]
        } else if let Some(p) = self.paths.fresh(&e.dest, now) {
            vec![Forward::Directed { iface: p.iface, nbr: p.nbr, bytes }]
        } else {
            Vec::new()
        }
    }

    // ---- sync (§6) -------------------------------------------------------
}
