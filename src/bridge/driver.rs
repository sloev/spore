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
use crate::{Forward, Iface};
use std::sync::mpsc::Receiver;

/// One received frame: the envelope bytes and the underlay address it came from
/// (`None` if the medium can't tell). `Ok(None)` means nothing was available.
pub type Received<A> = std::io::Result<Option<(Vec<u8>, Option<A>)>>;

/// A thin, platform-specific datagram medium. `Addr` (`U`) is however the medium
/// names a peer: a `SocketAddr`, a 6-byte MAC, a Meshtastic `u32`, `()` for
/// broadcast-only media, and so on.
pub trait DatagramTransport {
    /// The underlay address type `U`.
    type Addr: Clone + PartialEq;

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
/// medium. Blocks until an error or the process ends.
pub fn run_datagram<T: DatagramTransport>(
    hub: Shared,
    iface: Iface,
    rx: Receiver<Forward>,
    mut t: T,
) -> std::io::Result<()> {
    if let Some(m) = t.mtu() {
        hub.with_node(|n| n.mtu = n.mtu.min(m));
    }
    let mut nbrs: Neighbors<T::Addr> = Neighbors::new(2 * 3600);
    loop {
        // Receive: learn the sender (ARP snoop) and hand the frame to the node.
        if let Some((env, from)) = t.recv()? {
            let nbr = match from {
                Some(u) => nbrs.snoop(&env, u, now()),
                None => None,
            };
            hub.on_rx(iface, &env, nbr);
        }
        // Transmit: resolve directed sends to an underlay address, else broadcast.
        while let Ok(f) = rx.try_recv() {
            match f {
                Forward::Flood { bytes, .. } => t.send(None, &bytes)?,
                Forward::Directed { nbr, bytes, .. } => {
                    let u = nbr.and_then(|a| nbrs.resolve(&a, now()));
                    t.send(u.as_ref(), &bytes)?;
                }
            }
        }
    }
}
