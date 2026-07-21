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

pub struct Hub {
    node: Mutex<Node>,
    out: Mutex<Vec<Option<Sender<Forward>>>>, // index = iface id; None = pull-only
}

impl Hub {
    pub fn new(node: Node) -> Shared {
        Arc::new(Hub { node: Mutex::new(node), out: Mutex::new(Vec::new()) })
    }

    /// Register a sending interface: returns its iface id and the queue of
    /// forwards it must transmit.
    pub fn register(&self) -> (Iface, Receiver<Forward>) {
        let (tx, rx) = channel();
        let mut o = self.out.lock().unwrap();
        let iface = o.len() as Iface;
        o.push(Some(tx));
        (iface, rx)
    }

    /// Register a pull-only interface (an HTTP bag / server that answers requests
    /// from the shared store and never has anything pushed to it).
    pub fn register_pull(&self) -> Iface {
        let mut o = self.out.lock().unwrap();
        let iface = o.len() as Iface;
        o.push(None);
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
        let o = self.out.lock().unwrap();
        for f in forwards {
            match &f {
                Forward::Flood { except, .. } => {
                    for (i, slot) in o.iter().enumerate() {
                        if i as Iface != *except {
                            if let Some(tx) = slot {
                                let _ = tx.send(f.clone());
                            }
                        }
                    }
                }
                Forward::Directed { iface, .. } => {
                    if let Some(Some(tx)) = o.get(*iface as usize) {
                        let _ = tx.send(f.clone());
                    }
                }
            }
        }
    }
}
