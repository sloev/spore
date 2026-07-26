//! KISS-over-TCP byte-stream bridge runner (shape 2). Point-to-point: give a
//! `target` to connect, or `None` to listen for one peer.
//!
//! Reconnects on drop — a peer restarting, a laptop sleeping, a NAT idling the
//! connection out should all be survivable without restarting the daemon.

use super::hub::Shared;
use crate::*;
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::Receiver;
use std::time::Duration;

pub fn run(hub: Shared, iface: Iface, rx: Receiver<Forward>, target: Option<String>) -> std::io::Result<()> {
    // Bind once, outside the retry loop: rebinding on every reconnect would race
    // with the port still being in TIME_WAIT.
    let listener = match &target {
        Some(addr) => {
            println!("  [tcp] iface {iface} connecting to {addr}");
            None
        }
        None => {
            println!("  [tcp] iface {iface} listening on :7373");
            Some(TcpListener::bind(("0.0.0.0", 7373))?)
        }
    };

    super::stream_link::run_reconnecting(
        hub,
        iface,
        rx,
        move || {
            let s = match (&target, &listener) {
                (Some(addr), _) => TcpStream::connect(addr)?,
                (None, Some(l)) => l.accept()?.0,
                (None, None) => unreachable!("a listener exists when there is no target"),
            };
            s.set_read_timeout(Some(Duration::from_millis(200)))?;
            Ok(s)
        },
        "tcp",
    )
}
