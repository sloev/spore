//! Coordinated UDP hole-punching — P-Direct-NAT step 3.
//!
//! Step 2 let a node learn the address the outside world sees. That address is
//! still not reachable: a NAT drops an inbound datagram it has no mapping for,
//! and a mapping only exists once something has gone *out* to that peer. Both
//! ends sending at roughly the same time is what creates the pair of mappings —
//! each side's outbound probe opens the hole its peer's probe then arrives
//! through.
//!
//! **The punch happens on the socket the pipe will use.** That is the whole
//! constraint, and why this could not be bolted on later: a NAT mapping belongs
//! to a source port, so a hole punched on one socket is worthless to a pipe
//! running on another. [`punch`] therefore returns the socket it punched with,
//! connected to the peer, for [`UdpPort::from_socket`](super::UdpPort::from_socket).
//!
//! ## Timing, and why it is not read off the OFFER/ANSWER round trip
//!
//! Signalling rides the ordinary mesh, whose own docs say delivery is "seconds to
//! days", while a NAT UDP binding dies in 30s–5min. So the two sides cannot agree
//! on a moment to punch by exchanging one — by the time an ANSWER arrives over a
//! slow path, any instant named in it is long past.
//!
//! Instead each side punches when it *acts*: the responder as it answers, the
//! initiator as it finishes. Those are one mesh hop apart, so the windows overlap
//! exactly when signalling was fast — which is also exactly when the peer is
//! demonstrably live. When the mesh was slow the windows miss, the punch fails
//! inside its bound, and the pipe is not established. That is the honest outcome
//! rather than a pipe that appears up and drops every record.
//!
//! ## What a probe is, and is not
//!
//! Four magic bytes and the pipe id. It carries no payload and **is not
//! authenticated** — it cannot be, since it runs before the pipe has keys. It
//! decides one thing only: which address this socket then connects to. Everything
//! after is AEAD-sealed under the ephemeral DH from the sealed SPDR exchange, so
//! a spoofed probe cannot read, write or join a pipe; at worst it points a socket
//! at an address whose records then fail the MAC, and the pipe simply never
//! carries traffic. The pipe id is not a secret to rely on — it is a demultiplexer.

use std::io;
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::time::{Duration, Instant};

/// Probe magic — distinct from `SPDR` signalling and from a sealed record, so a
/// probe arriving on a live socket is recognisably not pipe traffic.
const PROBE: &[u8; 4] = b"SPPK";

/// How long to keep probing before giving up. Deliberately short: a punch that
/// has not landed in this long means the two windows did not overlap, and waiting
/// longer only holds up the caller.
pub const WINDOW: Duration = Duration::from_secs(2);

/// Gap between probes. Several go out because the first may be the one that
/// creates the mapping and is itself dropped by the peer's NAT.
const INTERVAL: Duration = Duration::from_millis(120);

/// Probes sent *after* the peer's has arrived. Receiving is not proof of being
/// received: our earlier probes may all have gone out before the peer's socket
/// existed. Without this tail one side succeeds while the other times out.
const TAIL_PROBES: usize = 4;

fn probe(pipe_id: &[u8; 16]) -> [u8; 20] {
    let mut p = [0u8; 20];
    p[..4].copy_from_slice(PROBE);
    p[4..].copy_from_slice(pipe_id);
    p
}

/// Is this datagram a punch probe for `pipe_id`?
pub fn is_probe(buf: &[u8], pipe_id: &[u8; 16]) -> bool {
    buf.len() >= 20 && &buf[..4] == PROBE && &buf[4..20] == pipe_id
}

