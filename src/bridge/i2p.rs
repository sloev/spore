//! SPORE over I2P, via the SAM v3 bridge.
//!
//! I2P is a garlic-routed overlay where every peer is a destination hash rather
//! than an address — no IP, no port, nothing to seize or geolocate. SAM is its
//! plain-text control protocol on TCP 7656, which means talking to it needs no
//! library at all: a handshake in ASCII, then the socket *becomes* the stream.
//!
//! Two sockets, because SAM works that way. The first creates the session and
//! must stay open for as long as you want the session to live. The second issues
//! `STREAM CONNECT` and, once SAM answers OK, is the raw connection to the peer —
//! at which point this is an ordinary KISS byte stream like any other.
//!
//! Accepting is the mirror image (`STREAM ACCEPT`) and is left to a future turn;
//! for now a node dials out, and two nodes meet if either can dial.
//!
//! As with Tor, nothing here trusts the link: envelopes are signed and sealed
//! whether or not I2P is underneath. What the overlay adds is that neither end
//! learns where the other is.

use super::hub::Shared;
use crate::*;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::sync::mpsc::Receiver;
use std::time::Duration;

/// Where the SAM bridge listens by default (i2pd and Java I2P alike).
pub const DEFAULT_SAM: &str = "127.0.0.1:7656";

/// Repliable-datagram MTU; streams carry more, but staying under it keeps a
/// node's traffic shaped like everyone else's on the network.
pub const I2P_MTU: usize = 1200;

fn bad(msg: impl Into<String>) -> std::io::Error {
    std::io::Error::other(msg.into())
}

/// Read one `\n`-terminated SAM reply line.
fn read_line(r: &mut BufReader<TcpStream>) -> std::io::Result<String> {
    let mut line = String::new();
    if r.read_line(&mut line)? == 0 {
        return Err(bad("SAM bridge closed the connection"));
    }
    Ok(line.trim_end().to_string())
}

/// Pull `KEY=VALUE` out of a SAM reply line.
///
/// Values may be quoted *and contain spaces* — `MESSAGE="already in use"` is the
/// common case — so this cannot be a `split_whitespace`: a quoted section has to
/// swallow the spaces inside it, or every error message arrives truncated to its
/// first word.
fn field<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let b = line.as_bytes();
    let mut i = 0;
    while i < b.len() {
        while i < b.len() && b[i] == b' ' {
            i += 1;
        }
        let start = i;
        let mut eq = None;
        while i < b.len() && b[i] != b' ' {
            if b[i] == b'=' && eq.is_none() {
                eq = Some(i);
            }
            if b[i] == b'"' {
                i += 1;
                while i < b.len() && b[i] != b'"' {
                    i += 1;
                }
            }
            i += 1;
        }
        if let Some(e) = eq {
            if &line[start..e] == key {
                return Some(line[e + 1..i.min(line.len())].trim_matches('"'));
            }
        }
    }
    None
}

/// `RESULT=OK`, or the message SAM gave for why not.
fn check(line: &str, what: &str) -> std::io::Result<()> {
    match field(line, "RESULT") {
        Some("OK") => Ok(()),
        Some(r) => {
            let msg = field(line, "MESSAGE").unwrap_or("no detail");
            Err(bad(format!("SAM {what} failed: {r} ({msg})")))
        }
        None => Err(bad(format!("SAM {what}: unparseable reply: {line}"))),
    }
}

/// The version handshake every SAM socket begins with.
pub fn hello(s: &mut TcpStream) -> std::io::Result<()> {
    s.write_all(b"HELLO VERSION MIN=3.0 MAX=3.1\n")?;
    let mut r = BufReader::new(s.try_clone()?);
    check(&read_line(&mut r)?, "HELLO")
}

/// Open a SAM session named `nick` with a transient destination, and return the
/// control socket. **Keep it open** — SAM tears the session down when it closes.
pub fn session_create(sam: &str, nick: &str) -> std::io::Result<TcpStream> {
    let mut s = TcpStream::connect(sam)?;
    s.set_read_timeout(Some(Duration::from_secs(60)))?; // tunnel build is slow
    hello(&mut s)?;
    s.write_all(
        format!("SESSION CREATE STYLE=STREAM ID={nick} DESTINATION=TRANSIENT SIGNATURE_TYPE=7\n").as_bytes(),
    )?;
    let mut r = BufReader::new(s.try_clone()?);
    check(&read_line(&mut r)?, "SESSION CREATE")?;
    Ok(s)
}

/// Dial `dest` (a b32 address or a full destination) inside session `nick`.
/// The returned socket is the raw stream to the peer.
pub fn stream_connect(sam: &str, nick: &str, dest: &str) -> std::io::Result<TcpStream> {
    let mut s = TcpStream::connect(sam)?;
    s.set_read_timeout(Some(Duration::from_secs(60)))?;
    hello(&mut s)?;
    s.write_all(format!("STREAM CONNECT ID={nick} DESTINATION={dest} SILENT=false\n").as_bytes())?;
    let mut r = BufReader::new(s.try_clone()?);
    check(&read_line(&mut r)?, "STREAM CONNECT")?;
    // Past the status line the socket carries nothing but peer bytes. BufReader
    // may have buffered some already, so hand back what it holds plus the socket.
    let pending = r.buffer().to_vec();
    if !pending.is_empty() {
        return Err(bad(format!("SAM sent {} unexpected bytes after STREAM STATUS", pending.len())));
    }
    Ok(s)
}

