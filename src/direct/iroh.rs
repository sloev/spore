//! Direct over an iroh QUIC connection — P-Direct-NAT step 5.
//!
//! The last rung, reached only when **both peers are on the public internet, both
//! are behind NATs, and no LAN, IPv6 or overlay path exists.** That is the tail of
//! what SPORE is for, not the trunk: a LAN needs nothing, a global IPv6 has no NAT
//! in front of it, and an overlay already routes. Leaning on a dependency for the
//! tail is cheap, which is the whole argument for using iroh here rather than
//! hand-rolling more traversal.
//!
//! iroh does the hole punching and, failing that, relays. Its connection *is* the
//! medium — its punched path cannot be extracted and reused, for the same reason a
//! punch must happen on the pipe's own socket — so it is wrapped as a
//! [`DatagramPort`] and handed to `Pipe` like any other.
//!
//! ## Four things this deliberately does
//!
//! **Datagrams, not streams.** `docs/DIRECT.md` avoids ordered delivery even on
//! TCP so a lost media frame never head-of-line-blocks. Running records over a
//! QUIC stream would reintroduce exactly that. `iroh::Connection::send_datagram`
//! is synchronous, so only the receive side needs pumping.
//!
//! **The record AEAD stays.** iroh's QUIC is already encrypted, so records are
//! sealed twice. That is the point, not waste: the SPDR key schedule binds both
//! SPORE addresses, the pipe id *and the medium name*, so dropping our own sealing
//! would make a pipe's security depend on its medium — and the threat model says
//! every link is hostile.
//!
//! **The iroh endpoint key is not the SPORE identity.** Reusing an Ed25519 signing
//! key as a TLS static key is cross-protocol reuse. Nothing needs it: the
//! candidate rides inside a sealed, signed OFFER, so the endpoint id is attested
//! by the SPORE identity exactly as an `ip:port` locator already is.
//!
//! **A relayed path is not "Direct" as this repo defines it.** DIRECT.md's
//! two-planes table says one hop, straight over an underlay; a relay is multi-hop
//! and sees ciphertext, metadata and timing. iroh reports which it got, so
//! [`IrohPort::is_relayed`] surfaces it rather than letting a relayed pipe pass as
//! a punched one — the same rule that made the plain-connect fallback loud.

use super::DatagramPort;
use iroh::endpoint::Connection;
use std::io;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::sync::Arc;
use tokio::runtime::Runtime;

/// The medium name for an iroh candidate. Convention, not a code — see
/// [`Medium`](super::Medium).
pub const MEDIUM: &str = "iroh";

/// A `DatagramPort` over an established iroh connection.
///
/// Holds the runtime alive: the receive pump and QUIC's own timers are tasks on
/// it, so dropping it would stall the connection rather than close it cleanly.
pub struct IrohPort {
    conn: Connection,
    rx: Receiver<Vec<u8>>,
    mtu: usize,
    _rt: Arc<Runtime>,
}

impl IrohPort {
    /// Wrap an established connection.
    ///
    /// `mtu` is taken from the connection when it will say, and clamped to
    /// `max_mtu` — the negotiated candidate's figure — because a record larger
    /// than the peer agreed to is not deliverable however much QUIC would carry.
    pub fn new(rt: Arc<Runtime>, conn: Connection, max_mtu: usize) -> IrohPort {
        // `max_datagram_size` is None when the peer disabled datagram support. A
        // conservative floor is better than refusing: QUIC will still carry a
        // small record, and the candidate's own mtu already bounds what the
        // application asked for.
        const FLOOR: usize = 1200;
        let mtu = conn.max_datagram_size().unwrap_or(FLOOR).min(max_mtu);

        let (tx, rx) = std::sync::mpsc::channel();
        let pump_conn = conn.clone();
        rt.spawn(async move {
            // Ends when the connection closes or the receiver is dropped, which
            // is what makes dropping an `IrohPort` tear the pump down too.
            while let Ok(bytes) = pump_conn.read_datagram().await {
                if tx.send(bytes.to_vec()).is_err() {
                    break;
                }
            }
        });

        IrohPort { conn, rx, mtu, _rt: rt }
    }

    /// The relay this connection is going through, if it is going through one.
    ///
    /// A relayed pipe works and is still end-to-end encrypted twice over, but it
    /// is **not one hop**, and the relay operator sees ciphertext, volume and
    /// timing. Naming the relay rather than returning a bare bool is deliberate:
    /// "relayed" and "relayed *via a host you did not choose*" are different
    /// disclosures, and only the second is a reason to stop.
    ///
    /// Anything that is not confirmed a direct IP path is reported as relayed —
    /// over-reporting a relay is an honest error, under-reporting it is not.
    pub fn relayed_via(&self) -> Option<String> {
        let paths = self.conn.paths();
        let mut relay = None;
        for p in paths.iter() {
            if p.is_ip() {
                // A direct path exists, so traffic is not obliged to go via a
                // relay even if one is also open. Direct wins the report.
                return None;
            }
            if p.is_relay() && relay.is_none() {
                relay = Some(format!("{p:?}"));
            }
        }
        relay
    }

    /// Whether this connection is going through a relay rather than a direct path.
    pub fn is_relayed(&self) -> bool {
        self.relayed_via().is_some()
    }

    /// The peer's iroh endpoint id — the locator a candidate carries.
    pub fn remote_id(&self) -> String {
        self.conn.remote_id().to_string()
    }
}

impl DatagramPort for IrohPort {
    fn mtu(&self) -> usize {
        self.mtu
    }

    fn send(&mut self, frame: &[u8]) -> io::Result<()> {
        self.conn
            .send_datagram(bytes::Bytes::copy_from_slice(frame))
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))
    }

    fn try_recv(&mut self) -> Option<Vec<u8>> {
        match self.rx.try_recv() {
            Ok(b) => Some(b),
            // Disconnected means the pump ended — the connection is gone. There
            // is no error channel on this trait, and there does not need to be:
            // a dead link stops producing records, and the pipe above is
            // best-effort by design.
            Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => None,
        }
    }
}
