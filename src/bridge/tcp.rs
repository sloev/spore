//! KISS-over-TCP byte-stream bridge runner (shape 2). Point-to-point: give a
//! `target` to connect, or `None` to listen for one peer.

use super::hub::Shared;
use crate::*;
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::Receiver;
use std::time::Duration;

pub fn run(hub: Shared, iface: Iface, rx: Receiver<Forward>, target: Option<String>) -> std::io::Result<()> {
    let mut stream = match target {
        Some(addr) => {
            println!("  [tcp] iface {iface} connecting to {addr}");
            TcpStream::connect(addr)?
        }
        None => {
            let l = TcpListener::bind(("0.0.0.0", 7373))?;
            println!("  [tcp] iface {iface} listening on :7373 for one peer");
            l.accept()?.0
        }
    };
    stream.set_read_timeout(Some(Duration::from_millis(200)))?;
    super::stream_link::run(hub, iface, rx, &mut stream, "tcp")
}
