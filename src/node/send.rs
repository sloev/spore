//! Node — origination, unicast/broadcast send, receipts, RPC, feed, busy state.
//!
//! Split out of the 3977-line `lib.rs` (task #23): `impl Node` in a descendant
//! module of the crate root, so it keeps full access to `Node`'s private fields
//! with no visibility change. A pure move — wire format and public API identical.

use crate::*;

impl Node {
    /// Put an envelope in the store, holding it no further ahead than §2's
    /// horizon.
    ///
    /// The clamp is on the *store's* copy of the expiry, never on the envelope:
    /// `expiry` is inside the signature, so rewriting it would invalidate the
    /// frame we are about to serve to somebody else. What changes is only how
    /// long *this* node agrees to carry it — which is the node's own business,
    /// and is what §2's "stores clamp horizon to 30 d" has always said.
    ///
    /// Every path into the store goes through here, so this one `min` is the
    /// whole fix.
    pub(crate) fn store_put(&mut self, e: &Envelope, now: u32) {
        let expiry = e.expiry.min(now.saturating_add(MAX_EXPIRY_HORIZON_SECS));
        self.store.put(e.id(), e.wire(), expiry, e.stamp(), self.seq, e.dest);
        self.seq += 1;
        self.enforce_budget();
    }
    pub(crate) fn enforce_budget(&mut self) {
        let mut total = self.store.bytes();
        if total <= self.max_store_bytes {
            return;
        }
        // Pin the chunks (and manifest) of any file we're still assembling, so
        // memory pressure never drops a chunk we're actively collecting and
        // stalls the fetch forever. Completed files are unpinned and evictable.
        let pinned = self.pinned_ids();
        // evict order: lowest stamp -> largest -> oldest (smallest seq)
        while total > self.max_store_bytes {
            let victim = self
                .store
                .entries()
                .filter(|(k, _)| !pinned.contains(*k))
                .min_by(|a, b| {
                    a.1.stamp.cmp(&b.1.stamp).then(b.1.len.cmp(&a.1.len)).then(a.1.seq.cmp(&b.1.seq))
                })
                .map(|(k, _)| *k);
            match victim {
                Some(k) => {
                    total = total.saturating_sub(self.store.meta(&k).map(|s| s.len).unwrap_or(0));
                    self.store.remove(&k);
                }
                None => break, // only in-progress file chunks remain — keep them
            }
        }
    }

    /// Content IDs that must not be evicted: the manifest and already-collected
    /// chunks of every file we hold a manifest for but haven't completed yet.
    fn pinned_ids(&self) -> HashSet<Id> {
        let mut pinned = HashSet::new();
        for (magnet, m) in &self.manifests {
            // Keep a file if it's still being assembled, or if it's explicitly
            // pinned (a seed-vault holding the bootstrap bundle forever).
            if self.has_file(magnet) && !self.pinned.contains(magnet) {
                continue;
            }
            pinned.insert(*magnet);
            // Interior manifests are pinned alongside the chunks: evicting one
            // mid-fetch would hide its whole subtree and stall the transfer with
            // no way to name what went missing.
            self.walk_tree(m, &mut |id, _, held| {
                if held {
                    // A manifest names *content*; eviction works on *envelopes*
                    // (M11-M). Pinning the content id alone silently pinned
                    // nothing, so an in-progress fetch could have its chunks
                    // evicted out from under it. Pin both: the envelope carrying
                    // the content, and the raw id for objects with no file-layer
                    // tag — a sealed header is named by envelope id.
                    if let Some(envelope) = self.store.by_content(id) {
                        pinned.insert(envelope);
                    }
                    pinned.insert(*id);
                }
                true
            });
        }
        pinned
    }

