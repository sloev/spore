//! RNS payload bridge — SPORE rides **Reticulum** as data on a shared PLAIN
//! destination (a broadcast bus). This is the *native* Reticulum integration
//! (distinct from driving an RNode radio directly, which the browser transport
//! [`web/transports/reticulum.mjs`] does): envelopes travel as RNS packets, so
//! Reticulum provides transport, path-finding, and reach across every interface
//! it is configured with (LoRa, TCP, I2P, packet radio, …).
//!
//! ## Why a companion, not a native RNS stack
//! Reticulum's reference implementation is the Python `rns` library, and its
//! packet format and identity/crypto are defined by that implementation. Rather
//! than re-implement (and risk mis-implementing) RNS on-wire, this bridge is the
//! **portable half only**: it exchanges KISS-framed envelopes on stdin/stdout,
//! exactly like [`crate::bridge::audio::run_pipe`], and a small companion process
//! ([`tools/reticulum_companion.py`]) does the real RNS work with the canonical
//! library. SPORE's envelope carries its own signature and optional encryption,
//! so nothing security-critical lives in the companion.
//!
//! ## Wire
//! - SPORE ⇄ companion: **KISS** frames (one envelope each), matching every other
//!   stream bridge (`src/kiss.rs`).
//! - companion ⇄ RNS: the raw envelope as the data of an `RNS.Packet` sent to a
//!   PLAIN destination `spore.mesh` that every SPORE-over-RNS node shares, so it
//!   behaves as a broadcast bus (`U` is effectively null; the envelope's own
//!   `dest` filters). The node's MTU is clamped so envelopes fit one RNS packet;
//!   larger objects fountain-fragment above the bridge like on any medium.
//!
//! ## Running
//! Over a socket (the companion can be on another host, no fifos):
//! ```text
//! python3 tools/reticulum_companion.py --listen tcp:4242   # on the RNS host
//! spore reticulum-tcp:10.0.0.9:4242                         # anywhere on the LAN
//! ```
//! Or over stdio, the original way:
//! ```text
//! mkfifo /tmp/spore_up /tmp/spore_down
//! python3 tools/reticulum_companion.py < /tmp/spore_up > /tmp/spore_down &
//! spore reticulum  < /tmp/spore_down > /tmp/spore_up
//! ```

/// The node's MTU is clamped to this so each envelope fits a single RNS packet's
/// PLAIN payload; SPORE fragments anything larger. Conservative on purpose.
pub const RNS_SINGLE_PACKET_MDU: usize = 383;

/// What a Reticulum link will carry of *other people's file chunks*, per second
/// — see [`crate::bridge::hub::Hub::register_limited`].
///
/// A conservative default, not a measurement: RNS spans everything from TCP to
/// LoRa, and the slowest interface on the path is the one that suffers. Messages,
/// announces and manifests are never counted against it. Raise it with
/// `Hub::set_bulk_budget` when the path is known to be fast.
pub const BULK_BYTES_PER_SEC: u32 = 32;

