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