    /// Originate a signed public (flooded) message on topic/broadcast `dest`.
    pub fn originate(&mut self, dest: Addr, payload: Vec<u8>, now: u32) -> Vec<Forward> {
        let mut e = Envelope::new(ty::DATA, dest, now + DEFAULT_MESSAGE_EXPIRY_SECS, payload);
        // Topics and public floods carry FLOOD; the relay uses this flag (not
        // structure) to tell multicast from unicast (§5).
        if dest == ZERO_DEST || self.topics.contains(&dest) {
            e.flags |= fl::FLOOD;
        }
        e.sign(&self.sk);
        // Unicast with no known path: flood to discover it (§5.6).
        if e.flags & fl::FLOOD == 0 && self.paths.fresh(&dest, now).is_none() {
            e.flags |= fl::FLOOD;
            e.sign(&self.sk);
        }
        self.mark_seen(&e);
        self.store_put(&e, now);
        self.forward_intents(&e, NO_IFACE, now)
    }

    /// High-level send: deliver `data` of *any* size to an address or topic.
    ///
    /// **One signed envelope, whatever the size.** Callers never think about
    /// MTUs: a hop that cannot carry the frame splits it for that hop alone and
    /// the far end reassembles before the router sees anything. The sender does
    /// not guess on behalf of a path it cannot see.
    ///
    /// Returns [`TooLarge`] past [`MAX_PAYLOAD_BYTES`] (65 535 — what `plen` can
    /// describe). That ceiling is structural rather than policy, so exceeding it
    /// is a property of the payload the caller handed over: an error to report,
    /// not a bug to abort on. For objects that large, use the file/manifest
    /// layer, which exists for exactly this.
    ///
    /// The error kept its name and changed its meaning. It used to mean "needs
    /// more than one fountain set"; `send` no longer fragments at all.
    pub fn send(&mut self, dest: Addr, data: Vec<u8>, now: u32) -> Result<Vec<Forward>, TooLarge> {
        if data.len() > MAX_PAYLOAD_BYTES {
            return Err(TooLarge { len: data.len(), max: MAX_PAYLOAD_BYTES });
        }
        let mut e = Envelope::new(ty::DATA, dest, now + DEFAULT_MESSAGE_EXPIRY_SECS, data);
        if dest == ZERO_DEST || self.topics.contains(&dest) {
            e.flags |= fl::FLOOD;
        }
        e.sign(&self.sk);
        // Unicast with no known path: flood to discover it (§5.6).
        if e.flags & fl::FLOOD == 0 && self.paths.fresh(&dest, now).is_none() {
            e.flags |= fl::FLOOD;
            e.sign(&self.sk);
        }

        // **One envelope, whatever its size.** This used to fountain-fragment
        // anything over `self.mtu` into unsigned fragments that flooded the mesh
        // — and that never solved the problem it existed for: a fragment carries
        // no nesting, so pieces cut for a 1400-byte link died at the first
        // 237-byte hop and no node on the path could re-cut them.
        //
        // Splitting belongs to the link that cannot carry the frame, not to the
        // sender guessing on behalf of a path it cannot see. A bridge now splits
        // for its own hop and the far end reassembles before the router sees
        // anything, so the sender emits the object and stops thinking about it.
        //
        // The size ceiling is `plen`: 65 535 payload bytes, checked by the
        // envelope encoder rather than by a fragment count. Larger objects are
        // the file layer's job.
        self.mark_seen(&e);
        self.store_put(&e, now);
        Ok(self.forward_intents(&e, NO_IFACE, now))
    }

    /// Originate a unicast message that asks the recipient for a delivery
    /// receipt (§8). Tracks it for backoff resend until a receipt arrives.
    pub fn originate_ackreq(&mut self, dest: Addr, payload: Vec<u8>, now: u32) -> Vec<Forward> {
        let mut e = Envelope::new(ty::DATA, dest, now + DEFAULT_MESSAGE_EXPIRY_SECS, payload);
        e.flags |= fl::ACKREQ;
        if dest == ZERO_DEST || self.topics.contains(&dest) {
            e.flags |= fl::FLOOD;
        }
        e.sign(&self.sk);
        if e.flags & fl::FLOOD == 0 && self.paths.fresh(&dest, now).is_none() {
            e.flags |= fl::FLOOD;
            e.sign(&self.sk);
        }
        let id = e.id();
        self.mark_seen(&e);
        self.store_put(&e, now);
        self.pending.insert(id, Pending { wire: e.wire(), backoff: congestion::Backoff::new(now), dest });
        self.forward_intents(&e, NO_IFACE, now)
    }