/// Run the RNS bridge over stdin/stdout: read KISS-framed envelopes from the
/// companion into the node, and write outbound forwards back as KISS frames.
/// Blocks until either stream ends. Status goes to **stderr** so it never
/// corrupts the frame stream on stdout.
#[cfg(not(target_arch = "wasm32"))]
pub fn run_pipe(
    hub: crate::bridge::hub::Shared,
    iface: crate::Iface,
    rx: std::sync::mpsc::Receiver<crate::Forward>,
) -> std::io::Result<()> {
    use std::io::{Read, Write};

    hub.with_node(|n| n.mtu = n.mtu.min(RNS_SINGLE_PACKET_MDU));
    eprintln!(
        "  [reticulum] iface {iface} — KISS envelopes on stdin/stdout; \
         pipe to tools/reticulum_companion.py"
    );

    // Reader thread: KISS frames from the companion → the shared node.
    let rhub = hub.clone();
    std::thread::spawn(move || {
        let mut stdin = std::io::stdin().lock();
        let mut framer = super::kiss_stream::KissStream::new();
        let mut buf = [0u8; 4096];
        loop {
            match stdin.read(&mut buf) {
                Ok(0) | Err(_) => break, // companion closed the pipe
                Ok(n) => {
                    for frame in framer.push(&buf[..n]) {
                        rhub.on_rx(iface, &frame, None);
                    }
                }
            }
        }
    });

    // Main loop: outbound forwards → KISS frames to the companion. `recv` blocks
    // until there is something to send, so this never busy-waits.
    let mut stdout = std::io::stdout().lock();
    loop {
        match rx.recv() {
            Ok(f) => {
                let bytes = match f {
                    crate::Forward::Flood { bytes, .. } => bytes,
                    crate::Forward::Directed { bytes, .. } => bytes,
                };
                stdout.write_all(&crate::kiss::encode(&bytes))?;
                stdout.flush()?;
            }
            Err(_) => return Ok(()), // hub gone
        }
    }
}

/// Run the RNS bridge to a companion reachable over **TCP**, reconnecting.
///
/// The same KISS-framed envelopes as [`run_pipe`], but the companion can live on
/// another host — so one machine's `rns` instance (with its own TCP/UDP/LoRa
/// interfaces configured) serves a fleet of SPORE nodes over the LAN, and the
/// awkward `mkfifo` dance is gone. `target` is `host:port`; the companion listens
/// there with `--listen tcp:PORT`.
#[cfg(not(target_arch = "wasm32"))]
pub fn run_tcp(
    hub: crate::bridge::hub::Shared,
    iface: crate::Iface,
    rx: std::sync::mpsc::Receiver<crate::Forward>,
    target: &str,
) -> std::io::Result<()> {
    use std::time::Duration;
    hub.with_node(|n| n.mtu = n.mtu.min(RNS_SINGLE_PACKET_MDU));
    println!("  [reticulum] iface {iface} — KISS to companion at {target} (TCP)");
    let target = target.to_string();
    super::stream_link::run_reconnecting(
        hub,
        iface,
        rx,
        move || {
            let s = std::net::TcpStream::connect(&target)?;
            s.set_read_timeout(Some(Duration::from_millis(200)))?;
            Ok(s)
        },
        "reticulum",
        // CLI/daemon-only: no per-bridge stop control yet.
        &std::sync::atomic::AtomicBool::new(false),
    )
}

