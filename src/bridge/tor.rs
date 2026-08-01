//! SPORE over Tor: dial a `.onion` through the local SOCKS proxy.
//!
//! An onion service gives a location-hidden rendezvous with no public IP and no
//! port forwarding — so two nodes behind NAT, on mobile networks, or in places
//! where being reachable is itself a risk can still meet. Tor does the hard
//! part; this only has to speak SOCKS5 to it.
//!
//! **Two directions, and only one needs code.**
//!
//! - *Dialling out* is here: SOCKS5 to `127.0.0.1:9050` (the Tor daemon; Tor
//!   Browser uses 9150), `CONNECT <name>.onion:7373`, then the ordinary KISS
//!   stream. Tor resolves the onion itself, which is why SOCKS5's *domain*
//!   address form matters — a `.onion` has no IP to look up.
//! - *Being reachable* needs no code at all: point an onion service at the
//!   existing TCP bridge and Tor forwards to it.
//!
//!   ```text
//!   # torrc
//!   HiddenServiceDir /var/lib/tor/spore/
//!   HiddenServicePort 7373 127.0.0.1:7373
//!   ```
//!
//!   Then run `spore tcp` (listening) beside it and hand out the hostname Tor
//!   writes to that directory.
//!
//! Onion addressing is *already* what SPORE does — a v3 address is a public key,
//! so the name is the identity, exactly like a SPORE address being the hash of
//! one. Nothing here trusts the link: envelopes are signed and sealed whether or
//! not Tor is underneath.

use super::hub::Shared;
use crate::*;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::mpsc::Receiver;
use std::time::Duration;

/// Where the Tor daemon's SOCKS port normally sits. Tor Browser uses 9150.
pub const DEFAULT_SOCKS: &str = "127.0.0.1:9050";

/// Complete a SOCKS5 handshake and CONNECT to `host:port` through `proxy`.
///
/// No authentication (a local Tor daemon wants none) and the **domain** address
/// form, so the name is resolved at the far end — mandatory for `.onion`, which
/// means nothing to a local resolver.
pub fn socks5_connect(proxy: &str, host: &str, port: u16) -> std::io::Result<TcpStream> {
    fn bad(msg: &str) -> std::io::Error {
        std::io::Error::other(msg)
    }
    let name = host.as_bytes();
    if name.len() > 255 {
        return Err(bad("SOCKS5 hostname longer than 255 bytes"));
    }

    let mut s = TcpStream::connect(proxy)?;
    s.set_read_timeout(Some(Duration::from_secs(30)))?; // onion circuits are slow

    // Greeting: version 5, one method, "no authentication".
    s.write_all(&[0x05, 0x01, 0x00])?;
    let mut hello = [0u8; 2];
    s.read_exact(&mut hello)?;
    if hello[0] != 0x05 || hello[1] != 0x00 {
        return Err(bad("SOCKS5 proxy refused the no-auth method"));
    }

    // CONNECT, address type 3 (domain name).
    let mut req = vec![0x05, 0x01, 0x00, 0x03, name.len() as u8];
    req.extend_from_slice(name);
    req.extend_from_slice(&port.to_be_bytes());
    s.write_all(&req)?;

    // Reply: ver, status, reserved, atyp, bound address, bound port.
    let mut head = [0u8; 4];
    s.read_exact(&mut head)?;
    if head[1] != 0x00 {
        return Err(bad(socks5_error(head[1])));
    }
    let skip = match head[3] {
        0x01 => 4,  // IPv4
        0x04 => 16, // IPv6
        0x03 => {
            let mut n = [0u8; 1];
            s.read_exact(&mut n)?;
            n[0] as usize
        }
        _ => return Err(bad("SOCKS5 reply used an unknown address type")),
    };
    let mut rest = vec![0u8; skip + 2]; // bound address + port, both ignored
    s.read_exact(&mut rest)?;
    Ok(s)
}

/// The SOCKS5 reply codes, in words — "connection refused" and "host
/// unreachable" mean very different things when debugging an onion.
fn socks5_error(code: u8) -> &'static str {
    match code {
        0x01 => "SOCKS5: general failure (is the onion online?)",
        0x02 => "SOCKS5: connection not allowed by ruleset",
        0x03 => "SOCKS5: network unreachable",
        0x04 => "SOCKS5: host unreachable (bad onion address?)",
        0x05 => "SOCKS5: connection refused (nothing listening on that port)",
        0x06 => "SOCKS5: TTL expired",
        0x07 => "SOCKS5: command not supported",
        0x08 => "SOCKS5: address type not supported",
        _ => "SOCKS5: unknown failure",
    }
}

