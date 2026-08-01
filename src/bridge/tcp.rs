//! KISS-over-TCP byte-stream bridge runner (shape 2). Point-to-point: give a
//! `target` to connect, or `None` to listen for one peer.
//!
//! Reconnects on drop — a peer restarting, a laptop sleeping, a NAT idling the
//! connection out should all be survivable without restarting the daemon.

use super::hub::Shared;
use crate::*;
use std::io::ErrorKind;
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::time::Duration;

/// Accept one connection on `l`, polling `stop` while none has arrived.
/// `TcpListener::accept` has no read-timeout equivalent (unlike a connected
/// `TcpStream`, which every other run loop here just gives a short one), so a
/// stop flag checked only *between* calls would still hang forever with no
/// peer ever connecting — `l` must already be non-blocking.
fn accept_or_stop(l: &TcpListener, stop: &AtomicBool) -> std::io::Result<TcpStream> {
    loop {
        match l.accept() {
            Ok((s, _)) => return Ok(s),
            Err(e) if e.kind() == ErrorKind::WouldBlock => {
                if stop.load(Ordering::Relaxed) {
                    return Err(std::io::Error::new(ErrorKind::Interrupted, "stopped"));
                }
                std::thread::sleep(Duration::from_millis(200));
            }
            Err(e) => return Err(e),
        }
    }
}

/// `stop`, once set, ends the loop — see [`super::udp::run`] for the split
/// between "stop the loop" and "unregister the hub interface" (the caller's
/// job).
pub fn run(
    hub: Shared,
    iface: Iface,
    rx: Receiver<Forward>,
    target: Option<String>,
    stop: Arc<AtomicBool>,
) -> std::io::Result<()> {
    // Bind once, outside the retry loop: rebinding on every reconnect would race
    // with the port still being in TIME_WAIT.
    let listener = match &target {
        Some(addr) => {
            println!("  [tcp] iface {iface} connecting to {addr}");
            None
        }
        None => {
            println!("  [tcp] iface {iface} listening on :7373");
            let l = TcpListener::bind(("0.0.0.0", 7373))?;
            l.set_nonblocking(true)?;
            Some(l)
        }
    };

    let accept_stop = stop.clone();
    super::stream_link::run_reconnecting(
        hub,
        iface,
        rx,
        move || {
            let s = match (&target, &listener) {
                (Some(addr), _) => TcpStream::connect(addr)?,
                (None, Some(l)) => accept_or_stop(l, &accept_stop)?,
                (None, None) => unreachable!("a listener exists when there is no target"),
            };
            s.set_nonblocking(false)?;
            s.set_read_timeout(Some(Duration::from_millis(200)))?;
            Ok(s)
        },
        "tcp",
        &stop,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The wrinkle this stop mechanism exists for: `TcpListener::accept` has no
    /// read-timeout, so a stop flag is useless unless the poll loop can actually
    /// notice it while no peer is connecting — not just after one shows up.
    #[test]
    fn accept_or_stop_ends_promptly_with_no_peer_ever_connecting() {
        let l = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        l.set_nonblocking(true).unwrap();
        let stop = AtomicBool::new(false);

        std::thread::scope(|scope| {
            let handle = scope.spawn(|| accept_or_stop(&l, &stop));
            // Let it poll a few times first, to prove it's really waiting, not
            // returning immediately for an unrelated reason.
            std::thread::sleep(Duration::from_millis(450));
            assert!(!handle.is_finished(), "accept_or_stop returned before anyone connected or stopped");

            stop.store(true, Ordering::Relaxed);
            let deadline = std::time::Instant::now() + Duration::from_secs(2);
            while !handle.is_finished() && std::time::Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(20));
            }
            let result = handle.join().unwrap();
            assert!(
                matches!(&result, Err(e) if e.kind() == ErrorKind::Interrupted),
                "expected an Interrupted stop signal, got {result:?}"
            );
        });
    }
}
