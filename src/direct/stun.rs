//! A minimal STUN binding client and echo — one packet out, one packet back.
//!
//! P-Direct-NAT step 2. Before this, a node could only offer the address it was
//! *told* it had, so Direct worked on a LAN and nowhere else. A reflexive locator
//! is what a peer on the far side of a NAT would actually have to dial.
//!
//! Deliberately not an ICE stack. `docs/BRIDGES.md` records that a native Rust
//! ICE/DTLS/SCTP stack was considered and declined as the largest dependency this
//! project would ever have taken; a binding request is one datagram with a
//! 20-byte header and one attribute to read back, which is a different order of
//! thing entirely — no agents, no candidate pairs, no connectivity checks, no
//! dependency.
//!
//! **The echo half matters as much as the client half.** A node answers binding
//! requests itself, statelessly: one packet in, one out, no payload, nothing
//! retained. That is what keeps SPORE from quietly depending on a third party's
//! STUN server for something the protocol can do for itself — every SPORE daemon
//! is a reflexive-locator server for every other one.
//!
//! Implements the subset of RFC 5389 that a binding exchange needs, and nothing
//! else: no authentication, no FINGERPRINT, no ALTERNATE-SERVER. Unknown
//! attributes are skipped rather than rejected, so a full STUN server's extra
//! attributes are simply ignored and interoperation still works.

use std::io;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

const BINDING_REQUEST: u16 = 0x0001;
const BINDING_SUCCESS: u16 = 0x0101;
const MAGIC_COOKIE: u32 = 0x2112_A442;
const ATTR_XOR_MAPPED_ADDRESS: u16 = 0x0020;
const HEADER: usize = 20;

/// A binding request's transaction id — echoed by the server, and what tells our
/// answer apart from a stray datagram that happened to arrive on this socket.
pub type Txn = [u8; 12];

fn be16(b: &[u8]) -> u16 {
    u16::from_be_bytes([b[0], b[1]])
}

/// Build a binding request and the transaction id to match its response against.
pub fn request() -> (Vec<u8>, Txn) {
    let mut txn = [0u8; 12];
    rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut txn);
    (request_with(&txn), txn)
}

fn request_with(txn: &Txn) -> Vec<u8> {
    let mut v = Vec::with_capacity(HEADER);
    v.extend_from_slice(&BINDING_REQUEST.to_be_bytes());
    v.extend_from_slice(&0u16.to_be_bytes()); // no attributes
    v.extend_from_slice(&MAGIC_COOKIE.to_be_bytes());
    v.extend_from_slice(txn);
    v
}

/// Is this a STUN binding request? Used to tell one from a Direct record when
/// both can arrive on the same socket.
pub fn is_request(buf: &[u8]) -> bool {
    buf.len() >= HEADER && be16(buf) == BINDING_REQUEST && buf[4..8] == MAGIC_COOKIE.to_be_bytes()
}

/// Build the success response telling `from` where we saw it.
///
/// `None` if `buf` is not a binding request. Stateless by construction: every
/// byte of the reply comes from the request and the observed source address, so
/// there is nothing to remember and nothing to exhaust.
pub fn answer(buf: &[u8], from: SocketAddr) -> Option<Vec<u8>> {
    if !is_request(buf) {
        return None;
    }
    let txn: Txn = buf[8..20].try_into().ok()?;
    let attr = xor_mapped_address(from, &txn);
    let mut v = Vec::with_capacity(HEADER + attr.len());
    v.extend_from_slice(&BINDING_SUCCESS.to_be_bytes());
    v.extend_from_slice(&(attr.len() as u16).to_be_bytes());
    v.extend_from_slice(&MAGIC_COOKIE.to_be_bytes());
    v.extend_from_slice(&txn);
    v.extend_from_slice(&attr);
    Some(v)
}