/// Bridge to a peer's onion service.
///
/// `target` is `<host>.onion[:port]`, optionally prefixed `proxy=HOST:PORT,` to
/// point at a Tor daemon other than [`DEFAULT_SOCKS`].
pub fn run(hub: Shared, iface: Iface, rx: Receiver<Forward>, target: &str) -> std::io::Result<()> {
    let (proxy, dest) = match target.split_once(',') {
        Some((p, d)) if p.starts_with("proxy=") => (&p[6..], d.trim()),
        _ => (DEFAULT_SOCKS, target.trim()),
    };
    let (host, port) = match dest.rsplit_once(':') {
        Some((h, p)) => (h, p.parse().unwrap_or(7373)),
        None => (dest, 7373u16),
    };

    println!("  [tor] iface {iface} dialling {host}:{port} via {proxy}");
    let (proxy, host) = (proxy.to_string(), host.to_string());
    super::stream_link::run_reconnecting(
        hub,
        iface,
        rx,
        move || {
            let s = socks5_connect(&proxy, &host, port)?;
            s.set_read_timeout(Some(Duration::from_millis(200)))?;
            println!("  [tor] circuit up");
            Ok(s)
        },
        "tor",
        // CLI/daemon-only: no per-bridge stop control yet.
        &std::sync::atomic::AtomicBool::new(false),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    /// Stand in for Tor: accept one SOCKS5 handshake, check the client got the
    /// bytes right, then behave like the far end.
    fn fake_socks(reply_code: u8, atyp: u8) -> (String, std::thread::JoinHandle<Vec<u8>>) {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap().to_string();
        let h = std::thread::spawn(move || {
            let (mut c, _) = l.accept().unwrap();
            let mut greet = [0u8; 3];
            c.read_exact(&mut greet).unwrap();
            assert_eq!(greet, [0x05, 0x01, 0x00], "greeting must offer no-auth");
            c.write_all(&[0x05, 0x00]).unwrap();

            let mut head = [0u8; 5];
            c.read_exact(&mut head).unwrap();
            let mut name = vec![0u8; head[4] as usize];
            c.read_exact(&mut name).unwrap();
            let mut port = [0u8; 2];
            c.read_exact(&mut port).unwrap();

            let mut reply = vec![0x05, reply_code, 0x00, atyp];
            match atyp {
                0x01 => reply.extend_from_slice(&[127, 0, 0, 1]),
                0x03 => {
                    reply.push(3);
                    reply.extend_from_slice(b"abc");
                }
                _ => reply.extend_from_slice(&[0u8; 16]),
            }
            reply.extend_from_slice(&[0x1c, 0xcd]);
            c.write_all(&reply).unwrap();

            let mut req = head[..4].to_vec();
            req.push(head[4]);
            req.extend_from_slice(&name);
            req.extend_from_slice(&port);
            req
        });
        (addr, h)
    }

    #[test]
    fn a_dial_asks_the_proxy_to_resolve_the_onion_itself() {
        let (proxy, server) = fake_socks(0x00, 0x01);
        let onion = "expyuzz4wqqyqhjn.onion";
        socks5_connect(&proxy, onion, 7373).expect("connect");
        let req = server.join().unwrap();

        assert_eq!(&req[..4], &[0x05, 0x01, 0x00, 0x03], "CONNECT by domain name");
        assert_eq!(req[4] as usize, onion.len());
        assert_eq!(&req[5..5 + onion.len()], onion.as_bytes(), "the name goes over verbatim");
        assert_eq!(&req[5 + onion.len()..], &7373u16.to_be_bytes(), "port is big-endian");
    }

    #[test]
    fn a_refusal_is_reported_in_words_not_a_number() {
        let (proxy, server) = fake_socks(0x05, 0x01);
        let e = socks5_connect(&proxy, "nope.onion", 7373).unwrap_err();
        let _ = server.join();
        assert!(e.to_string().contains("refused"), "got: {e}");
    }

    /// The bound-address field is variable length; miscounting it desynchronises
    /// the stream and every later frame is garbage.
    #[test]
    fn every_bound_address_form_is_consumed_exactly() {
        for atyp in [0x01u8, 0x03, 0x04] {
            let (proxy, server) = fake_socks(0x00, atyp);
            let mut s =
                socks5_connect(&proxy, "x.onion", 7373).unwrap_or_else(|e| panic!("atyp {atyp:#x}: {e}"));
            let _ = server.join();
            // Nothing of the reply may be left over. Miscounting shows up as a
            // stray byte here; EOF or a timeout both mean we consumed it all.
            s.set_read_timeout(Some(Duration::from_millis(120))).unwrap();
            let mut spare = [0u8; 4];
            let left = s.read(&mut spare).unwrap_or(0);
            assert_eq!(left, 0, "atyp {atyp:#x} left {left} byte(s) of the reply unread");
        }
    }
}
