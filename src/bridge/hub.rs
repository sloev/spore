//! Multi-interface hub: one shared `Node`, many bridges, cross-medium relay.
//!
//! Every bridge runs in its own thread and shares one node behind a mutex. When
//! a bridge feeds a received frame in, the hub runs the router and fans the
//! resulting forwards out to the *other* bridges' outbound queues — so a message
//! arriving over UDP is relayed onto TCP, a folder, and a Meshtastic mesh at
//! once. That is exactly a gateway node with several interfaces.

use crate::*;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

/// Convenience handle shared across bridge threads.
pub type Shared = Arc<Hub>;

/// Wall-clock seconds (the router's time base).
pub fn now() -> u32 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as u32
}

/// A leaky bucket over one interface's share of bulk traffic.
struct Budget {
    per_sec: u32,
    allowance: u32,
    last: u32,
}

impl Budget {
    /// Unused budget accumulates for a few seconds, so a quiet link can answer a
    /// small burst promptly without being able to save up indefinitely.
    fn admit(&mut self, bytes: usize, now: u32) -> bool {
        const BURST_SECS: u32 = 8;
        let elapsed = now.saturating_sub(self.last);
        if elapsed > 0 {
            self.allowance = self
                .allowance
                .saturating_add(elapsed.saturating_mul(self.per_sec))
                .min(self.per_sec.saturating_mul(BURST_SECS));
            self.last = now;
        }
        let need = bytes as u32;
        if need <= self.allowance {
            self.allowance -= need;
            true
        } else {
            false
        }
    }
}

/// One registered interface: where to send, and what it is willing to carry.
struct Slot {
    tx: Option<Sender<Forward>>, // None = pull-only
    bulk: Option<Budget>,        // None = no limit
}

impl Slot {
    fn admit_bulk(&mut self, bytes: usize, now: u32) -> bool {
        match &mut self.bulk {
            Some(b) => b.admit(bytes, now),
            None => true,
        }
    }
}

/// Is this envelope **bulk** — the traffic that makes a transfer big?
///
/// Only file chunks are. A manifest is what makes a file findable at all and is
/// a single frame, so it always passes, as does every message, announce and
/// receipt. That is the point: a slow link stays useful for conversation and for
/// telling the mesh what exists, and simply declines to be the pipe for
/// somebody's gigabyte.
fn is_bulk(wire: &[u8]) -> bool {
    matches!(Envelope::decode(wire), Ok((e, _))
        if e.typ == ty::DATA && e.payload.first() == Some(&file::CHUNK_TAG))
}

pub struct Hub {
    node: Mutex<Node>,
    out: Mutex<Vec<Slot>>,                   // index = iface id
    deliver: Mutex<Option<Sender<Vec<u8>>>>, // optional app inbox: delivered envelope wires
}

impl Hub {
    pub fn new(node: Node) -> Shared {
        Arc::new(Hub { node: Mutex::new(node), out: Mutex::new(Vec::new()), deliver: Mutex::new(None) })
    }

    /// Install an inbox that receives the wire bytes of every envelope delivered
    /// to this node (addressed to us, or on a topic we follow). Embedders — the
    /// Android app, bindings — drain the paired `Receiver`. Replaces any prior sink.
    pub fn set_delivery_sink(&self, tx: Sender<Vec<u8>>) {
        *self.deliver.lock().unwrap() = Some(tx);
    }

    /// Originate a signed app message to `dest` (all-zero = public) and flood it
    /// onto every interface. The convenience the daemon's `main` and the Android
    /// app both use to *send*.
    ///
    /// Forwards [`TooLarge`] from [`Node::send`] rather than hiding it: an object
    /// past one fountain set is the caller's payload choice, and silently sending
    /// nothing would be the worst of the available outcomes.
    pub fn send(&self, dest: Addr, data: Vec<u8>) -> Result<(), crate::TooLarge> {
        let forwards = {
            let mut n = self.node.lock().unwrap();
            n.send(dest, data, now())?
        };
        self.dispatch(forwards);
        Ok(())
    }

