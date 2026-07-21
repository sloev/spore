//! KISS-over-TCP byte-stream bridge runner (shape 2). Point-to-point: give a
//! `target` to connect, or `None` to listen for one peer.

use super::hub::Shared;
use super::KissStream;
use crate::*;
use std::io::{ErrorKind, Read, Write};
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
    let mut ks = KissStream::new();
    let mut buf = [0u8; 2048];
    loop {
        match stream.read(&mut buf) {
            Ok(0) => {
                println!("  [tcp] iface {iface} peer closed");
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
            let bytes = match f {
                Forward::Flood { bytes, .. } => bytes,
                Forward::Directed { bytes, .. } => bytes,
            };
            stream.write_all(&KissStream::frame(&bytes))?;
        }
    }
}
