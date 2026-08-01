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
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
#[cfg(test)]
use std::sync::Mutex;
use std::time::Duration;

/// Pump one framed byte stream until it closes.
///
/// `label` names the bridge in log lines. The stream should carry a read
/// timeout, so the loop can alternate between reading and draining `rx` without
/// blocking forever on either. The receiver is *borrowed*, so a caller that
/// reconnects keeps the same outbound queue across attempts — anything the
/// router handed over while the link was down is still there when it returns.
pub fn run<S: Read + Write>(
    hub: Shared,
    iface: Iface,
    rx: &Receiver<Forward>,
    stream: &mut S,
    label: &str,
    stop: &AtomicBool,
) -> std::io::Result<()> {
    let mut ks = KissStream::new();
    let mut buf = [0u8; 2048];
    loop {
        if stop.load(Ordering::Relaxed) {
            return Ok(());
        }
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

/// Keep a stream bridge up: connect, pump, and on any drop wait and try again.
///
/// A bridge that exits when its peer closes is a bridge that works once. Radios
/// get unplugged, onion circuits expire, laptops sleep — and the router will
/// happily keep handing this interface traffic throughout. So the default
/// behaviour is to reconnect, with exponential backoff so a link that is *really*
/// gone doesn't spin.
///
/// The outbound queue survives across attempts (see [`run`]): envelopes handed
/// over during an outage are delivered once the link returns, which is exactly
/// the store-and-forward behaviour the protocol expects of a link.
///
/// `connect` is whatever it takes to get a fresh stream — a TCP dial, a SOCKS
/// handshake, a SAM `STREAM ACCEPT`. Returning `Err` counts as a failed attempt.
pub fn run_reconnecting<S, F>(
    hub: Shared,
    iface: Iface,
    rx: Receiver<Forward>,
    mut connect: F,
    label: &str,
    stop: &AtomicBool,
) -> std::io::Result<()>
where
    S: Read + Write,
    F: FnMut() -> std::io::Result<S>,
{
    const FIRST: Duration = Duration::from_secs(2);
    const CAP: Duration = Duration::from_secs(60);
    const STEP: Duration = Duration::from_millis(200);
    let mut wait = FIRST;

    loop {
        if stop.load(Ordering::Relaxed) {
            return Ok(());
        }
        match connect() {
            Ok(mut s) => {
                wait = FIRST; // a successful connect resets the backoff
                if let Err(e) = run(hub.clone(), iface, &rx, &mut s, label, stop) {
                    eprintln!("  [{label}] iface {iface} link error: {e}");
                }
            }
            Err(e) => {
                // A `connect` that failed because `stop` cut it short (see
                // tcp::run's non-blocking accept poll) isn't a real failure —
                // don't log noise for the caller's own shutdown.
                if !stop.load(Ordering::Relaxed) {
                    eprintln!("  [{label}] iface {iface} connect failed: {e}");
                }
            }
        }
        if stop.load(Ordering::Relaxed) {
            return Ok(());
        }
        eprintln!("  [{label}] iface {iface} retrying in {}s", wait.as_secs());
        // Sleep in small steps so a stop signal isn't stuck behind up to 60s
        // of backoff.
        let mut remaining = wait;
        while remaining > Duration::ZERO {
            if stop.load(Ordering::Relaxed) {
                return Ok(());
            }
            let step = remaining.min(STEP);
            std::thread::sleep(step);
            remaining -= step;
        }
        wait = (wait * 2).min(CAP);
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc::channel;
    use std::sync::Arc;

    /// A stream that reports EOF immediately — a peer that hangs up at once.
    struct Dead;
    impl Read for Dead {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            Ok(0)
        }
    }
    impl Write for Dead {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// The point of reconnecting: a dropped link is retried, not fatal. Failures
    /// to *connect* count too, so a peer that is down when we start is waited
    /// for rather than giving up.
    #[test]
    fn a_dropped_link_is_retried_and_a_failed_connect_counts_as_an_attempt() {
        let tries = Arc::new(AtomicUsize::new(0));
        let seen = tries.clone();
        let hub = crate::bridge::hub::Hub::new(Node::new("n", &[]));
        let (_iface, rx) = hub.register();

        std::thread::spawn(move || {
            let _ = run_reconnecting(
                hub,
                0,
                rx,
                move || {
                    // Alternate: refuse, then connect-and-immediately-drop.
                    let n = seen.fetch_add(1, Ordering::SeqCst);
                    if n % 2 == 0 {
                        // `is_multiple_of` is 1.87; MSRV 1.75
                        Err(std::io::Error::other("nobody home"))
                    } else {
                        Ok(Dead)
                    }
                },
                "test",
                &AtomicBool::new(false),
            );
        });

        // Wait for the third attempt rather than for a fixed span. Backoff is 2s,
        // so a healthy run reaches three attempts in ~4s and this returns then —
        // the deadline is only a failure bound, never a delay, because the loop
        // exits the moment the count is reached. It is generous on purpose: at
        // 6s there was under 2s of slack for thread start and scheduling, and a
        // loaded CI runner spent it (seen on macOS: "expected repeated attempts,
        // got 2"). Timing out here should mean retrying is broken, not that the
        // machine was busy.
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        while tries.load(Ordering::SeqCst) < 3 && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(
            tries.load(Ordering::SeqCst) >= 3,
            "expected repeated attempts, got {}",
            tries.load(Ordering::SeqCst)
        );
    }

    /// Traffic handed over while the link is down must still go out when it
    /// returns — the queue belongs to the interface, not to one connection.
    #[test]
    fn the_outbound_queue_survives_a_reconnect() {
        let (tx, rx) = channel::<Forward>();
        tx.send(Forward::Flood { except: 9, bytes: b"queued while down".to_vec() }).unwrap();

        let hub = crate::bridge::hub::Hub::new(Node::new("n", &[]));
        let written = Arc::new(Mutex::new(Vec::new()));

        // First attempt: a stream that dies before draining anything.
        let stop = AtomicBool::new(false);
        let mut dead = Dead;
        let _ = run(hub.clone(), 0, &rx, &mut dead, "test", &stop);

        // Second attempt with the same receiver: the envelope is still there.
        // A real socket with a read timeout yields WouldBlock when idle, which
        // is what gives the loop its chance to drain the outbound queue; only
        // then does it EOF. A stream that EOFs on the very first read never
        // drains — which is fine, because the *next* reconnect will.
        struct Recorder(Arc<Mutex<Vec<u8>>>, usize);
        impl Read for Recorder {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
                self.1 += 1;
                if self.1 == 1 {
                    return Err(std::io::Error::from(ErrorKind::WouldBlock));
                }
                Ok(0)
            }
        }
        impl Write for Recorder {
            fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
                self.0.lock().unwrap().extend_from_slice(b);
                Ok(b.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let mut rec = Recorder(written.clone(), 0);
        let _ = run(hub, 0, &rx, &mut rec, "test", &stop);

        let got = written.lock().unwrap().clone();
        assert!(!got.is_empty(), "the queued envelope was lost across the reconnect");
        assert!(
            got.windows(17).any(|w| w == b"queued while down"),
            "the queued bytes should be framed and sent on the new link"
        );
    }
}
