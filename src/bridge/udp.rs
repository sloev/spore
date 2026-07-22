//! UDP/IPv4 broadcast bridge (message-pipe shape 1). The medium-specific part is
//! just `recv`/`send`; the shared logic lives in `driver::run_datagram`.

use super::driver::{run_datagram, DatagramTransport};
use super::hub::Shared;
use crate::{Forward, Iface};
use std::io::ErrorKind;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};
use std::sync::mpsc::Receiver;
use std::time::Duration;

struct Udp {
    sock: UdpSocket,
    bcast: SocketAddr,
}

impl DatagramTransport for Udp {
    type Addr = SocketAddr;

    fn recv(&mut self) -> std::io::Result<Option<(Vec<u8>, Option<SocketAddr>)>> {
        let mut buf = [0u8; 2048];
        match self.sock.recv_from(&mut buf) {
            Ok((n, peer)) => Ok(Some((buf[..n].to_vec(), Some(peer)))),
            Err(e) if e.kind() == ErrorKind::WouldBlock || e.kind() == ErrorKind::TimedOut => Ok(None),
            Err(e) => Err(e),
        }
    }

    fn send(&mut self, to: Option<&SocketAddr>, env: &[u8]) -> std::io::Result<()> {
        self.sock.send_to(env, to.copied().unwrap_or(self.bcast))?;
        Ok(())
    }
}

/// Bind UDP `port` and bridge the LAN: broadcast floods, unicast directed sends
/// to peers learned by snooping.
pub fn run(hub: Shared, iface: Iface, rx: Receiver<Forward>, port: u16) -> std::io::Result<()> {
    let sock = UdpSocket::bind(("0.0.0.0", port))?;
    sock.set_broadcast(true)?;
    sock.set_read_timeout(Some(Duration::from_millis(200)))?;
    let bcast: SocketAddr = SocketAddrV4::new(Ipv4Addr::BROADCAST, port).into();
    println!("  [udp] iface {iface} on :{port} (LAN broadcast)");
    run_datagram(hub, iface, rx, Udp { sock, bcast })
}