    /// Has a receipt for `id` come back?
    pub fn acked(&self, id: &Id) -> bool {
        self.acked.contains(id)
    }

    /// Send a **direct message**: through the established §7 ratchet session
    /// when we have one (PR0b), sealed to the peer's prekey otherwise, and
    /// flagged `ACKREQ` so the recipient returns a delivery receipt (§8).
    /// Returns the envelope id — poll [`Node::acked`] for delivery — and
    /// whether it actually went out encrypted.
    ///
    /// A peer's prekey arrives with their ANNOUNCE, at which point a session
    /// is already bootstrapped (see `absorb_announce`). Whichever address
    /// sorts lower is always that pair's ratchet initiator and can ratchet
    /// immediately; the other side's session has no sending chain until it
    /// actually receives the initiator's first message ([`ratchet::Ratchet`]'s
    /// own documented constraint), so *its* first send here still falls back
    /// to a plain seal — and every send after that first receive is
    /// ratcheted. The plain-seal branch also remains the fallback for a
    /// session-less peer, and the cleartext branch the last resort for a
    /// total stranger; all three mirror this function's own shape before
    /// PR0b, just demoted a rung. This is the one call a messenger UI needs.
    pub fn send_direct(&mut self, dest: Addr, plaintext: &[u8], now: u32) -> (Id, Vec<Forward>, bool) {
        let (payload, encrypted, ratcheted) = if self.sessions.get(&dest).is_some_and(|s| s.can_send()) {
            (self.sessions.get_mut(&dest).unwrap().encrypt(plaintext), true, true)
        } else if let Some(pk) = self.peer_prekey(&dest) {
            (seal(plaintext, &pk), true, false)
        } else {
            (plaintext.to_vec(), false, false)
        };
        let mut e = Envelope::new(ty::DATA, dest, now + DEFAULT_MESSAGE_EXPIRY_SECS, payload);
        e.flags |= fl::ACKREQ;
        if encrypted {
            e.flags |= fl::ENCRYPTED;
        }
        if ratcheted {
            e.flags |= fl::RATCHET;
        }
        e.sign(&self.sk);
        // Unicast with no known path: flood to discover it (§5.6).
        if self.paths.fresh(&dest, now).is_none() {
            e.flags |= fl::FLOOD;
            e.sign(&self.sk);
        }
        let id = e.id();
        self.mark_seen(&e);
        self.store_put(&e, now);
        self.pending.insert(id, Pending { wire: e.wire(), backoff: congestion::Backoff::new(now), dest });
        (id, self.forward_intents(&e, NO_IFACE, now), encrypted)
    }

    /// The display name a peer announced, if any.
    ///
    /// This is what the peer **claims** to be called — anyone may announce any
    /// name, so it is a display hint, never identity. Offer it as the default
    /// when the user assigns their own local petname; that petname is the name
    /// to trust.
    pub fn peer_name(&self, a: &Addr) -> Option<&str> {
        self.peer_names.get(a).map(|s| s.as_str())
    }

    /// Peers we've heard from, freshest first: `(address, seconds since last
    /// heard, whether we hold their prekey)`. A peer appears once any signed
    /// traffic — usually their ANNOUNCE — has reached us; holding their prekey
    /// is what makes an encrypted message to them possible.
    pub fn peers(&self, now: u32) -> Vec<(Addr, u32, bool)> {
        let mut v: Vec<(Addr, u32, bool)> = self
            .paths
            .map
            .iter()
            .filter(|(a, _)| !self.addrs.contains(*a))
            .filter_map(|(a, ps)| {
                let newest = ps.iter().map(|p| p.age).max()?;
                Some((*a, now.saturating_sub(newest), self.peer_prekeys.contains_key(a)))
            })
            .collect();
        v.sort_by_key(|(_, age, _)| *age);
        v
    }

