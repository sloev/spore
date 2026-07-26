use super::{Addr, HashMap};

/// (d) Exponential backoff for FLOOD retransmits: 30 s, doubling each try,
/// capped at 1 h, at most `MAX` attempts (§5.4d, §5.6).
pub struct Backoff {
    next_at: u32,
    tries: u8,
}
impl Backoff {
    pub const BASE: u32 = 30;
    pub const CAP: u32 = 3600;
    pub const MAX: u8 = 5;
    pub fn new(now: u32) -> Self {
        Backoff { next_at: now + Self::BASE, tries: 0 }
    }
    /// A retry is due (and we still have attempts left).
    pub fn due(&self, now: u32) -> bool {
        self.tries < Self::MAX && now >= self.next_at
    }
    /// Record a fired retry and schedule the next (doubling, capped).
    pub fn fired(&mut self, now: u32) {
        self.tries += 1;
        let delay = (Self::BASE << (self.tries - 1).min(20)).min(Self::CAP);
        self.next_at = now + delay;
    }
    pub fn exhausted(&self) -> bool {
        self.tries >= Self::MAX
    }
    pub fn tries(&self) -> u8 {
        self.tries
    }
}

/// (b) Trickle timer for HELLO/ANNOUNCE: the interval doubles from `min` to
/// `max` while nothing new is heard, and snaps back to `min` on any novelty.
pub struct Trickle {
    min: u32,
    max: u32,
    cur: u32,
    fire_at: u32,
}
impl Trickle {
    pub fn new(now: u32, min: u32, max: u32) -> Self {
        Trickle { min, max, cur: min, fire_at: now + min }
    }
    pub fn due(&self, now: u32) -> bool {
        now >= self.fire_at
    }
    pub fn fired(&mut self, now: u32) {
        self.cur = (self.cur * 2).min(self.max);
        self.fire_at = now + self.cur;
    }
    /// Something new was heard — reset to the fast interval.
    pub fn reset(&mut self, now: u32) {
        self.cur = self.min;
        self.fire_at = now + self.min;
    }
    pub fn interval(&self) -> u32 {
        self.cur
    }
}

/// (a) Token bucket capping relayed bytes to a sustained rate (law on ISM
/// bands: ≤ 10 % airtime). Time is in seconds; `allow` refills then spends.
pub struct TokenBucket {
    rate: u32, // bytes/sec sustained
    burst: u32,
    tokens: u32,
    last: u32,
}
impl TokenBucket {
    pub fn new(rate: u32) -> Self {
        let burst = rate.max(2048); // hold at least one full frame
        TokenBucket { rate, burst, tokens: rate, last: 0 }
    }
    /// Size the bucket at 10 % of a link's raw capacity (bytes/sec).
    pub fn ten_percent(link_bytes_per_sec: u32) -> Self {
        Self::new((link_bytes_per_sec / 10).max(1))
    }
    /// May we spend `bytes` of relay budget now? Refills by elapsed time.
    pub fn allow(&mut self, bytes: u32, now: u32) -> bool {
        let refill = now.saturating_sub(self.last).saturating_mul(self.rate);
        self.tokens = self.tokens.saturating_add(refill).min(self.burst);
        self.last = now;
        if self.tokens >= bytes {
            self.tokens -= bytes;
            true
        } else {
            false
        }
    }
    /// When this bucket last spent tokens — used to evict idle sources.
    pub fn last_active(&self) -> u32 {
        self.last
    }
}

/// (d) Per-source quotas (§10): a token bucket *per originating address* so no
/// single node can flood the mesh beyond a sustained byte rate. A relay charges
/// each signed envelope against its source's bucket before storing/forwarding
/// it; over-budget traffic is dropped (still delivered locally if it's for us).
/// Stamped (proof-of-work) mail bypasses the quota, matching `admit`.
/// The lowest stamp class that buys a full exemption from the per-source quota.
///
/// The spec is explicit that "priority is bought, not claimed" (§2) and that a
/// stamp is "proof of work" (§10) — so the exemption has to cost something. A
/// stamp is the count of leading zero bits of the envelope id, which is a hash,
/// so **class 1 is not work**: half of all envelopes have it by chance and
/// grinding one costs about two tries. Exempting `stamp > 0`, as this once did,
/// therefore let roughly half of all traffic past the quota at random and let a
/// flooder past it deliberately for double the hashing — which is to say §10 did
/// not bound anything.
///
/// 16 bits is ~65k tries: milliseconds on a laptop, seconds on a microcontroller,
/// affordable once for an genuinely urgent message and ruinous for a flooder who
/// must pay it *per envelope*. Below this class mail still flows — it is charged
/// to its source's budget like anything else, and stamp still orders eviction and
/// TX priority (§10.3). This constant governs one thing: skipping the bucket.
pub const STAMP_EXEMPT_CLASS: u8 = 16;

/// Bucket for traffic that names a source we could not verify — see
/// [`Quotas::admit`] and the attribution rules in `Node::ingest`. One shared
/// bucket, so unverifiable claims are still bounded but cannot be aimed at any
/// particular victim's budget.
pub const UNATTRIBUTED: Addr = [0u8; 8];

pub struct Quotas {
    rate: u32,
    max_sources: usize,
    per_src: HashMap<Addr, TokenBucket>,
}
impl Quotas {
    /// `rate_bytes_per_sec` is the sustained budget allowed to each source.
    pub fn new(rate_bytes_per_sec: u32) -> Self {
        Quotas { rate: rate_bytes_per_sec, max_sources: 4096, per_src: HashMap::new() }
    }
    /// Charge `bytes` from `src`'s budget; `true` if within quota. Mail stamped
    /// to at least [`STAMP_EXEMPT_CLASS`] (proof-of-work) always passes.
    pub fn admit(&mut self, src: Addr, bytes: u32, stamp: u8, now: u32) -> bool {
        if stamp >= STAMP_EXEMPT_CLASS {
            return true;
        }
        if self.per_src.len() >= self.max_sources && !self.per_src.contains_key(&src) {
            self.evict_oldest();
        }
        let rate = self.rate;
        self.per_src.entry(src).or_insert_with(|| TokenBucket::new(rate)).allow(bytes, now)
    }
    /// Number of sources currently tracked.
    pub fn tracked(&self) -> usize {
        self.per_src.len()
    }
    // Drop the least-recently-active source so the table can't grow without
    // bound under a spray of forged/one-off source addresses.
    fn evict_oldest(&mut self) {
        if let Some(k) = self.per_src.iter().min_by_key(|(_, b)| b.last_active()).map(|(k, _)| *k) {
            self.per_src.remove(&k);
        }
    }
}

/// (c) Backpressure: a peer advertises a `busy` byte (queue fill 0–255);
/// neighbours admit a send with probability (255−busy)/255 and let stamped
/// (proof-of-work priority) mail through regardless. `roll` is a random byte.
///
/// "Stamped" means [`STAMP_EXEMPT_CLASS`] or better, for the same reason it does
/// in [`Quotas::admit`]: class 1 is two hashes' work, so treating any non-zero
/// stamp as priority would let a sender ignore a busy peer's backpressure for
/// free — and backpressure only works if ignoring it costs more than heeding it.
pub fn admit(busy: u8, stamp: u8, roll: u8) -> bool {
    if busy == 0 || stamp >= STAMP_EXEMPT_CLASS {
        return true;
    }
    (roll as u16) < (255 - busy as u16)
}