    /// Register a sending interface: returns its iface id and the queue of
    /// forwards it must transmit.
    pub fn register(&self) -> (Iface, Receiver<Forward>) {
        let (tx, rx) = channel();
        let mut o = self.out.lock().unwrap();
        let iface = o.len() as Iface;
        o.push(Slot { tx: Some(tx), bulk: None });
        (iface, rx)
    }

    /// Register a sending interface that will carry at most `bulk_bytes_per_sec`
    /// of **other people's file chunks**.
    ///
    /// Manifests, messages, announces and receipts are never counted, so a
    /// limited link stays fully useful for conversation and for learning what
    /// exists — it just declines to be the pipe for a large transfer. `0` refuses
    /// bulk outright, which is the right answer for a link measured in tens of
    /// bytes per second: see [`crate::bridge::audio::BULK_BYTES_PER_SEC`].
    ///
    /// Nothing breaks when a link refuses: the chunks are content-addressed, so
    /// whoever wants them asks again and any other path answers.
    pub fn register_limited(&self, bulk_bytes_per_sec: u32) -> (Iface, Receiver<Forward>) {
        let (tx, rx) = channel();
        let mut o = self.out.lock().unwrap();
        let iface = o.len() as Iface;
        o.push(Slot {
            tx: Some(tx),
            bulk: Some(Budget { per_sec: bulk_bytes_per_sec, allowance: 0, last: now() }),
        });
        (iface, rx)
    }

    /// Change what an interface will carry after the fact. `None` lifts the
    /// limit entirely.
    pub fn set_bulk_budget(&self, iface: Iface, bytes_per_sec: Option<u32>) {
        let mut o = self.out.lock().unwrap();
        if let Some(slot) = o.get_mut(iface as usize) {
            slot.bulk = bytes_per_sec.map(|per_sec| Budget { per_sec, allowance: 0, last: now() });
        }
    }

    /// Register a pull-only interface (an HTTP bag / server that answers requests
    /// from the shared store and never has anything pushed to it).
    pub fn register_pull(&self) -> Iface {
        let mut o = self.out.lock().unwrap();
        let iface = o.len() as Iface;
        o.push(Slot { tx: None, bulk: None });
        iface
    }

    /// Feed a received frame into the router and fan its forwards to the other
    /// interfaces. Returns the envelopes delivered locally (for logging).
    pub fn on_rx(&self, iface: Iface, bytes: &[u8], nbr: Option<Addr>) -> Vec<Envelope> {
        let rx = {
            let mut n = self.node.lock().unwrap();
            n.on_rx(bytes, iface, nbr, now())
        };
        self.dispatch(rx.forwards);
        if let Some(tx) = self.deliver.lock().unwrap().as_ref() {
            for e in &rx.delivered {
                let _ = tx.send(e.wire());
            }
        }
        rx.delivered
    }

    /// Send a node-originated batch of forwards (a beacon, an app message) out
    /// every interface.
    pub fn originate(&self, forwards: Vec<Forward>) {
        self.dispatch(forwards);
    }

    /// Flood the node's ANNOUNCE beacon on every interface.
    pub fn beacon(&self) {
        let forwards = {
            let mut n = self.node.lock().unwrap();
            n.build_announce(now())
        };
        self.dispatch(forwards);
    }

    /// Run a closure with exclusive access to the node.
    pub fn with_node<R>(&self, f: impl FnOnce(&mut Node) -> R) -> R {
        f(&mut self.node.lock().unwrap())
    }

    pub fn addr(&self) -> Addr {
        self.node.lock().unwrap().addr
    }

