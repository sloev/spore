//! The generic datagram bridge driver — where all the medium-independent logic
//! lives, so a new transport is a thin shim.
//!
//! Most media in the bridge matrix are "datagram" shaped: receive a frame from
//! some underlay address `U`, and send a frame to a `U` (or broadcast). A
//! transport for such a medium implements [`DatagramTransport`] — just `recv`
//! and `send` — and [`run_datagram`] handles everything else: neighbour learning
//! and resolution (`Neighbors<U>`), relaying to the shared node via the hub, MTU
//! clamping, and the broadcast fallback when a destination isn't known yet.
//!
//! The two non-datagram forms (a byte *stream* like TCP, and a shared *store*
//! like a folder) keep their own small runners.

use super::hub::{now, Shared};
use super::Neighbors;
use crate::linkfrag;
use crate::{Forward, Iface};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;

/// One received frame: the envelope bytes and the underlay address it came from
/// (`None` if the medium can't tell). `Ok(None)` means nothing was available.
pub type Received<A> = std::io::Result<Option<(Vec<u8>, Option<A>)>>;

/// A thin, platform-specific datagram medium. `Addr` (`U`) is however the medium
/// names a peer: a `SocketAddr`, a 6-byte MAC, a Meshtastic `u32`, `()` for
/// broadcast-only media, and so on.
pub trait DatagramTransport {
    /// The underlay address type `U`.
    ///
    /// `Debug` is required only so the driver can derive a stable reassembly key
    /// from an address whose shape it does not know — a `SocketAddr`, a callsign,
    /// a DevAddr, `()`. It never needs to be reversible, only to separate peers.
    type Addr: Clone + PartialEq + std::fmt::Debug;

    /// Poll for one inbound SPORE envelope and the underlay address it came from
    /// (`None` if the medium can't tell — e.g. broadcast-only audio). Return
    /// `Ok(None)` on timeout / nothing available. Any medium-specific framing or
    /// decoding (protobuf, KISS, …) happens here.
    fn recv(&mut self) -> Received<Self::Addr>;

    /// Transmit an envelope. `to == None` means broadcast to the whole medium;
    /// `Some(u)` means unicast to that underlay address. Medium-specific framing
    /// happens here.
    fn send(&mut self, to: Option<&Self::Addr>, env: &[u8]) -> std::io::Result<()>;

    /// This medium's payload budget, if smaller than the default — the driver
    /// clamps the shared node's MTU so SPORE auto-fragments to fit.
    fn mtu(&self) -> Option<usize> {
        None
    }
}

/// Run a datagram transport as a bridge: the whole shared loop, generic over the
/// medium. Blocks until an error, `stop` is set, or the process ends.
///
/// `stop` is checked once per iteration, right after the read that already
/// bounds it to `t`'s own poll interval (every medium here sets a short read
/// timeout) — so a caller that wants this bridge stoppable just needs a
/// transport that doesn't block indefinitely, which they all already are.
/// A stable reassembly key from an underlay address of whatever shape this
/// medium uses. Only needs to separate peers, not to be reversible.
fn hash_addr<A: std::fmt::Debug>(a: &A) -> u32 {
    let s = format!("{a:?}");
    let mut h: u32 = 2166136261;
    for b in s.as_bytes() {
        h ^= *b as u32;
        h = h.wrapping_mul(16777619);
    }
    h
}

pub fn run_datagram<T: DatagramTransport>(
    hub: Shared,
    iface: Iface,
    rx: Receiver<Forward>,
    stop: &AtomicBool,
    mut t: T,
) -> std::io::Result<()> {
    // No MTU clamp. This used to be `n.mtu = n.mtu.min(m)`, dragging the whole
    // node down to its narrowest link so it pre-fragmented small enough for the
    // worst one it owned. That only ever helped a node that *held* the narrow
    // link — nothing helped a node three hops upstream whose frames had to cross
    // someone else's LoRa hop, and the clamp was `min`, so detaching the bridge
    // never raised it back. Splitting here instead means each link uses its own
    // MTU and the node never has to know.
    let link_mtu = t.mtu();
    let mut frag = linkfrag::Reassembler::default();
    let mut set_id: u16 = 0;
    let mut nbrs: Neighbors<T::Addr> = Neighbors::new(2 * 3600);
    loop {
        if stop.load(Ordering::Relaxed) {
            return Ok(());
        }
        // Receive: learn the sender (ARP snoop) and hand the frame to the node.
        if let Some((env, from)) = t.recv()? {
            // Reassemble before the node sees anything. A fragment is link
            // framing, not an envelope, and it never travels further than this
            // link — so `snoop` and the router are only ever shown a whole one.
            //
            // The key is the underlay address where the medium has one, and the
            // interface where it does not: on a broadcast-only medium two
            // senders genuinely can collide on a set id, which is why a
            // mis-reassembly is dropped rather than parsed.
            let key = from.as_ref().map(hash_addr).unwrap_or(iface as u32);
            let Some(whole) = frag.accept(key, &env, now()) else { continue };
            let nbr = match from {
                Some(u) => nbrs.snoop(&whole, u, now()),
                None => None,
            };
            hub.on_rx(iface, &whole, nbr);
        }
        // Transmit: resolve directed sends to an underlay address, else broadcast.
        while let Ok(f) = rx.try_recv() {
            let (to, bytes) = match f {
                Forward::Flood { bytes, .. } => (None, bytes),
                Forward::Directed { nbr, bytes, .. } => (nbr.and_then(|a| nbrs.resolve(&a, now())), bytes),
            };
            match link_mtu {
                // A frame this link cannot carry is split for this link only.
                // `split` returns it in one piece when it already fits, so a
                // wide link pays nothing for this being here.
                Some(m) if bytes.len() > m => {
                    set_id = set_id.wrapping_add(1);
                    for piece in linkfrag::split_for_link(&bytes, m, set_id) {
                        t.send(to.as_ref(), &piece)?;
                    }
                }
                _ => t.send(to.as_ref(), &bytes)?,
            }
        }
    }
}
