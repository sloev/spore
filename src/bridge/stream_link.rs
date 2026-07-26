//! KISS over any byte stream — the shared body of every `stream`-form bridge.
//!
//! Shape 2 in the spec is "a byte stream needing framing", and once you have the
//! stream the rest is identical no matter how you got it: KISS-frame outbound
//! forwards, de-frame inbound bytes, hand them to the router. A TCP socket, a
//! serial port, a connection dialled through Tor's SOCKS proxy and a TNC on a
//! radio all reduce to the same loop.
//!
//! So the per-bridge work is only ever *obtaining the stream*. This is the part
//! nobody should write twice.

use super::hub::Shared;
use super::KissStream;
use crate::*;
use std::io::{ErrorKind, Read, Write};
use std::sync::mpsc::Receiver;

/// Pump one framed byte stream until it closes.
///
/// `label` names the bridge in log lines. The stream should carry a read
/// timeout, so the loop can alternate between reading and draining `rx` without
/// blocking forever on either.
pub fn run<S: Read + Write>(
    hub: Shared,
    iface: Iface,
    rx: Receiver<Forward>,
    stream: &mut S,
    label: &str,
) -> std::io::Result<()> {
    let mut ks = KissStream::new();
    let mut buf = [0u8; 2048];
    loop {
        match stream.read(&mut buf) {
            Ok(0) => {
                println!("  [{label}] iface {iface} peer closed");
                return Ok(());
            }
            Ok(n) => {
                for frame in ks.push(&buf[..n]) {
                    hub.on_rx(iface, &frame, None);
                }
            }
            Err(e) if e.kind() == ErrorKind::WouldBlock || e.kind() == ErrorKind::TimedOut => {}
            Err(e) => return Err(e),
        }
        while let Ok(f) = rx.try_recv() {
            let (Forward::Flood { bytes, .. } | Forward::Directed { bytes, .. }) = f;
            stream.write_all(&KissStream::frame(&bytes))?;
        }
    }
}

/// Pump a stream whose halves are separate handles and whose reads *block* — a
/// serial port, a pipe.
///
/// The reader goes in its own thread and the main loop blocks on `rx.recv()`, so
/// neither side busy-waits and neither starves the other. (The single-handle
/// [`run`] above can't do this: it needs a read timeout to alternate.)
pub fn run_split<R, W>(
    hub: Shared,
    iface: Iface,
    rx: Receiver<Forward>,
    mut r: R,
    mut w: W,
    label: &str,
) -> std::io::Result<()>
where
    R: Read + Send + 'static,
    W: Write,
{
    let rhub = hub.clone();
    let tag = label.to_string();
    std::thread::spawn(move || {
        let mut ks = KissStream::new();
        let mut buf = [0u8; 2048];
        loop {
            match r.read(&mut buf) {
                Ok(0) | Err(_) => break, // unplugged, or the pipe closed
                Ok(n) => {
                    for frame in ks.push(&buf[..n]) {
                        rhub.on_rx(iface, &frame, None);
                    }
                }
            }
        }
        println!("  [{tag}] iface {iface} reader ended");
    });

    loop {
        let Ok(f) = rx.recv() else { return Ok(()) }; // hub gone
        let (Forward::Flood { bytes, .. } | Forward::Directed { bytes, .. }) = f;
        w.write_all(&KissStream::frame(&bytes))?;
        w.flush()?;
    }
}
