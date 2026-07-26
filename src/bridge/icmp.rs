//! SPORE over **ping** — envelopes in the payload of ICMP echo packets.
//!
//! Every IP host answers ping, and almost every firewall passes it. That makes
//! ICMP echo a carrier that reaches places a new port never would: a captive
//! portal that blocks everything but lets ping through, a network that permits
//! only "diagnostics", a host you can reach but not connect to. The echo
//! request/reply payload is arbitrary bytes — traditionally a timestamp — so an
//! envelope rides there directly.
//!
//! This module is split deliberately:
//!
//! - The **codec** ([`encode_echo`], [`decode_echo`]) is pure, portable and
//!   tested: it builds and parses an ICMP echo message, checksum and all. It has
//!   no idea what a socket is.
//! - The **runner** ([`run`]) needs a raw socket, which needs `CAP_NET_RAW` (or
//!   root) and is Linux-only. It cannot be exercised in CI — there are no raw
//!   packets in a sandbox — so it is an honest template: the framing it puts on
//!   the wire is tested, the sending of it is not. See
//!   [`HARDWARE.md`](../../docs/HARDWARE.md).
//!
//! MTU is conservative: a SPORE envelope must fit one echo payload, and the core
//! fragments anything larger, so no IP fragmentation is relied on. Nothing here
//! is trusted — the envelope carries its own signature and sealing, so ICMP is
//! pure transport.

/// ICMPv4 echo **request** type. Replies are type 0.
pub const ECHO_REQUEST: u8 = 8;
/// ICMPv4 echo **reply** type.
pub const ECHO_REPLY: u8 = 0;

/// Keep an envelope inside one echo payload without leaning on IP fragmentation:
/// 1500-byte Ethernet MTU − 20 IP − 8 ICMP, rounded down.
pub const ICMP_MDU: usize = 1400;

/// The one-byte marker at the start of our payload, so a node ignores the
/// world's ordinary pings (timestamps, `ping` from a shell) and only ingests
/// envelopes another SPORE node sent. `S` ^ `P`.
pub const MAGIC: u8 = b'S' ^ b'P';