fn xor_mapped_address(addr: SocketAddr, txn: &Txn) -> Vec<u8> {
    let xport = addr.port() ^ (MAGIC_COOKIE >> 16) as u16;
    let mut body = vec![0u8, 0, 0, 0];
    body[2..4].copy_from_slice(&xport.to_be_bytes());
    match addr.ip() {
        IpAddr::V4(v4) => {
            body[1] = 0x01;
            let x = u32::from(v4) ^ MAGIC_COOKIE;
            body.extend_from_slice(&x.to_be_bytes());
        }
        IpAddr::V6(v6) => {
            body[1] = 0x02;
            // IPv6 is XORed with the cookie *concatenated with* the transaction
            // id, so the mask is 16 bytes rather than 4.
            let mut mask = [0u8; 16];
            mask[..4].copy_from_slice(&MAGIC_COOKIE.to_be_bytes());
            mask[4..].copy_from_slice(txn);
            let o = v6.octets();
            body.extend((0..16).map(|i| o[i] ^ mask[i]));
        }
    }
    let mut v = Vec::with_capacity(4 + body.len());
    v.extend_from_slice(&ATTR_XOR_MAPPED_ADDRESS.to_be_bytes());
    v.extend_from_slice(&(body.len() as u16).to_be_bytes());
    v.extend_from_slice(&body);
    v
}

/// Read the reflexive address out of a binding success response.
///
/// `None` unless it is a success for *this* transaction and carries an address we
/// understand — a mismatched transaction id is someone else's answer, or a stray
/// packet, and either way is not ours to believe.
pub fn parse_response(buf: &[u8], txn: &Txn) -> Option<SocketAddr> {
    if buf.len() < HEADER || be16(buf) != BINDING_SUCCESS {
        return None;
    }
    if buf[4..8] != MAGIC_COOKIE.to_be_bytes() || &buf[8..20] != txn {
        return None;
    }
    let len = be16(&buf[2..4]) as usize;
    let attrs = buf.get(HEADER..HEADER + len)?;
    let mut i = 0;
    while i + 4 <= attrs.len() {
        let typ = be16(&attrs[i..]);
        let alen = be16(&attrs[i + 2..]) as usize;
        let body = attrs.get(i + 4..i + 4 + alen)?;
        if typ == ATTR_XOR_MAPPED_ADDRESS {
            return decode_xor_mapped(body, txn);
        }
        // Attributes are padded to a 4-byte boundary. Anything we do not know is
        // skipped, not rejected: a fuller STUN server sends attributes this
        // subset has no use for, and refusing them would fail for no reason.
        i += 4 + alen.div_ceil(4) * 4;
    }
    None
}

fn decode_xor_mapped(body: &[u8], txn: &Txn) -> Option<SocketAddr> {
    if body.len() < 4 {
        return None;
    }
    let port = be16(&body[2..4]) ^ (MAGIC_COOKIE >> 16) as u16;
    match body[1] {
        0x01 => {
            let x = u32::from_be_bytes(body.get(4..8)?.try_into().ok()?);
            Some(SocketAddr::new(IpAddr::V4(Ipv4Addr::from(x ^ MAGIC_COOKIE)), port))
        }
        0x02 => {
            let raw: [u8; 16] = body.get(4..20)?.try_into().ok()?;
            let mut mask = [0u8; 16];
            mask[..4].copy_from_slice(&MAGIC_COOKIE.to_be_bytes());
            mask[4..].copy_from_slice(txn);
            let mut o = [0u8; 16];
            for i in 0..16 {
                o[i] = raw[i] ^ mask[i];
            }
            Some(SocketAddr::new(IpAddr::V6(Ipv6Addr::from(o)), port))
        }
        _ => None,
    }
}

/// Ask `server` where this socket appears to be, from this socket.
///
/// The request goes out on the *same* socket the pipe will use, which is the
/// whole point: a NAT's mapping belongs to a source port, so a reflexive address
/// discovered on another socket describes a binding the peer cannot reach.
///
/// Retries a couple of times — one lost datagram on a path this is being used to
/// characterise is unremarkable — then gives up rather than blocking a caller.
pub fn reflexive(
    sock: &UdpSocket,
    server: impl std::net::ToSocketAddrs,
    wait: Duration,
) -> io::Result<SocketAddr> {
    let addr = server
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no such STUN server"))?;
    let prior = sock.read_timeout()?;
    sock.set_read_timeout(Some(wait))?;
    let out = (|| {
        for _ in 0..3 {
            let (req, txn) = request();
            sock.send_to(&req, addr)?;
            let deadline = Instant::now() + wait;
            let mut buf = [0u8; 512];
            while Instant::now() < deadline {
                match sock.recv_from(&mut buf) {
                    // Anything that is not our answer is left alone: this socket
                    // may already be carrying pipe traffic.
                    Ok((n, from)) if from == addr => {
                        if let Some(a) = parse_response(&buf[..n], &txn) {
                            return Ok(a);
                        }
                    }
                    Ok(_) => continue,
                    Err(e)
                        if e.kind() == io::ErrorKind::WouldBlock || e.kind() == io::ErrorKind::TimedOut =>
                    {
                        break
                    }
                    Err(e) => return Err(e),
                }
            }
        }
        Err(io::Error::new(io::ErrorKind::TimedOut, "no STUN response"))
    })();
    sock.set_read_timeout(prior)?;
    out
}

