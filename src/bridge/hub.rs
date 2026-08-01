//! Multi-interface hub: one shared `Node`, many bridges, cross-medium relay.
//!
//! Every bridge runs in its own thread and shares one node behind a mutex. When
//! a bridge feeds a received frame in, the hub runs the router and fans the
//! resulting forwards out to the *other* bridges' outbound queues — so a message
//! arriving over UDP is relayed onto TCP, a folder, and a Meshtastic mesh at
//! once. That is exactly a gateway node with several interfaces.

use crate::*;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
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

/// Lock a mutex, recovering from poisoning instead of propagating it.
///
/// A panic while a lock is held poisons it, and `lock().unwrap()` then panics in
/// every thread that touches it afterwards. One fault anywhere under the lock
/// would take every bridge thread with it, permanently — which is precisely the
/// remote denial of service this audit has spent its time removing, arriving
/// through a different door. The panic need not even be ours: `with_node` runs
/// arbitrary embedder code under this lock.
///
/// Recovering is the right trade for *what is protected here*. `Node` is a router
/// state machine whose every table is independently bounded and self-healing —
/// dedup expires, quotas refill, partial objects time out — so the worst a
/// half-applied `on_rx` leaves behind is one duplicate relay or one dropped
/// envelope. Continuing degraded beats dying. This reasoning does not generalise:
/// a lock guarding an invariant that other code *depends* on should still poison.
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl Hub {
    pub fn new(node: Node) -> Shared {
        Arc::new(Hub { node: Mutex::new(node), out: Mutex::new(Vec::new()), deliver: Mutex::new(None) })
    }

    /// Install an inbox that receives the wire bytes of every envelope delivered
    /// to this node (addressed to us, or on a topic we follow). Embedders — the
    /// Android app, bindings — drain the paired `Receiver`. Replaces any prior sink.
    pub fn set_delivery_sink(&self, tx: Sender<Vec<u8>>) {
        *lock(&self.deliver) = Some(tx);
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
            let mut n = lock(&self.node);
            n.send(dest, data, now())?
        };
        self.dispatch(forwards);
        Ok(())
    }

    /// Register a sending interface: returns its iface id and the queue of
    /// forwards it must transmit.
    pub fn register(&self) -> (Iface, Receiver<Forward>) {
        let (tx, rx) = channel();
        let mut o = lock(&self.out);
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
        let mut o = lock(&self.out);
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
        let mut o = lock(&self.out);
        if let Some(slot) = o.get_mut(iface as usize) {
            slot.bulk = bytes_per_sec.map(|per_sec| Budget { per_sec, allowance: 0, last: now() });
        }
    }

    /// Retire an interface: a stopped or removed bridge.
    ///
    /// The slot is emptied (`tx`/`bulk` cleared) rather than removed from the
    /// vector, so every other interface keeps its id. `dispatch` already skips a
    /// slot with no sender, and `Flood`'s `except` addresses interfaces by index —
    /// `Vec::remove` here would renumber every later interface and silently
    /// misroute the `except`, so **iface ids are never recycled** within a process.
    /// The paired `Receiver` sees the channel disconnect when its `Sender` drops.
    /// Idempotent, and a no-op for an id that was never registered.
    pub fn unregister(&self, iface: Iface) {
        let mut o = lock(&self.out);
        if let Some(slot) = o.get_mut(iface as usize) {
            slot.tx = None;
            slot.bulk = None;
        }
    }

    /// Register a pull-only interface (an HTTP bag / server that answers requests
    /// from the shared store and never has anything pushed to it).
    pub fn register_pull(&self) -> Iface {
        let mut o = lock(&self.out);
        let iface = o.len() as Iface;
        o.push(Slot { tx: None, bulk: None });
        iface
    }

    /// Feed a received frame into the router and fan its forwards to the other
    /// interfaces. Returns the envelopes delivered locally (for logging).
    pub fn on_rx(&self, iface: Iface, bytes: &[u8], nbr: Option<Addr>) -> Vec<Envelope> {
        let rx = {
            let mut n = lock(&self.node);
            n.on_rx(bytes, iface, nbr, now())
        };
        self.dispatch(rx.forwards);
        if let Some(tx) = lock(&self.deliver).as_ref() {
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

    /// Flood the node's ANNOUNCE mesh-wide (`hops = 16`).
    ///
    /// Expensive: every node that hears it relays it. Call at most once an hour
    /// ([`spore::ANNOUNCE_FLOOD_MIN_SECS`]); for the frequent beacon use
    /// [`Hub::hello`].
    pub fn beacon(&self) {
        let forwards = {
            let mut n = lock(&self.node);
            n.build_announce(now())
        };
        self.dispatch(forwards);
    }

    /// Send the link-local HELLO (`hops = 0`) on every interface.
    ///
    /// Reaches direct neighbours and stops there, so it is cheap enough to repeat
    /// on the Trickle schedule and is what carries the `busy` backpressure byte to
    /// the peers that act on it.
    pub fn hello(&self) {
        let forwards = {
            let mut n = lock(&self.node);
            n.build_hello(now())
        };
        self.dispatch(forwards);
    }

    /// Run the node's periodic work and send whatever it produces.
    ///
    /// The scheduling nutrient, wired for a hosted node: call it on a timer
    /// (roughly once a second) alongside [`Hub::hello`] and [`Hub::beacon`].
    /// Without it the node only maintains itself when traffic happens to
    /// arrive — see [`Node::tick`].
    pub fn tick(&self) {
        let forwards = {
            let mut n = lock(&self.node);
            n.tick(now())
        };
        self.dispatch(forwards);
    }

    /// Run a closure with exclusive access to the node.
    ///
    /// The closure runs **while the node lock is held**, so it must not call back
    /// into any other `Hub` method that touches the node — `send`, `on_rx`,
    /// `beacon`, `addr`, or `with_node` again. A `std::sync::Mutex` is not
    /// reentrant, so that self-deadlocks the calling thread rather than returning
    /// an error. Use the `&mut Node` you were handed; it can do everything those
    /// methods can, minus the dispatch. `dispatch` itself is safe to reach after
    /// the closure returns, which is why every hub method that needs both scopes
    /// the node guard and releases it first — that ordering is deliberate and is
    /// what keeps `node` and `out` deadlock-free.
    pub fn with_node<R>(&self, f: impl FnOnce(&mut Node) -> R) -> R {
        f(&mut lock(&self.node))
    }

    pub fn addr(&self) -> Addr {
        lock(&self.node).addr
    }

    // Flood -> every interface except the source; Directed -> the path's iface.
    fn dispatch(&self, forwards: Vec<Forward>) {
        if forwards.is_empty() {
            return;
        }
        let t = now();
        let mut o = lock(&self.out);
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
    fn unregister_stops_a_slot_without_renumbering_the_rest() {
        let hub = Hub::new(Node::new("gateway", &[]));
        let (a, a_rx) = hub.register();
        let (b, b_rx) = hub.register();
        assert_eq!((a, b), (0, 1), "ids are assigned in order");

        // Both carry a public message.
        hub.send(ZERO_DEST, b"one".to_vec()).unwrap();
        assert_eq!(drained(&a_rx), 1);
        assert_eq!(drained(&b_rx), 1);

        // Retire the first. Its receiver disconnects; the second keeps its id.
        hub.unregister(a);
        hub.send(ZERO_DEST, b"two".to_vec()).unwrap();
        assert_eq!(drained(&a_rx), 0, "a retired interface receives nothing");
        assert_eq!(drained(&b_rx), 1, "the live interface still carries, id unchanged");

        // A directed forward to the still-live id b reaches it — proof b was not
        // renumbered into the hole a left.
        let (c, c_rx) = hub.register();
        assert_eq!(c, 2, "a new interface takes a fresh id, never a's hole");
        hub.unregister(a); // idempotent
        hub.unregister(99); // unknown id: no-op, no panic
        hub.send(ZERO_DEST, b"three".to_vec()).unwrap();
        assert_eq!(drained(&b_rx), 1);
        assert_eq!(drained(&c_rx), 1);
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

    #[test]
    fn a_poisoned_lock_does_not_kill_every_other_thread() {
        // A panic under the lock poisons it. With `lock().unwrap()` every later
        // caller panicked too, so one fault anywhere became a permanently dead
        // node — the same denial of service this audit exists to remove, reached
        // through a different door.
        let hub = Hub::new(Node::new("gateway", &[]));
        let (_iface, rx) = hub.register();

        // Poison `node` deliberately, from another thread so this one survives.
        let h = hub.clone();
        let poisoned = std::thread::spawn(move || {
            let _guard = lock(&h.node);
            panic!("something under the lock went wrong");
        });
        assert!(poisoned.join().is_err(), "the helper thread really did panic");
        assert!(hub.node.is_poisoned(), "and the mutex really is poisoned");

        // The hub must still work.
        hub.send(ZERO_DEST, b"the dam holds".to_vec()).unwrap();
        assert_eq!(drained(&rx), 1, "a poisoned lock still serves traffic");
        let _ = hub.addr();
        hub.with_node(|n| n.store_len());
    }
}
