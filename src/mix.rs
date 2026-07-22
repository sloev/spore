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
pub fn onion_wrap(inner: &Envelope, hops: &[(Addr, [u8; 32])], expiry: u32) -> Option<Envelope> {
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
        let mut layer = Envelope::new(ty::DATA, *addr, expiry, sealed);
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
    /// Release due envelopes, but only while at least `min_batch` are held —
    /// so a lone message can't be traced straight through.
    pub fn ready(&mut self, now: u32) -> Vec<Vec<u8>> {
        if self.items.len() < self.min_batch {
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
        out
    }
    pub fn len(&self) -> usize {
        self.items.len()
    }
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
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
