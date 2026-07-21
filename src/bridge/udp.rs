//! UDP/IPv4 broadcast bridge runner (message-pipe shape 1).

use super::hub::{now, Shared};
use super::Neighbors;
use crate::*;
use std::io::ErrorKind;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};
use std::sync::mpsc::Receiver;
use std::time::Duration;

/// Bind UDP :7373, LAN-broadcast floods, and unicast directed sends to nodes
/// learned by snooping. Blocks until an error or the process ends.
pub fn run(hub: Shared, iface: Iface, rx: Receiver<Forward>) -> std::io::Result<()> {
    let sock = UdpSocket::bind(("0.0.0.0", 7373))?;
    sock.set_broadcast(true)?;
    sock.set_read_timeout(Some(Duration::from_millis(200)))?;
    let bcast: SocketAddr = SocketAddrV4::new(Ipv4Addr::BROADCAST, 7373).into();
    let mut nbrs: Neighbors<SocketAddr> = Neighbors::new(2 * 3600);
    println!("  [udp] iface {iface} on :7373 (LAN broadcast)");

    let mut buf = [0u8; 2048];
    loop {
        match sock.recv_from(&mut buf) {
            Ok((n, peer)) => {
                let nbr = nbrs.snoop(&buf[..n], peer, now());
                hub.on_rx(iface, &buf[..n], nbr);
            }
            Err(e) if e.kind() == ErrorKind::WouldBlock || e.kind() == ErrorKind::TimedOut => {}
            Err(e) => return Err(e),
        }
        while let Ok(f) = rx.try_recv() {
            match f {
                Forward::Flood { bytes, .. } => {
                    sock.send_to(&bytes, bcast)?;
                }
                Forward::Directed { nbr, bytes, .. } => {
                    let dst = nbr.and_then(|a| nbrs.resolve(&a, now())).unwrap_or(bcast);
                    sock.send_to(&bytes, dst)?;
                }
            }
        }
    }
}
