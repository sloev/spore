//! Node — origination, unicast/broadcast send, receipts, RPC, feed, busy state.
//!
//! Split out of the 3977-line `lib.rs` (task #23): `impl Node` in a descendant
//! module of the crate root, so it keeps full access to `Node`'s private fields
//! with no visibility change. A pure move — wire format and public API identical.

use crate::*;

impl Node {
    pub(crate) fn store_put(&mut self, e: &Envelope) {
        self.store.put(e.id(), e.wire(), e.expiry, e.stamp(), self.seq, e.dest);
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
                    pinned.insert(*id);
                }
                true
            });
        }
        pinned
    }

    /// Originate a signed public (flooded) message on topic/broadcast `dest`.
    pub fn originate(&mut self, dest: Addr, payload: Vec<u8>, now: u32) -> Vec<Forward> {
        let mut e = Envelope::new(ty::DATA, dest, now + 7 * 86400, payload);
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
        self.store_put(&e);
        self.forward_intents(&e, NO_IFACE, now)
    }

    /// High-level send: deliver `data` of *any* size to an address or topic.
    ///
    /// Small payloads ride a single signed envelope (identical to `originate`).
    /// Anything larger than `self.mtu` is fountain-fragmented (§3) into equal
    /// chunks plus a margin of repair chunks, so it survives lossy, reordered,
    /// even one-way delivery; the receiver reassembles and verifies the original
    /// signature before the app ever sees it. Callers never think about MTUs.
    ///
    /// One fountain set caps at ~`mtu`×255 (≈ 50 KB at defaults); larger objects
    /// belong to the manifest+swarm layer (files), not a single `send`.
    /// Originate a signed DATA message, fountain-fragmenting it if it exceeds the
    /// MTU.
    ///
    /// Returns [`TooLarge`] rather than panicking when the object needs more than
    /// [`MAX_FOUNTAIN_CHUNKS`] chunks at the MTU in force. That ceiling is
    /// structural, not policy — the fragment header carries `count` as one wire
    /// byte — so exceeding it is a property of the payload the caller handed over:
    /// an error to report, not a bug to abort on. For objects that large, use the
    /// file/manifest layer, which exists for exactly this.
    pub fn send(&mut self, dest: Addr, data: Vec<u8>, now: u32) -> Result<Vec<Forward>, TooLarge> {
        let mut e = Envelope::new(ty::DATA, dest, now + 7 * 86400, data);
        if dest == ZERO_DEST || self.topics.contains(&dest) {
            e.flags |= fl::FLOOD;
        }
        e.sign(&self.sk);
        // Unicast with no known path: flood to discover it (§5.6).
        if e.flags & fl::FLOOD == 0 && self.paths.fresh(&dest, now).is_none() {
            e.flags |= fl::FLOOD;
            e.sign(&self.sk);
        }

        let wire = e.wire();
        if wire.len() <= self.mtu {
            self.mark_seen(&e);
            self.store_put(&e);
            return Ok(self.forward_intents(&e, NO_IFACE, now));
        }

        // Too big for one envelope: fountain-fragment the signed wire form.
        let chunk = self.mtu.saturating_sub(FRAG_OVERHEAD).max(1);
        let count = wire.len().div_ceil(chunk);
        if count > MAX_FOUNTAIN_CHUNKS {
            return Err(TooLarge { needed: count, chunk });
        }
        let orig_id = e.id();
        // Data chunks 0..count, then a few repair chunks for loss resilience.
        let repair = (count / 8 + 2).min(MAX_FOUNTAIN_CHUNKS - count);
        let indices: Vec<u8> = (0..(count + repair)).map(|i| i as u8).collect();
        let frags = fragment(&wire, chunk, e.hops, e.expiry, dest, orig_id, &indices);

        let mut forwards = Vec::new();
        for fr in &frags {
            self.mark_seen(fr);
            self.store_put(fr);
            forwards.append(&mut self.forward_intents(fr, NO_IFACE, now));
        }
        Ok(forwards)
    }

    /// Originate a unicast message that asks the recipient for a delivery
    /// receipt (§8). Tracks it for backoff resend until a receipt arrives.
    pub fn originate_ackreq(&mut self, dest: Addr, payload: Vec<u8>, now: u32) -> Vec<Forward> {
        let mut e = Envelope::new(ty::DATA, dest, now + 7 * 86400, payload);
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
        self.store_put(&e);
        self.pending.insert(id, Pending { wire: e.wire(), backoff: congestion::Backoff::new(now) });
        self.forward_intents(&e, NO_IFACE, now)
    }

    /// Has a receipt for `id` come back?
    pub fn acked(&self, id: &Id) -> bool {
        self.acked.contains(id)
    }

    /// Send a **direct message**: sealed to the peer's prekey when we know it
    /// (§7), and flagged `ACKREQ` so the recipient returns a delivery receipt
    /// (§8). Returns the envelope id — poll [`Node::acked`] for delivery — and
    /// whether it actually went out encrypted.
    ///
    /// A peer's prekey arrives with their ANNOUNCE, so the first message to a
    /// stranger may go as signed cleartext; once we've heard them, everything
    /// after is sealed. This is the one call a messenger UI needs.
    pub fn send_direct(&mut self, dest: Addr, plaintext: &[u8], now: u32) -> (Id, Vec<Forward>, bool) {
        let (payload, encrypted) = match self.peer_prekey(&dest) {
            Some(pk) => (seal(plaintext, &pk), true),
            None => (plaintext.to_vec(), false),
        };
        let mut e = Envelope::new(ty::DATA, dest, now + 7 * 86400, payload);
        e.flags |= fl::ACKREQ;
        if encrypted {
            e.flags |= fl::ENCRYPTED;
        }
        e.sign(&self.sk);
        // Unicast with no known path: flood to discover it (§5.6).
        if self.paths.fresh(&dest, now).is_none() {
            e.flags |= fl::FLOOD;
            e.sign(&self.sk);
        }
        let id = e.id();
        self.mark_seen(&e);
        self.store_put(&e);
        self.pending.insert(id, Pending { wire: e.wire(), backoff: congestion::Backoff::new(now) });
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

    // ---- L4 request/response (RPC) --------------------------------------

    /// Call a service (an address or a served topic). Returns the request id
    /// (to match the reply) and the `Forward`s to send. The reply arrives via
    /// `take_response`.
    pub fn request(&mut self, service: Addr, req: rpc::Request, now: u32) -> (u64, Vec<Forward>) {
        let mut idb = [0u8; 8];
        OsRng.fill_bytes(&mut idb);
        let id = u64::from_be_bytes(idb);
        let payload = rpc::encode_request(id, &req);
        let mut e = Envelope::new(ty::DATA, service, now + 7 * 86400, payload);
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
        self.store_put(&e);
        (id, self.forward_intents(&e, NO_IFACE, now))
    }

    /// Drain requests delivered to us as a service: `(requester, req_id, request)`.
    pub fn poll_requests(&mut self) -> Vec<(Addr, u64, rpc::Request)> {
        std::mem::take(&mut self.rpc_inbox)
    }

    /// Reply to a request, routed back toward the requester.
    pub fn respond(&mut self, to: Addr, req_id: u64, resp: rpc::Response, now: u32) -> Vec<Forward> {
        let payload = rpc::encode_response(req_id, &resp);
        let mut e = Envelope::new(ty::DATA, to, now + 7 * 86400, payload);
        e.sign(&self.sk);
        if self.paths.fresh(&to, now).is_none() {
            e.flags |= fl::FLOOD; // reverse path unknown -> flood to find it
            e.sign(&self.sk);
        }
        self.mark_seen(&e);
        self.store_put(&e);
        self.forward_intents(&e, NO_IFACE, now)
    }

    /// Take the response to `id` if it has arrived.
    pub fn take_response(&mut self, id: u64) -> Option<rpc::Response> {
        self.rpc_responses.remove(&id)
    }

    // ---- L5 feeds (pub/sub) ---------------------------------------------

    /// Follow a feed topic so its events are delivered to us.
    pub fn subscribe(&mut self, topic: &str) {
        self.topics.insert(topic_of(topic));
    }

    /// Publish an event to a feed topic (floods to all subscribers).
    pub fn publish(&mut self, topic: &str, event: Vec<u8>, now: u32) -> Vec<Forward> {
        let mut payload = Vec::with_capacity(1 + event.len());
        payload.push(feed::FEED_TAG);
        payload.extend_from_slice(&event);
        let mut e = Envelope::new(ty::DATA, topic_of(topic), now + 7 * 86400, payload);
        e.flags |= fl::FLOOD;
        e.sign(&self.sk);
        self.mark_seen(&e);
        self.store_put(&e);
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