/// Bridge to a peer's I2P destination.
///
/// `target` is `<b32>.b32.i2p` or a full destination, optionally prefixed
/// `sam=HOST:PORT,` to point at a SAM bridge other than [`DEFAULT_SAM`].
pub fn run(hub: Shared, iface: Iface, rx: Receiver<Forward>, target: &str) -> std::io::Result<()> {
    let (sam, dest) = match target.split_once(',') {
        Some((p, d)) if p.starts_with("sam=") => (&p[4..], d.trim()),
        _ => (DEFAULT_SAM, target.trim()),
    };
    hub.with_node(|n| n.mtu = n.mtu.min(I2P_MTU));

    // The session id only has to be unique on this SAM bridge; our own address
    // already is.
    let a = hub.addr();
    let nick = format!("spore-{}", a.iter().map(|b| format!("{b:02x}")).collect::<String>());

    println!("  [i2p] iface {iface} opening SAM session on {sam}");
    let _session = session_create(sam, &nick)?; // held open for the bridge's life
    println!("  [i2p] iface {iface} dialling {dest}");
    let mut s = stream_connect(sam, &nick, dest)?;
    s.set_read_timeout(Some(Duration::from_millis(200)))?;
    println!("  [i2p] iface {iface} stream up");
    super::stream_link::run(hub, iface, rx, &mut s, "i2p")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    /// Stand in for a SAM bridge: reply OK to everything and record what was
    /// asked, so the request lines can be checked against the SAM v3 spec.
    fn fake_sam(results: Vec<&'static str>) -> (String, std::thread::JoinHandle<Vec<String>>) {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap().to_string();
        let h = std::thread::spawn(move || {
            let mut seen = Vec::new();
            for reply in results {
                let (c, _) = l.accept().unwrap();
                let mut r = BufReader::new(c.try_clone().unwrap());
                let mut w = c;
                // Every socket starts with HELLO.
                let mut line = String::new();
                r.read_line(&mut line).unwrap();
                seen.push(line.trim_end().to_string());
                w.write_all(b"HELLO REPLY RESULT=OK VERSION=3.1\n").unwrap();
                // Then one command.
                let mut cmd = String::new();
                r.read_line(&mut cmd).unwrap();
                seen.push(cmd.trim_end().to_string());
                w.write_all(reply.as_bytes()).unwrap();
                // Keep the socket alive until the client is done with it.
                std::thread::sleep(Duration::from_millis(50));
            }
            seen
        });
        (addr, h)
    }

    #[test]
    fn a_session_and_a_dial_speak_sam_v3() {
        let (sam, server) =
            fake_sam(vec!["SESSION STATUS RESULT=OK DESTINATION=abc\n", "STREAM STATUS RESULT=OK\n"]);
        let _s = session_create(&sam, "spore-test").expect("session");
        let _c = stream_connect(&sam, "spore-test", "peer.b32.i2p").expect("connect");
        let seen = server.join().unwrap();

        assert!(seen[0].starts_with("HELLO VERSION MIN=3.0"), "got {}", seen[0]);
        assert!(seen[1].contains("SESSION CREATE STYLE=STREAM"), "got {}", seen[1]);
        assert!(seen[1].contains("ID=spore-test"), "the session must be named");
        assert!(seen[3].contains("STREAM CONNECT"), "got {}", seen[3]);
        assert!(seen[3].contains("DESTINATION=peer.b32.i2p"), "got {}", seen[3]);
        assert!(seen[3].contains("ID=spore-test"), "the dial must join the session");
    }

    #[test]
    fn a_sam_refusal_is_reported_with_its_reason() {
        let (sam, server) =
            fake_sam(vec!["SESSION STATUS RESULT=DUPLICATED_ID MESSAGE=\"already in use\"\n"]);
        let e = session_create(&sam, "spore-test").unwrap_err();
        let _ = server.join();
        let msg = e.to_string();
        assert!(msg.contains("DUPLICATED_ID"), "got: {msg}");
        assert!(msg.contains("already in use"), "the MESSAGE field should survive: {msg}");
    }

    #[test]
    fn quoted_and_bare_fields_both_parse() {
        let line = "STREAM STATUS RESULT=CANT_REACH_PEER MESSAGE=\"no route\" VERSION=3.1";
        assert_eq!(field(line, "RESULT"), Some("CANT_REACH_PEER"));
        assert_eq!(field(line, "MESSAGE"), Some("no route"), "a quoted value keeps its spaces");
        assert_eq!(field(line, "VERSION"), Some("3.1"));
        assert_eq!(field(line, "ABSENT"), None);
    }
}