/// Punch a hole to `peer` from `local_port`, and hand back the socket that did it.
///
/// Returns as soon as one of the peer's probes arrives — that is the proof the
/// mapping is open in the direction that matters. The returned socket is
/// *connected* to the peer and ready for
/// [`UdpPort::from_socket`](super::UdpPort::from_socket).
///
/// `Err` means the windows did not overlap or the path is genuinely blocked.
/// Blocking, bounded by `window`: the caller is a runtime's Direct loop, and the
/// bound is what keeps a failed punch from stalling it.
pub fn punch(
    local_port: u16,
    peer: impl ToSocketAddrs,
    pipe_id: &[u8; 16],
    window: Duration,
) -> io::Result<UdpSocket> {
    let peer: SocketAddr = peer
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no such peer"))?;
    let sock = UdpSocket::bind(("0.0.0.0", local_port))?;
    sock.set_read_timeout(Some(INTERVAL))?;

    let p = probe(pipe_id);
    let deadline = Instant::now() + window;
    let mut buf = [0u8; 256];
    while Instant::now() < deadline {
        // Send first, always: our outbound datagram is what opens our own NAT's
        // mapping, so it has to go even if nothing has arrived yet.
        let _ = sock.send_to(&p, peer);
        match sock.recv_from(&mut buf) {
            Ok((n, from)) if is_probe(&buf[..n], pipe_id) => {
                // Accept the address it actually came from, not the one we
                // predicted: a symmetric NAT may have rewritten the peer's port
                // since it was learned, and the address that just reached us is
                // by construction the one that works.
                sock.connect(from)?;
                // Hearing them does not mean they have heard us. Our earlier
                // probes may all have arrived before their socket existed, and if
                // we stop the instant we succeed, their punch times out while ours
                // reports success — a half-open pipe that looks up on one end
                // only. So send a short tail before returning.
                for _ in 0..TAIL_PROBES {
                    let _ = sock.send(&p);
                    std::thread::sleep(INTERVAL / 2);
                }
                sock.set_read_timeout(None)?;
                return Ok(sock);
            }
            // Anything else on this socket is not ours to interpret yet.
            Ok(_) => continue,
            // Every error here is transient by construction, so none of them ends
            // the attempt — only the deadline does. In particular a probe sent
            // before the peer's socket exists draws an ICMP port-unreachable,
            // which Linux reports as `ConnectionRefused` on the *next* recv; that
            // is the normal opening move of a punch, not a failure. Treating it
            // as fatal made the whole thing lose a race it should have won.
            Err(_) => continue,
        }
    }
    Err(io::Error::new(io::ErrorKind::TimedOut, "hole punch did not land"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn two_sides_punching_at_once_meet_in_the_middle() {
        let pipe_id = [9u8; 16];
        // Pick two free ports by binding and immediately dropping — the punch
        // rebinds them itself, which is the behaviour under test.
        let a_port = UdpSocket::bind("0.0.0.0:0").unwrap().local_addr().unwrap().port();
        let b_port = UdpSocket::bind("0.0.0.0:0").unwrap().local_addr().unwrap().port();

        let a = thread::spawn(move || punch(a_port, ("127.0.0.1", b_port), &pipe_id, Duration::from_secs(8)));
        let b = thread::spawn(move || punch(b_port, ("127.0.0.1", a_port), &pipe_id, Duration::from_secs(8)));
        let a_sock = a.join().unwrap().expect("A's punch lands");
        let b_sock = b.join().unwrap().expect("B's punch lands");

        // Both ends came back connected to each other, and the sockets work.
        assert_eq!(a_sock.peer_addr().unwrap().port(), b_port);
        assert_eq!(b_sock.peer_addr().unwrap().port(), a_port);
        a_sock.send(b"after the punch").unwrap();
        let mut buf = [0u8; 64];
        b_sock.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        // The tail probes are still in flight behind the handshake, so the first
        // datagram after a punch is often one of them. A caller has to skip them
        // — in a live pipe they simply fail the record MAC and are dropped, which
        // is why the probe magic is distinct from a sealed record.
        loop {
            let n = b_sock.recv(&mut buf).unwrap();
            if is_probe(&buf[..n], &pipe_id) {
                continue;
            }
            assert_eq!(&buf[..n], b"after the punch");
            break;
        }
    }

    #[test]
    fn a_punch_nobody_answers_fails_inside_its_window() {
        let dead = UdpSocket::bind("0.0.0.0:0").unwrap();
        let dead_port = dead.local_addr().unwrap().port();
        drop(dead); // nothing is listening there now

        let start = Instant::now();
        let r = punch(0, ("127.0.0.1", dead_port), &[3u8; 16], Duration::from_millis(600));
        assert!(r.is_err(), "no peer means no pipe — not a socket pointed at nothing");
        assert!(
            start.elapsed() < Duration::from_secs(3),
            "a failed punch must stay bounded; it runs on a runtime's Direct loop"
        );
    }

    #[test]
    fn a_probe_for_another_pipe_is_not_ours() {
        let mine = [1u8; 16];
        let theirs = [2u8; 16];
        assert!(is_probe(&probe(&mine), &mine));
        assert!(!is_probe(&probe(&theirs), &mine), "probes are demultiplexed by pipe id");
        assert!(!is_probe(b"SPDR\x02\x01 not a probe", &mine), "signalling is not a probe");
        assert!(!is_probe(b"short", &mine));
    }
}