    // Flood -> every interface except the source; Directed -> the path's iface.
    fn dispatch(&self, forwards: Vec<Forward>) {
        if forwards.is_empty() {
            return;
        }
        let t = now();
        let mut o = self.out.lock().unwrap();
        for f in forwards {
            // Classify once per forward, not once per interface — decoding is
            // the expensive part and the answer is the same for all of them.
            let (Forward::Flood { bytes, .. } | Forward::Directed { bytes, .. }) = &f;
            let (len, bulk) = (bytes.len(), is_bulk(bytes));

            match &f {
                Forward::Flood { except, .. } => {
                    for (i, slot) in o.iter_mut().enumerate() {
                        if i as Iface == *except || slot.tx.is_none() {
                            continue;
                        }
                        if bulk && !slot.admit_bulk(len, t) {
                            continue; // this link is not carrying that for you
                        }
                        if let Some(tx) = &slot.tx {
                            let _ = tx.send(f.clone());
                        }
                    }
                }
                Forward::Directed { iface, .. } => {
                    if let Some(slot) = o.get_mut(*iface as usize) {
                        if slot.tx.is_some() && (!bulk || slot.admit_bulk(len, t)) {
                            if let Some(tx) = &slot.tx {
                                let _ = tx.send(f.clone());
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Drain a bridge's outbound queue.
    fn drained(rx: &Receiver<Forward>) -> usize {
        let mut n = 0;
        while rx.try_recv().is_ok() {
            n += 1;
        }
        n
    }

    #[test]
    fn a_slow_link_carries_the_conversation_but_not_the_freight() {
        let hub = Hub::new(Node::new("gateway", &[]));
        let (_fast, fast_rx) = hub.register();
        let (_slow, slow_rx) = hub.register_limited(0); // an audio-modem-shaped link

        // A message reaches both — a paced link is still a full member of the
        // mesh for everything that isn't bulk.
        hub.send(ZERO_DEST, b"the dam holds".to_vec()).unwrap();
        assert_eq!(drained(&fast_rx), 1);
        assert_eq!(drained(&slow_rx), 1, "a slow link still carries messages");

        // Publishing a file floods its manifest, which is also not bulk: it is
        // one frame, and it is what makes the file findable at all.
        let body: Vec<u8> = (0..40_000u32).map(|i| i as u8).collect();
        let forwards = hub.with_node(|n| n.publish_file("big.bin", &body, ZERO_DEST, now()).1);
        hub.originate(forwards);
        assert_eq!(drained(&slow_rx), 1, "a manifest is not freight");

        // But answering a WANT with actual chunks is. The fast link serves them;
        // the slow one declines, and the chunks route around it.
        let magnet = hub.with_node(|n| n.files()[0].0);
        let chunk_ids: Vec<u8> = hub.with_node(|n| {
            let mut v = Vec::new();
            for id in n.missing(&magnet, 8).iter().chain(std::iter::empty()) {
                v.extend_from_slice(id);
            }
            // Nothing is missing — we published it — so ask for what we hold.
            if v.is_empty() {
                v.extend_from_slice(&n.stored_ids()[..16 * 8]);
            }
            v
        });
        let want = Envelope::new(ty::WANT, ZERO_DEST, 0, chunk_ids).wire();
        hub.on_rx(9, &want, None); // arrives on an iface neither of ours

        let fast_served = drained(&fast_rx);
        assert!(fast_served > 0, "the fast link answered with chunks");
        assert_eq!(drained(&slow_rx), 0, "the slow link refused to haul them");
    }

    #[test]
    fn a_budget_refills_over_time_rather_than_latching_shut() {
        let t = 1_000_000u32;
        let mut b = Budget { per_sec: 100, allowance: 100, last: t };
        assert!(b.admit(100, t), "spends its allowance");
        assert!(!b.admit(1, t), "and then has none");
        assert!(b.admit(100, t + 1), "a second later it can carry again");
        // Unused budget accumulates, but only so far.
        let mut c = Budget { per_sec: 100, allowance: 0, last: t };
        assert!(c.admit(800, t + 3600), "an hour idle buys a burst");
        assert!(!c.admit(1, t + 3600), "but not an unbounded one");
    }
}
