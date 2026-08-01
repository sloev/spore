//! Ham packet radio: KISS to a TNC, over TCP or a serial port.
//!
//! KISS is the minimal framing between a host and a Terminal Node Controller,
//! and it is already what [`super::kiss_stream`] speaks — so a TNC is a byte
//! stream like any other and the whole bridge is *getting* that stream. Two ways
//! in, because that is how TNCs actually present themselves:
//!
//! - **TCP** — soundcard TNCs (Direwolf's `KISSPORT`, default 8001) and most
//!   networked hardware TNCs listen on a socket.
//! - **Serial** — a hardware TNC on USB. Configure the line first (`stty`); this
//!   deliberately links no termios, exactly as the Meshtastic serial path does.
//!
//! **Regulatory note.** On amateur bands encryption is generally illegal, and
//! SPORE respects that by separating the two: signing identifies the sender and
//! is always fine, `ENCRYPTED` is what you must not set (spec §7). Transmitting
//! also requires a licence and your callsign. This bridge moves bytes; the rules
//! are yours to keep.

use super::hub::Shared;
use crate::*;
use std::sync::mpsc::Receiver;
use std::time::Duration;

/// Typical AX.25 `paclen`. Frames larger than this get fountain-fragmented by
/// the core rather than dropped by the TNC.
pub const AX25_PACLEN: usize = 256;

/// What a packet link will carry of *other people's file chunks*, per second —
/// see [`super::hub::Hub::register_limited`].
///
/// Zero. 1200-baud VHF packet is ~150 bytes/s shared by everyone on the
/// frequency, and HF is far slower; a single file chunk would hold the channel
/// for seconds. Messages, announces and manifests still pass at full speed,
/// which is what packet radio is good at.
pub const BULK_BYTES_PER_SEC: u32 = 0;

/// Bridge a TNC listening on a TCP socket (`host:port`, e.g. Direwolf's
/// `localhost:8001`).
pub fn run_tcp(hub: Shared, iface: Iface, rx: Receiver<Forward>, target: &str) -> std::io::Result<()> {
    hub.with_node(|n| n.mtu = n.mtu.min(AX25_PACLEN));
    println!("  [ax25] iface {iface} on {target} (KISS/TCP, {AX25_PACLEN}-byte paclen)");
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
        "ax25",
        // CLI/daemon-only: no per-bridge stop control yet (the process itself
        // is the unit of shutdown here), so this flag is never set.
        &std::sync::atomic::AtomicBool::new(false),
    )
}

/// Bridge a TNC on a serial port, by path. Configure the line first, e.g.
/// `stty -F /dev/ttyUSB0 9600 raw -echo`.
pub fn run_serial(hub: Shared, iface: Iface, rx: Receiver<Forward>, path: &str) -> std::io::Result<()> {
    hub.with_node(|n| n.mtu = n.mtu.min(AX25_PACLEN));
    let (r, w) = super::serial::open(path)?;
    println!("  [ax25] iface {iface} on {path} (KISS/serial, {AX25_PACLEN}-byte paclen)");
    super::stream_link::run_split(hub, iface, rx, r, w, "ax25")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    /// A TNC is just a KISS peer, so a socket that speaks KISS is enough to
    /// prove the whole path: envelope out, envelope in, router notified.
    #[test]
    fn a_kiss_tnc_over_tcp_carries_envelopes_both_ways() {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap().to_string();

        let tnc = std::thread::spawn(move || {
            let (mut c, _) = l.accept().unwrap();
            // Transmit: hand the node a framed envelope, as a radio would.
            let mut e = Envelope::new(ty::DATA, ZERO_DEST, 2_000_000_000, b"de m0spore".to_vec());
            e.flags |= fl::FLOOD;
            c.write_all(&crate::bridge::KissStream::frame(&e.wire())).unwrap();
            // Receive: whatever the node sends back, de-framed.
            c.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            let mut ks = crate::bridge::KissStream::new();
            let mut buf = [0u8; 2048];
            loop {
                let n = c.read(&mut buf).unwrap_or(0);
                if n == 0 {
                    return Vec::new();
                }
                let got = ks.push(&buf[..n]);
                if !got.is_empty() {
                    return got;
                }
            }
        });

        let hub = crate::bridge::hub::Hub::new(Node::new("ham", &[]));
        let (iface, rx) = hub.register_limited(BULK_BYTES_PER_SEC);
        let h = hub.clone();
        let target = addr.clone();
        std::thread::spawn(move || {
            let _ = run_tcp(h, iface, rx, &target);
        });

        // The MTU is clamped to the radio's paclen, so the core fragments to fit
        // rather than handing the TNC something it would silently truncate.
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while hub.with_node(|n| n.mtu) > AX25_PACLEN && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(hub.with_node(|n| n.mtu), AX25_PACLEN, "paclen must clamp the MTU");

        hub.send(ZERO_DEST, b"cq cq de spore".to_vec()).unwrap();
        let frames = tnc.join().unwrap();
        assert!(!frames.is_empty(), "the TNC received nothing");
        let (e, _) = Envelope::decode(&frames[0]).expect("a real envelope arrived KISS-framed");
        assert!(e.verify(), "and it is signed");

        // The inbound frame reached the router: it is in the store by content id.
        assert!(hub.with_node(|n| n.store_len()) >= 1);
    }
}