/// Answer one binding request if that is what arrived. `true` if it was handled.
///
/// A node calls this on any datagram it did not recognise, which is what makes
/// every SPORE node a reflexive-locator server for every other one — no separate
/// port, no separate service, nothing kept between packets.
pub fn serve_one(sock: &UdpSocket, buf: &[u8], from: SocketAddr) -> bool {
    match answer(buf, from) {
        Some(reply) => sock.send_to(&reply, from).is_ok(),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_binding_exchange_round_trips_through_two_real_sockets() {
        let server = UdpSocket::bind("127.0.0.1:0").unwrap();
        let server_addr = server.local_addr().unwrap();
        let client = UdpSocket::bind("127.0.0.1:0").unwrap();
        let client_addr = client.local_addr().unwrap();

        // The echo is stateless, so it is just a thread that answers whatever
        // arrives — exactly what a daemon does inline.
        std::thread::spawn(move || {
            let mut buf = [0u8; 512];
            let (n, from) = server.recv_from(&mut buf).unwrap();
            assert!(is_request(&buf[..n]), "the client sent a binding request");
            serve_one(&server, &buf[..n], from);
        });

        let seen = reflexive(&client, server_addr, Duration::from_secs(2)).unwrap();
        assert_eq!(seen, client_addr, "the server reports the port it actually saw");
    }

    #[test]
    fn a_response_for_another_transaction_is_not_believed() {
        let (_, txn) = request();
        let mut other = txn;
        other[0] ^= 0xff;
        let reply = answer(&request_with(&other), "203.0.113.9:1234".parse().unwrap()).unwrap();
        assert!(parse_response(&reply, &other).is_some(), "it is a valid answer for *its* transaction");
        assert_eq!(parse_response(&reply, &txn), None, "but not for ours");
    }

    #[test]
    fn unknown_attributes_are_skipped_not_rejected() {
        let txn = [7u8; 12];
        let addr: SocketAddr = "198.51.100.4:9999".parse().unwrap();
        let xor = xor_mapped_address(addr, &txn);

        // A SOFTWARE-like attribute of a length that needs padding, ahead of the
        // one we want — a real server sends things this subset has no use for.
        let filler = b"spore-test";
        let mut attrs = Vec::new();
        attrs.extend_from_slice(&0x8022u16.to_be_bytes());
        attrs.extend_from_slice(&(filler.len() as u16).to_be_bytes());
        attrs.extend_from_slice(filler);
        attrs.resize(attrs.len() + (4 - filler.len() % 4) % 4, 0); // pad to 4
        attrs.extend_from_slice(&xor);

        let mut msg = Vec::new();
        msg.extend_from_slice(&BINDING_SUCCESS.to_be_bytes());
        msg.extend_from_slice(&(attrs.len() as u16).to_be_bytes());
        msg.extend_from_slice(&MAGIC_COOKIE.to_be_bytes());
        msg.extend_from_slice(&txn);
        msg.extend_from_slice(&attrs);

        assert_eq!(parse_response(&msg, &txn), Some(addr));
    }

    #[test]
    fn ipv6_is_xored_with_the_cookie_and_the_transaction_id() {
        let txn = [0x5au8; 12];
        let addr: SocketAddr = "[2001:db8::1]:4242".parse().unwrap();
        let attr = xor_mapped_address(addr, &txn);
        assert_eq!(decode_xor_mapped(&attr[4..], &txn), Some(addr));
        // Wrong transaction id, wrong address — which is why the mask includes it.
        let mut wrong = txn;
        wrong[3] ^= 0x01;
        assert_ne!(decode_xor_mapped(&attr[4..], &wrong), Some(addr));
    }

    #[test]
    fn a_short_or_alien_datagram_is_not_mistaken_for_stun() {
        assert!(!is_request(b"short"));
        assert!(!is_request(&[0u8; 20]), "zeroed 20 bytes has neither type nor cookie");
        assert_eq!(answer(b"not stun at all right here", "127.0.0.1:1".parse().unwrap()), None);
        let (req, txn) = request();
        assert_eq!(parse_response(&req, &txn), None, "a request is not a response");
    }
}
