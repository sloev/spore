use super::*;

/// Marks a peeled payload as an onion layer (`'O'`).
pub const ONION_TAG: u8 = b'O';
/// Pad classes that hide depth (spec §9).
pub const SIZE_CLASSES: [usize; 3] = [256, 1024, 4096];

fn pad_to_class(v: &mut Vec<u8>) {
    for &c in &SIZE_CLASSES {
        if v.len() <= c {
            v.resize(c, 0);
            return;
        }
    }
    // Larger than the top class: round up to a whole class multiple.
    let top = SIZE_CLASSES[SIZE_CLASSES.len() - 1];
    let target = v.len().div_ceil(top) * top;
    v.resize(target, 0);
}

/// Wrap `inner` for delivery through `hops` (first hop = outermost). Each
/// `(addr, prekey)` is a mix that follows topic `mix`. Returns the outermost
/// envelope to inject; `None` if `hops` is empty.
pub fn onion_wrap(inner: &Envelope, hops: &[(Addr, [u8; 32])], created_at: u32) -> Option<Envelope> {
    if hops.is_empty() {
        return None;
    }
    let mut current = inner.wire();
    let mut outer = None;
    for (addr, prekey) in hops.iter().rev() {
        let mut plain = Vec::with_capacity(1 + current.len());
        plain.push(ONION_TAG);
        plain.extend_from_slice(&current);
        pad_to_class(&mut plain);
        let sealed = seal(&plain, prekey);
        let mut layer = Envelope::new(ty::DATA, *addr, created_at, sealed);
        // Unsigned (sender anonymity) and flooded so it reaches the mix.
        layer.flags |= fl::ENCRYPTED | fl::FLOOD;
        current = layer.wire();
        outer = Some(layer);
    }
    outer
}