    /// Resend any ACKREQ messages whose backoff has elapsed without a receipt
    /// (§5.6: flooding is route discovery). Drops exhausted or acked ones.
    pub fn resend_unacked(&mut self, now: u32) -> Vec<Forward> {
        let mut out = Vec::new();
        let mut done = Vec::new();
        for (id, p) in self.pending.iter_mut() {
            if self.acked.contains(id) || p.backoff.exhausted() {
                done.push(*id);
                continue;
            }
            if p.backoff.due(now) {
                p.backoff.fired(now);
                out.push(Forward::Flood { except: NO_IFACE, bytes: p.wire.clone() });
            }
        }
        for id in done {
            self.pending.remove(&id);
        }
        out
    }

    /// The node's periodic work — **the scheduling nutrient** a runtime supplies
    /// (`docs/SPEC.md`'s runtime contract makes it normative).
    ///
    /// Call it on a timer, roughly once a second; it is cheap and self-gating, so
    /// calling it more often costs almost nothing and less often only makes the
    /// node lazier. It returns the envelopes that fell due for resend, which the
    /// caller sends the same way it sends anything else.
    ///
    /// This exists because every duty below was previously reachable *only* from
    /// [`Node::on_rx`] or from one platform's UI loop, so a node hearing nothing
    /// did none of it: it stopped pruning, stopped rotating prekeys — quietly
    /// freezing its own forward secrecy — and never retried an unacked send.
    ///
    /// Additive on purpose: the same sweep still runs on ingest, and both are
    /// idempotent (one interval gate guards them), so a runtime that never ticks
    /// behaves exactly as it did before.
    pub fn tick(&mut self, now: u32) -> Vec<Forward> {
        self.enforce_bounds(now);
        let mut out = self.resend_unacked(now);
        // M11-P: say what we are still carrying, now and then.
        //
        // An interest that survived a journey is worth nothing until it is spoken
        // somewhere new, and no other path would speak it: a WANT goes out when a
        // neighbour asks, and the neighbour who asked is by definition not here
        // any more. Paced rather than event-driven because "we have arrived
        // somewhere new" is not a thing a node can observe — an interface coming
        // up does not mean anyone is listening, and on broadcast media there is no
        // event at all. Asking on a slow cadence covers arrival, reconnection and
        // a neighbour that simply rebooted, with one mechanism.
        if now.saturating_sub(self.last_interest_resume) >= INTEREST_RESUME_SECS {
            self.last_interest_resume = now;
            out.append(&mut self.resume_interests(now));
        }
        out
    }

    // ---- L4 request/response (RPC) --------------------------------------

    /// Call a service (an address or a served topic). Returns the request id
    /// (to match the reply) and the `Forward`s to send. The reply arrives via
    /// `take_response`.
    pub fn request(&mut self, service: Addr, req: rpc::Request, now: u32) -> (u64, Vec<Forward>) {
        let mut idb = [0u8; 8];
        OsRng.fill_bytes(&mut idb);
        let id = u64::from_be_bytes(idb);
        let payload = rpc::encode_request(id, &req);
        let mut e = Envelope::new(ty::DATA, service, now + DEFAULT_MESSAGE_EXPIRY_SECS, payload);
        if service == ZERO_DEST || self.topics.contains(&service) {
            e.flags |= fl::FLOOD;
        }
        e.sign(&self.sk);
        if e.flags & fl::FLOOD == 0 && self.paths.fresh(&service, now).is_none() {
            e.flags |= fl::FLOOD;
            e.sign(&self.sk);
        }
        self.rpc_pending.insert(id);
        self.mark_seen(&e);
        self.store_put(&e, now);
        (id, self.forward_intents(&e, NO_IFACE, now))
    }

    /// Drain requests delivered to us as a service: `(requester, req_id, request)`.
    pub fn poll_requests(&mut self) -> Vec<(Addr, u64, rpc::Request)> {
        std::mem::take(&mut self.rpc_inbox)
    }

