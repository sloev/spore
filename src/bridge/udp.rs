//! UDP/IPv4 bridge (message-pipe shape 1). The medium-specific part is just
//! `recv`/`send`; the shared logic lives in `driver::run_datagram`.
//!
//! Two runners:
//! - [`run`] — the limited broadcast `255.255.255.255:port` (never forwarded by
//!   routers, so it stays on the wire you're plugged into).
//! - [`run_primary`] — the **directed broadcast of your primary subnet** (e.g.
//!   `192.168.1.255`), auto-discovered from the OS, on [`SPORE_LAN_PORT`]. This
//!   is the "just works on this LAN" default: SPORE finds the network you're
//!   actually on and floods there, no address or port to configure.

use super::driver::{run_datagram, DatagramTransport};
use super::hub::Shared;
use crate::{Forward, Iface};
use std::io::ErrorKind;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};
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

/// Bind UDP `port` and bridge the LAN with the *limited* broadcast: floods go to
/// `255.255.255.255:port`, unicast to peers learned by snooping.
pub fn run(hub: Shared, iface: Iface, rx: Receiver<Forward>, port: u16) -> std::io::Result<()> {
    let sock = UdpSocket::bind(("0.0.0.0", port))?;
    sock.set_broadcast(true)?;
    sock.set_read_timeout(Some(Duration::from_millis(200)))?;
    let bcast: SocketAddr = SocketAddrV4::new(Ipv4Addr::BROADCAST, port).into();
    println!("  [udp] iface {iface} on :{port} (limited broadcast)");
    run_datagram(hub, iface, rx, Udp { sock, bcast })
}

/// The standardized SPORE LAN port. Nodes that speak `run_primary` agree on this
/// port so any two SPORE devices on the same subnet find each other with zero
/// configuration — plug in, and you're on the mesh.
pub const SPORE_LAN_PORT: u16 = 7373;

/// Bridge the LAN using the **primary subnet's directed broadcast**, discovered
/// from the OS (e.g. `192.168.1.255`), on [`SPORE_LAN_PORT`] (or `port`). This
/// is the zero-config default: it targets the exact network you're on rather
/// than the blunt `255.255.255.255`, so it reaches every host on your subnet and
/// plays nicely with multi-homed machines.
pub fn run_primary(
    hub: Shared,
    iface: Iface,
    rx: Receiver<Forward>,
    port: Option<u16>,
) -> std::io::Result<()> {
    let port = port.unwrap_or(SPORE_LAN_PORT);
    let sock = UdpSocket::bind(("0.0.0.0", port))?;
    sock.set_broadcast(true)?;
    sock.set_read_timeout(Some(Duration::from_millis(200)))?;
    let bcast_ip = primary_broadcast().unwrap_or(Ipv4Addr::BROADCAST);
    let bcast: SocketAddr = SocketAddrV4::new(bcast_ip, port).into();
    println!("  [udp] iface {iface} on :{port} (primary subnet broadcast {bcast_ip})");
    run_datagram(hub, iface, rx, Udp { sock, bcast })
}

/// Discover this host's primary local IPv4 by asking the OS which source address
/// it would route a packet to the internet from. Sends nothing — a *connected*
/// UDP socket just fixes the route so `local_addr` reports the source the kernel
/// picked.
pub fn primary_ipv4() -> Option<Ipv4Addr> {
    let probe = UdpSocket::bind(("0.0.0.0", 0)).ok()?;
    probe.connect(("192.0.2.1", 9)).ok()?; // TEST-NET-1: picks a source, routes nowhere
    match probe.local_addr().ok()?.ip() {
        IpAddr::V4(ip) if !ip.is_unspecified() && !ip.is_loopback() => Some(ip),
        _ => None,
    }
}

/// The directed broadcast address of the primary subnet, e.g. `192.168.1.255`.
///
/// On Linux the exact netmask comes from `/proc/net/route`; everywhere else (and
/// if that fails) we fall back to the interface's classful mask. `None` means
/// "couldn't tell — use the limited broadcast".
pub fn primary_broadcast() -> Option<Ipv4Addr> {
    let ip = primary_ipv4()?;
    let mask = netmask_for(ip).unwrap_or_else(|| default_mask(ip));
    Some(Ipv4Addr::from(u32::from(ip) | !u32::from(mask)))
}

// Parse the netmask for `ip` from Linux's routing table. Each `/proc/net/route`
// line holds Destination and Mask as little-endian hex; the subnet route is the
// longest-prefix one whose (ip & mask) equals its destination network.
fn netmask_for(ip: Ipv4Addr) -> Option<Ipv4Addr> {
    let text = std::fs::read_to_string("/proc/net/route").ok()?;
    let ip_be = u32::from(ip);
    let mut best: Option<(u32, u32)> = None; // (mask_be, prefix_len)
    for line in text.lines().skip(1) {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() < 8 {
            continue;
        }
        let dest = hex_le_to_be(cols[1])?;
        let mask = hex_le_to_be(cols[7])?;
        if mask == 0 {
            continue; // default route — no subnet info
        }
        if (ip_be & mask) == dest {
            let plen = mask.count_ones();
            if best.is_none_or(|(_, bp)| plen > bp) {
                best = Some((mask, plen));
            }
        }
    }
    best.map(|(m, _)| Ipv4Addr::from(m))
}

// `/proc/net/route` stores addresses little-endian: "0001A8C0" is 192.168.1.0.
fn hex_le_to_be(hex: &str) -> Option<u32> {
    let v = u32::from_str_radix(hex, 16).ok()?;
    Some(u32::from(Ipv4Addr::from(v.to_le_bytes())))
}

// Classful fallback when we can't read a real netmask.
fn default_mask(ip: Ipv4Addr) -> Ipv4Addr {
    match ip.octets()[0] {
        0..=127 => Ipv4Addr::new(255, 0, 0, 0),
        128..=191 => Ipv4Addr::new(255, 255, 0, 0),
        _ => Ipv4Addr::new(255, 255, 255, 0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn broadcast_from_ip_and_mask() {
        let ip = Ipv4Addr::new(192, 168, 1, 42);
        let m = u32::from(Ipv4Addr::new(255, 255, 255, 0));
        assert_eq!(Ipv4Addr::from(u32::from(ip) | !m), Ipv4Addr::new(192, 168, 1, 255));
        let ip = Ipv4Addr::new(172, 20, 5, 9);
        let m = u32::from(Ipv4Addr::new(255, 255, 0, 0));
        assert_eq!(Ipv4Addr::from(u32::from(ip) | !m), Ipv4Addr::new(172, 20, 255, 255));
    }

    #[test]
    fn proc_route_hex_is_little_endian() {
        assert_eq!(hex_le_to_be("0001A8C0").map(Ipv4Addr::from), Some(Ipv4Addr::new(192, 168, 1, 0)));
        assert_eq!(hex_le_to_be("00FFFFFF").map(Ipv4Addr::from), Some(Ipv4Addr::new(255, 255, 255, 0)));
    }

    #[test]
    fn default_masks_are_classful() {
        assert_eq!(default_mask(Ipv4Addr::new(10, 0, 0, 1)), Ipv4Addr::new(255, 0, 0, 0));
        assert_eq!(default_mask(Ipv4Addr::new(172, 16, 0, 1)), Ipv4Addr::new(255, 255, 0, 0));
        assert_eq!(default_mask(Ipv4Addr::new(192, 168, 0, 1)), Ipv4Addr::new(255, 255, 255, 0));
    }
}