/// A mix's release queue (§9 timing): hold peeled inner envelopes, then let
/// them out only once a minimum batch has gathered *and* each item's random
/// delay has elapsed — breaking the timing link between arrival and re-send.
/// (Poisson delays and decoy onions are the runner's policy; this is the
/// batching core.)
pub struct Batch {
    items: Vec<(Vec<u8>, u32)>, // (inner wire, release_at)
    min_batch: usize,
}
impl Batch {
    pub fn new(min_batch: usize) -> Self {
        Batch { items: Vec::new(), min_batch }
    }
    /// Queue a peeled inner envelope with a random `delay` before release.
    pub fn add(&mut self, inner: Vec<u8>, now: u32, delay: u32) {
        self.items.push((inner, now + delay));
    }
    /// Release a batch of at least `min_batch` due envelopes, **shuffled**.
    ///
    /// Two properties, and this used to have neither.
    ///
    /// *At least `min_batch`* — §9 says "batches ≥ 3", and the old check was on
    /// how many were *held* rather than how many were *due*. Three held with one
    /// due released that one alone, which is precisely the traceable singleton
    /// the threshold exists to prevent. If a batch cannot be filled the items
    /// wait, and §9's decoy traffic is what tops up a quiet mix — that is the job
    /// decoys exist for, and leaning on them here is the design working rather
    /// than a starvation risk.
    ///
    /// *Shuffled* — the old version walked `items` in insertion order, so the
    /// k-th message in was the k-th message out and an observer could correlate
    /// by position alone. A mix that preserves order is not mixing: the delays
    /// hide *when* a message left and the batch hides *which* one it was, and
    /// without the shuffle the second buys nothing.
    pub fn ready(&mut self, now: u32) -> Vec<Vec<u8>> {
        let due = self.items.iter().filter(|(_, at)| *at <= now).count();
        if due < self.min_batch {
            return Vec::new();
        }
        let mut out = Vec::new();
        let mut keep = Vec::new();
        for (w, at) in std::mem::take(&mut self.items) {
            if at <= now {
                out.push(w);
            } else {
                keep.push((w, at));
            }
        }
        self.items = keep;
        shuffle(&mut out);
        out
    }
    pub fn len(&self) -> usize {
        self.items.len()
    }
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

/// Fisher-Yates, from the host's randomness.
///
/// Not seeded: a mix's output order is the one thing an observer is trying to
/// predict, so it should not be derivable from anything they could learn. The
/// `TimingPolicy` RNG next door *is* seeded, deliberately, because delays have to
/// be reproducible in tests — this is the part that must not be.
fn shuffle<T>(v: &mut [T]) {
    for i in (1..v.len()).rev() {
        let mut b = [0u8; 8];
        crate::fill_random(&mut b);
        v.swap(i, (u64::from_be_bytes(b) % (i as u64 + 1)) as usize);
    }
}

/// §9 timing policy: the runner's half of mixing. It turns a uniform RNG into the
/// **exponential (Poisson-process) delays** each held onion waits before release —
/// so re-send times carry no information about arrival times — and decides when to
/// emit **decoy** (cover) traffic so a silent mix is indistinguishable from a busy
/// one. Deterministic from a seed, so it's testable; seed it from `OsRng` in
/// production. Pair it with [`Batch`]: `policy.delay()` feeds `batch.add`.
pub struct TimingPolicy {
    mean_delay: u32, // seconds; the mean of the exponential release delay
    decoy_rate: u8,  // 0..=255 chance per tick of emitting a cover onion
    rng: u64,
}
impl TimingPolicy {
    pub fn new(mean_delay: u32, decoy_rate: u8, seed: u64) -> Self {
        TimingPolicy { mean_delay: mean_delay.max(1), decoy_rate, rng: seed | 1 }
    }
    fn next_u32(&mut self) -> u32 {
        // xorshift64* — deterministic, good enough for delay jitter.
        self.rng ^= self.rng >> 12;
        self.rng ^= self.rng << 25;
        self.rng ^= self.rng >> 27;
        (self.rng.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 32) as u32
    }
    /// An exponentially distributed delay (seconds) with the configured mean —
    /// the inter-departure time of a Poisson process (inverse-CDF `-mean·ln U`).
    pub fn delay(&mut self) -> u32 {
        let u = (self.next_u32() as f64 + 1.0) / (u32::MAX as f64 + 1.0); // (0,1]
        (-(self.mean_delay as f64) * u.ln()).round().clamp(0.0, u32::MAX as f64) as u32
    }
    /// Whether to inject one decoy onion this tick (cover traffic).
    pub fn decoy_due(&mut self) -> bool {
        (self.next_u32() & 0xff) < self.decoy_rate as u32
    }
    /// A random padding size class for a decoy's inner bytes, so cover onions look
    /// exactly like real ones on the wire.
    pub fn decoy_class(&mut self) -> usize {
        SIZE_CLASSES[(self.next_u32() as usize) % SIZE_CLASSES.len()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn poisson_delays_average_near_the_mean() {
        let mut p = TimingPolicy::new(100, 0, 0xC0FFEE);
        let n = 20_000u64;
        let sum: u64 = (0..n).map(|_| p.delay() as u64).sum();
        let mean = sum as f64 / n as f64;
        // Exponential(mean=100): sample mean should land close to 100.
        assert!((80.0..125.0).contains(&mean), "sample mean {mean} off");
    }

    #[test]
    fn decoys_fire_near_the_configured_rate() {
        let mut p = TimingPolicy::new(30, 64, 0x1234_5678); // 64/256 ≈ 25 %
        let n = 20_000;
        let fired = (0..n).filter(|_| p.decoy_due()).count();
        let rate = fired as f64 / n as f64;
        assert!((0.20..0.30).contains(&rate), "decoy rate {rate} off");
    }
}

#[cfg(test)]
mod properties {
    //! Property tests for the mix's release machine (#278).
    //!
    //! Both of the failures below were live, and both were invisible to anything
    //! that checked the module worked: messages went in and came out, correct and
    //! intact. What was wrong was *when* and *in what order*, which is the entire
    //! content of what a mix does.
    use super::*;

    const T0: u32 = 1_000;

    /// Add `delays.len()` messages at `T0`, body = index.
    fn queue(min_batch: usize, delays: &[u32]) -> Batch {
        let mut b = Batch::new(min_batch);
        for (i, d) in delays.iter().enumerate() {
            b.add(vec![i as u8], T0, *d);
        }
        b
    }

    #[test]
    fn a_batch_smaller_than_the_threshold_is_never_released() {
        // §9 says "batches >= 3". The check used to be on how many were *held*
        // rather than how many were *due*, so three held with one due released
        // that one alone — the traceable singleton the threshold exists to stop.
        let mut b = queue(3, &[1, 50, 50]);
        assert!(b.ready(T0 + 1).is_empty(), "one due out of three held must release nothing");
        assert!(b.ready(T0 + 49).is_empty(), "still only one due");
        assert_eq!(b.ready(T0 + 50).len(), 3, "and when three are due, three leave together");
    }

    #[test]
    fn release_order_is_not_arrival_order() {
        // A mix that preserves order is not mixing: an observer correlates by
        // position without needing the timings at all. Run it enough times that
        // "it happened to come out sorted once" is not the explanation.
        let mut seen_reordered = false;
        for _ in 0..40 {
            let mut b = queue(6, &[1, 1, 1, 1, 1, 1]);
            let out: Vec<u8> = b.ready(T0 + 2).iter().map(|v| v[0]).collect();
            assert_eq!(out.len(), 6, "all six were due");
            let sorted: Vec<u8> = (0..6).collect();
            if out != sorted {
                seen_reordered = true;
            }
        }
        assert!(seen_reordered, "forty batches of six all came out in arrival order");
    }

    #[test]
    fn nothing_is_lost_duplicated_or_released_early() {
        // The boring properties, which still have to hold once shuffling is
        // involved — a shuffle is an easy place to drop or repeat an element.
        let delays: Vec<u32> = (0..30).map(|i| (i * 7) % 23 + 1).collect();
        let mut b = queue(3, &delays);
        let mut out: Vec<u8> = Vec::new();
        for t in 0..40u32 {
            for w in b.ready(T0 + t) {
                let idx = w[0] as usize;
                assert!(
                    T0 + t >= T0 + delays[idx],
                    "message {idx} left at {t} but its delay was {}",
                    delays[idx]
                );
                out.push(w[0]);
            }
        }
        out.sort_unstable();
        let all: Vec<u8> = (0..30).collect();
        assert_eq!(out, all, "every message left exactly once");
        assert!(b.is_empty());
    }

    #[test]
    fn a_quiet_mix_holds_rather_than_leaking() {
        // The cost of the threshold, stated rather than discovered: below it,
        // messages wait indefinitely. Section 9's answer is decoy traffic, and
        // this is the test that says why a mix needs it.
        let mut b = queue(3, &[1, 1]);
        assert!(b.ready(T0 + 1_000_000).is_empty(), "two will never make a batch of three");
        assert_eq!(b.len(), 2, "and they are held, not dropped");
        b.add(vec![99], T0, 1);
        assert_eq!(b.ready(T0 + 2).len(), 3, "a third — real or decoy — releases all of them");
    }
}