/// The Internet checksum (RFC 1071): one's-complement sum of 16-bit words.
///
/// The same routine validates and generates — a correct packet checksums to
/// zero — so there is one implementation and no way for the two directions to
/// disagree.
pub fn checksum(bytes: &[u8]) -> u16 {
    let mut sum: u32 = 0;
    let mut chunks = bytes.chunks_exact(2);
    for c in &mut chunks {
        sum += u16::from_be_bytes([c[0], c[1]]) as u32;
    }
    if let [last] = chunks.remainder() {
        sum += (*last as u32) << 8; // odd final byte is the high half
    }
    while sum >> 16 != 0 {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    !(sum as u16)
}

/// Build an ICMP echo message carrying `env`.
///
/// `reply` picks request (false) vs reply (true); a responder should answer with
/// a reply so the exchange looks like a normal ping to anything watching.
/// `ident`/`seq` are the echo identifier and sequence — set them to match a
/// request when replying, so middleboxes that track ping state stay happy.
pub fn encode_echo(env: &[u8], reply: bool, ident: u16, seq: u16) -> Vec<u8> {
    let mut p = Vec::with_capacity(9 + env.len());
    p.push(if reply { ECHO_REPLY } else { ECHO_REQUEST }); // type
    p.push(0); // code
    p.extend_from_slice(&[0, 0]); // checksum placeholder
    p.extend_from_slice(&ident.to_be_bytes());
    p.extend_from_slice(&seq.to_be_bytes());
    p.push(MAGIC); // our payload starts here
    p.extend_from_slice(env);
    let ck = checksum(&p);
    p[2..4].copy_from_slice(&ck.to_be_bytes());
    p
}

/// Parse an ICMP echo message and return the envelope it carried.
///
/// `None` unless it is a well-formed echo (checksum valid) that carries our
/// [`MAGIC`] — so ordinary pings from the rest of the world are skipped rather
/// than fed to the router as garbage. Accepts both request and reply, since a
/// node hears whichever its peer chose to send.
pub fn decode_echo(pkt: &[u8]) -> Option<Vec<u8>> {
    if pkt.len() < 9 {
        return None;
    }
    if pkt[0] != ECHO_REQUEST && pkt[0] != ECHO_REPLY {
        return None;
    }
    if pkt[1] != 0 {
        return None; // echo code is always 0
    }
    if checksum(pkt) != 0 {
        return None; // corrupt, or not really an echo
    }
    if pkt[8] != MAGIC {
        return None; // someone else's ping
    }
    Some(pkt[9..].to_vec())
}

/// Run SPORE over ICMP echo on a raw socket. **Linux, `CAP_NET_RAW` (or root).**
///
/// A template, not a CI-tested runner: raw packets cannot be exercised in a
/// sandbox. The [codec](encode_echo) it depends on *is* tested, so what this adds
/// on top is only the socket plumbing — kept small on purpose.
///
/// `peer` is the IPv4 address to send echoes to (a specific host, or a broadcast
/// / multicast address for a LAN). Grant the capability without running as root:
/// `sudo setcap cap_net_raw+ep ./spore`.
#[cfg(target_os = "linux")]
pub fn run(
    hub: crate::bridge::hub::Shared,
    iface: crate::Iface,
    rx: std::sync::mpsc::Receiver<crate::Forward>,
    peer: &str,
) -> std::io::Result<()> {
    use std::net::Ipv4Addr;
    use std::os::fd::FromRawFd;

    let dst: Ipv4Addr =
        peer.parse().map_err(|_| std::io::Error::other(format!("icmp: bad peer {peer:?}")))?;
    hub.with_node(|n| n.mtu = n.mtu.min(ICMP_MDU));

    // SOCK_RAW + IPPROTO_ICMP: the kernel fills the IP header on send and hands
    // us the IP header on receive (hence the 20-byte skip below).
    let fd = unsafe { libc::socket(libc::AF_INET, libc::SOCK_RAW, libc::IPPROTO_ICMP) };
    if fd < 0 {
        return Err(std::io::Error::last_os_error()); // usually "operation not permitted"
    }
    // Wrap the fd so it is closed on every return path.
    let sock = unsafe { std::net::UdpSocket::from_raw_fd(fd) };
    sock.set_read_timeout(Some(std::time::Duration::from_millis(200)))?;

    let dst_sa: std::net::SocketAddr = (dst, 0).into();
    let ident = (std::process::id() & 0xffff) as u16;
    let mut seq: u16 = 0;

    println!("  [icmp] iface {iface} echoing to {dst} (raw socket)");
    loop {
        // Receive: strip the IP header the kernel prepends, then decode.
        let mut buf = [0u8; 2048];
        if let Ok((n, _)) = sock.recv_from(&mut buf) {
            if n > 20 {
                if let Some(env) = decode_echo(&buf[20..n]) {
                    hub.on_rx(iface, &env, None);
                }
            }
        }
        // Send whatever the router queued, each as one echo request.
        while let Ok(f) = rx.try_recv() {
            let (crate::Forward::Flood { bytes, .. } | crate::Forward::Directed { bytes, .. }) = f;
            seq = seq.wrapping_add(1);
            let pkt = encode_echo(&bytes, false, ident, seq);
            let _ = sock.send_to(&pkt, dst_sa);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_real_echo_checksums_to_zero() {
        // RFC 1071: a valid message, checksum field included, sums to zero.
        let pkt = encode_echo(b"the dam holds", false, 0x1234, 7);
        assert_eq!(checksum(&pkt), 0, "a correctly built echo must self-verify");
    }

    #[test]
    fn an_envelope_survives_the_round_trip_both_ways() {
        for reply in [false, true] {
            let env = b"meet at the north pier";
            let pkt = encode_echo(env, reply, 42, 9);
            assert_eq!(pkt[0], if reply { ECHO_REPLY } else { ECHO_REQUEST });
            assert_eq!(decode_echo(&pkt).as_deref(), Some(&env[..]), "reply={reply}");
        }
    }

    #[test]
    fn the_world_ordinary_pings_are_ignored() {
        // A normal ping: valid echo, but no SPORE magic — must not reach the router.
        let mut ping = vec![ECHO_REQUEST, 0, 0, 0, 0x00, 0x01, 0x00, 0x01];
        ping.extend_from_slice(b"abcdefghijklmnop"); // a shell ping's timestamp pad
        let ck = checksum(&ping);
        ping[2..4].copy_from_slice(&ck.to_be_bytes());
        assert_eq!(checksum(&ping), 0, "our test ping is itself valid");
        assert_eq!(decode_echo(&ping), None, "but it carries no envelope");
    }

    #[test]
    fn a_corrupt_echo_is_rejected() {
        let mut pkt = encode_echo(b"burn the ledgers", false, 1, 1);
        pkt[10] ^= 0xff; // flip a payload byte, leave the checksum stale
        assert_eq!(decode_echo(&pkt), None, "a bad checksum must not decode");
    }

    #[test]
    fn an_odd_length_payload_checksums_correctly() {
        // The odd-final-byte path in the checksum is easy to get wrong.
        for len in [0, 1, 2, 3, 15, 16, 17] {
            let env = vec![0xA5u8; len];
            let pkt = encode_echo(&env, false, 7, 7);
            assert_eq!(checksum(&pkt), 0, "len {len} must self-verify");
            assert_eq!(decode_echo(&pkt).as_deref(), Some(&env[..]), "len {len} round-trip");
        }
    }
}