    /// Reply to a request, routed back toward the requester.
    pub fn respond(&mut self, to: Addr, req_id: u64, resp: rpc::Response, now: u32) -> Vec<Forward> {
        let payload = rpc::encode_response(req_id, &resp);
        let mut e = Envelope::new(ty::DATA, to, now + DEFAULT_MESSAGE_EXPIRY_SECS, payload);
        e.sign(&self.sk);
        if self.paths.fresh(&to, now).is_none() {
            e.flags |= fl::FLOOD; // reverse path unknown -> flood to find it
            e.sign(&self.sk);
        }
        self.mark_seen(&e);
        self.store_put(&e, now);
        self.forward_intents(&e, NO_IFACE, now)
    }

    /// Take the response to `id` if it has arrived.
    pub fn take_response(&mut self, id: u64) -> Option<rpc::Response> {
        self.rpc_responses.remove(&id).map(|(_, r)| r)
    }

    /// Take the response to `id` along with its **authenticated** sender — the
    /// address whose key signed the reply. A caller that knows which service it
    /// asked can reject a reply that came from anyone else (a flooded response
    /// is forgeable by any node that saw the request id, so this check is what
    /// makes a pulled record trustworthy).
    pub fn take_response_from(&mut self, id: u64) -> Option<(Addr, rpc::Response)> {
        self.rpc_responses.remove(&id)
    }

    // ---- L5 feeds (pub/sub) ---------------------------------------------

    /// Follow a feed topic so its events are delivered to us.
    ///
    /// Following is **public**: §4's ANNOUNCE carries the whole topic set, so
    /// every node that hears this one learns what it reads. That is what lets a
    /// neighbour know the traffic is worth relaying here, and it is not
    /// something a caller can opt out of while still receiving the feed.
    pub fn subscribe(&mut self, topic: &str) {
        self.topics.insert(topic_of(topic));
    }

    /// Stop following a topic. `true` if it was followed.
    ///
    /// The counterpart `subscribe` never had. Without it a node could only ever
    /// accumulate topics — the set is announced, so an unwanted subscription was
    /// not merely noise in the feed but a permanent public claim about what this
    /// node reads. It takes effect on the next ANNOUNCE; anyone who already
    /// heard the old set keeps that belief until then.
    pub fn unsubscribe(&mut self, topic: &str) -> bool {
        self.topics.remove(&topic_of(topic))
    }

    /// Every topic this node follows, as the 8-byte addresses that go on the
    /// wire. `topic_of` is a hash, so the names are not recoverable from these —
    /// a caller that wants to show names must remember its own.
    pub fn subscriptions(&self) -> Vec<Addr> {
        let mut out: Vec<Addr> = self.topics.iter().copied().collect();
        out.sort_unstable(); // a HashSet has no order; a UI needs a stable one
        out
    }

    /// Publish an event to a feed topic (floods to all subscribers).
    pub fn publish(&mut self, topic: &str, event: Vec<u8>, now: u32) -> Vec<Forward> {
        let mut payload = Vec::with_capacity(1 + event.len());
        payload.push(feed::FEED_TAG);
        payload.extend_from_slice(&event);
        let mut e = Envelope::new(ty::DATA, topic_of(topic), now + DEFAULT_MESSAGE_EXPIRY_SECS, payload);
        e.flags |= fl::FLOOD;
        e.sign(&self.sk);
        self.mark_seen(&e);
        self.store_put(&e, now);
        self.forward_intents(&e, NO_IFACE, now)
    }

    /// Drain feed events received on subscribed topics.
    pub fn poll_feed(&mut self) -> Vec<feed::Event> {
        std::mem::take(&mut self.feed_inbox)
    }

    /// Our current backpressure `busy` byte (§5.4c): store fill scaled to 0–255.
    /// Neighbours use it to throttle relays toward a swamped peer.
    pub fn busy(&self) -> u8 {
        let used = self.store.bytes();
        (used.saturating_mul(255) / self.max_store_bytes.max(1)).min(255) as u8
    }
    /// The `busy` byte a peer last advertised in its ANNOUNCE, if heard.
    pub fn peer_busy(&self, a: &Addr) -> Option<u8> {
        self.peer_busy.get(a).copied()
    }
}
