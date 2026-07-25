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
//! ```text
//! # two fifos wire the daemon and the companion together (bidirectional):
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
}