/// Run the RNS bridge to a companion over **UDP**.
///
/// `bind` is our local `host:port`; `peer` is the companion's `host:port`. One
/// envelope per datagram (KISS-framed for parity with the other transports, and
/// so a fragmented frame split across datagrams still reassembles). Datagram
/// loss is fine — SPORE re-asks — which is what makes UDP a legitimate choice
/// here rather than a corner cut.
#[cfg(not(target_arch = "wasm32"))]
pub fn run_udp(
    hub: crate::bridge::hub::Shared,
    iface: crate::Iface,
    rx: std::sync::mpsc::Receiver<crate::Forward>,
    bind: &str,
    peer: &str,
) -> std::io::Result<()> {
    use std::net::{SocketAddr, UdpSocket};
    use std::time::Duration;

    let peer: SocketAddr =
        peer.parse().map_err(|_| std::io::Error::other(format!("reticulum: bad peer {peer:?}")))?;
    let sock = UdpSocket::bind(bind)?;
    sock.set_read_timeout(Some(Duration::from_millis(200)))?;
    hub.with_node(|n| n.mtu = n.mtu.min(RNS_SINGLE_PACKET_MDU));
    println!("  [reticulum] iface {iface} — KISS to companion at {peer} (UDP, bound {bind})");

    struct Rns {
        sock: UdpSocket,
        peer: SocketAddr,
        framer: super::kiss_stream::KissStream,
        out: std::collections::VecDeque<Vec<u8>>,
    }
    impl super::driver::DatagramTransport for Rns {
        type Addr = SocketAddr;
        fn recv(&mut self) -> super::driver::Received<SocketAddr> {
            // Drain any frames a previous datagram completed before reading more.
            if let Some(f) = self.out.pop_front() {
                return Ok(Some((f, Some(self.peer))));
            }
            let mut buf = [0u8; 2048];
            match self.sock.recv_from(&mut buf) {
                Ok((n, from)) => {
                    // Only the companion may feed this framer. Unlike
                    // `udp::run_group`, where each datagram is a whole envelope
                    // and a stranger's datagram is merely ignored, this framer
                    // carries KISS state *across* datagrams — so bytes from any
                    // other source interleave with the frame the companion is
                    // halfway through sending and corrupt it. Single-sourcing it
                    // is what makes that unreachable.
                    if from != self.peer {
                        return Ok(None);
                    }
                    for f in self.framer.push(&buf[..n]) {
                        self.out.push_back(f);
                    }
                    Ok(self.out.pop_front().map(|f| (f, Some(self.peer))))
                }
                Err(e)
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        || e.kind() == std::io::ErrorKind::TimedOut =>
                {
                    Ok(None)
                }
                Err(e) => Err(e),
            }
        }
        fn send(&mut self, _to: Option<&SocketAddr>, env: &[u8]) -> std::io::Result<()> {
            self.sock.send_to(&crate::kiss::encode(env), self.peer)?;
            Ok(())
        }
    }

    // This CLI/daemon-only runner has no stop control of its own yet (the
    // process itself is the unit of shutdown here); `run_datagram`'s stop
    // check is a no-op flag that never gets set.
    super::driver::run_datagram(
        hub,
        iface,
        rx,
        &std::sync::atomic::AtomicBool::new(false),
        Rns { sock, peer, framer: super::kiss_stream::KissStream::new(), out: Default::default() },
    )
}

#[cfg(test)]
mod tests {
    // The two halves the bridge relies on — `kiss::encode` (outbound) and
    // `KissStream::push` (inbound) — must agree, including on the escaped bytes
    // (0xC0/0xDB) that appear in real envelopes.
    #[test]
    fn kiss_pipe_roundtrip() {
        let env: &[u8] = &[0x01, 0x02, 0xC0, 0x03, 0xDB, 0xDC, 0x00, 0xFF, 0xC0];
        let framed = crate::kiss::encode(env);
        let mut framer = crate::bridge::kiss_stream::KissStream::new();
        // Split across two reads to prove the streaming de-framer reassembles.
        let mut out = framer.push(&framed[..3]);
        out.extend(framer.push(&framed[3..]));
        assert_eq!(out, vec![env.to_vec()]);
    }

    /// Why `run_udp` drops datagrams that are not from the companion.
    ///
    /// The UDP bridge keeps one `KissStream` across datagrams, so a frame may be
    /// half-assembled when the next one arrives. If any host could feed it, this
    /// is what they would get to do — so the source check is the fix, and this is
    /// the damage it prevents.
    #[test]
    fn foreign_bytes_would_corrupt_a_half_assembled_frame() {
        let env: &[u8] = b"the companion's envelope";
        let framed = crate::kiss::encode(env);
        let split = framed.len() / 2;

        let mut framer = crate::bridge::kiss_stream::KissStream::new();
        let mut out = framer.push(&framed[..split]); // companion, mid-frame
        out.extend(framer.push(&[0xC0, 0x00, b'j', b'u', b'n', b'k', 0xC0])); // a stranger
        out.extend(framer.push(&framed[split..])); // companion finishes
        assert_ne!(out, vec![env.to_vec()], "interleaved bytes must be able to break the frame");

        // Single-sourced — which is what the bridge now guarantees — it survives.
        let mut clean = crate::bridge::kiss_stream::KissStream::new();
        let mut ok = clean.push(&framed[..split]);
        ok.extend(clean.push(&framed[split..]));
        assert_eq!(ok, vec![env.to_vec()]);
    }
}
