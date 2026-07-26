//! A shared HTTP directory as a store — copyparty, and anything WebDAV-ish.
//!
//! Shape 5 in the spec is "a shared store": write envelopes as entries named by
//! hex id, and *reading the listing is receiving*. The folder bridge
//! ([`super::store`]) does that on a local disk; this does the same thing over
//! HTTP, so the shared drive can be somewhere else entirely — a
//! [copyparty](https://github.com/9001/copyparty) share, a WebDAV mount, any
//! server that answers `PUT` and `GET`.
//!
//! Why it is worth having as its own bridge rather than "just mount it": a
//! network mount fails in ways a filesystem API cannot express, and a poll loop
//! that treats every failure as "the folder is empty" would quietly stop
//! relaying. Here a failed listing is visibly a failed listing.
//!
//! **HTTP only, deliberately.** For an HTTPS share, front it with a local TLS
//! tunnel and point this at the tunnel — the same pattern the audio bridge uses
//! for sound cards and the Reticulum bridge uses for RNS. See the TLS note in
//! [`BRIDGES.md`](../../docs/BRIDGES.md).
//!
//! Nothing here trusts the server: envelopes are signed and content-addressed,
//! so a hostile share can withhold or replay, but cannot forge.

use super::hub::Shared;
use crate::*;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::sync::mpsc::Receiver;
use std::time::Duration;

/// Caps on what a share is allowed to make us hold. A listing of a very large
/// bag is the biggest legitimate response, and an envelope is at most a few KB.
const MAX_BODY: u64 = 8 * 1024 * 1024;
const MAX_LINE: u64 = 16 * 1024;
const MAX_HEADERS: usize = 100;

/// A parsed `http://host[:port]/path/` target.
struct Share {
    host: String,
    port: u16,
    path: String,
}

impl Share {
    fn parse(url: &str) -> std::io::Result<Share> {
        let rest = url
            .strip_prefix("http://")
            .ok_or_else(|| std::io::Error::other("copyparty: only http:// (put TLS in a tunnel)"))?;
        let (auth, path) = match rest.find('/') {
            Some(i) => (&rest[..i], &rest[i..]),
            None => (rest, "/"),
        };
        let (host, port) = match auth.rsplit_once(':') {
            Some((h, p)) => (h.to_string(), p.parse().unwrap_or(80)),
            None => (auth.to_string(), 80u16),
        };
        let mut path = path.to_string();
        if !path.ends_with('/') {
            path.push('/');
        }
        Ok(Share { host, port, path })
    }

    fn connect(&self) -> std::io::Result<TcpStream> {
        let s = TcpStream::connect((self.host.as_str(), self.port))?;
        s.set_read_timeout(Some(Duration::from_secs(20)))?;
        s.set_write_timeout(Some(Duration::from_secs(20)))?;
        Ok(s)
    }

    /// One request; returns `(status, body)`.
    ///
    /// Every read is bounded. The server is chosen by whoever wrote the config,
    /// but a share can be compromised or simply broken, and "it streams forever"
    /// must not be a way to take the node down.
    fn request(&self, method: &str, target: &str, body: &[u8]) -> std::io::Result<(u16, Vec<u8>)> {
        let mut s = self.connect()?;
        let head = format!(
            "{method} {target} HTTP/1.1\r\nHost: {}\r\nContent-Length: {}\r\n\
             Accept: */*\r\nConnection: close\r\n\r\n",
            self.host,
            body.len()
        );
        s.write_all(head.as_bytes())?;
        if !body.is_empty() {
            s.write_all(body)?;
        }
        s.flush()?;

        let mut r = BufReader::new(s);
        let mut status_line = String::new();
        (&mut r).take(MAX_LINE).read_line(&mut status_line)?;
        let status = status_line
            .split_whitespace()
            .nth(1)
            .and_then(|c| c.parse().ok())
            .ok_or_else(|| std::io::Error::other(format!("copyparty: bad status: {status_line:?}")))?;
        // Skip headers; `Connection: close` means the body runs to EOF. Both the
        // number of headers and each one's length are capped, so neither an
        // endless header block nor an endless single header can hold us.
        for _ in 0..MAX_HEADERS {
            let mut line = String::new();
            if (&mut r).take(MAX_LINE).read_line(&mut line)? == 0 || line.trim().is_empty() {
                break;
            }
        }
        let mut out = Vec::new();
        r.take(MAX_BODY).read_to_end(&mut out)?;
        if out.len() as u64 == MAX_BODY {
            return Err(std::io::Error::other(format!(
                "copyparty: response from {target} hit the {MAX_BODY}-byte cap"
            )));
        }
        Ok((status, out))
    }
}

/// Pull `<hexid>.spore` names out of whatever the server returned.
///
/// copyparty can answer with HTML or JSON depending on how it is asked, and a
/// WebDAV server answers with XML. Rather than parse three formats, scan for the
/// one thing they all contain: the file names. An id is fixed-width hex, so this
/// cannot match anything else by accident.
fn scan_names(body: &[u8]) -> Vec<String> {
    let text = String::from_utf8_lossy(body);
    let mut out = Vec::new();
    let suffix = b".spore";
    let mut i = 0;
    while let Some(pos) = text[i..].find(".spore") {
        let end = i + pos;
        let start = end.saturating_sub(32);
        if end >= 32 {
            let name = &text[start..end];
            if name.len() == 32 && name.bytes().all(|c| c.is_ascii_hexdigit()) {
                out.push(format!("{name}.spore"));
            }
        }
        i = end + suffix.len();
    }
    out.sort();
    out.dedup();
    out
}

/// Poll a shared HTTP directory: import what is new, upload what we hold.
///
/// `url` is `http://host[:port]/path/`. `period` is how often to sync.
pub fn run(
    hub: Shared,
    iface: Iface,
    rx: Receiver<Forward>,
    url: &str,
    period: Duration,
) -> std::io::Result<()> {
    let share = Share::parse(url)?;
    println!("  [copyparty] iface {iface} syncing {url}");
    let mut known: std::collections::HashSet<String> = std::collections::HashSet::new();

    loop {
        // Read = receive. A failed listing is reported, not mistaken for empty.
        match share.request("GET", &share.path, &[]) {
            Ok((200, body)) => {
                for name in scan_names(&body) {
                    if !known.insert(name.clone()) {
                        continue;
                    }
                    // The share's file list is the server's to choose, so this set
                    // is bounded like the folder bridges' — see `bound_known`.
                    crate::store::bound_known(&mut known);
                    let target = format!("{}{}", share.path, name);
                    if let Ok((200, bytes)) = share.request("GET", &target, &[]) {
                        hub.on_rx(iface, &bytes, None);
                    }
                }
            }
            Ok((status, _)) => eprintln!("  [copyparty] listing returned {status}"),
            Err(e) => eprintln!("  [copyparty] listing failed: {e}"),
        }

        // Write = send.
        while let Ok(f) = rx.try_recv() {
            let (Forward::Flood { bytes, .. } | Forward::Directed { bytes, .. }) = f;
            let Ok((e, _)) = Envelope::decode(&bytes) else { continue };
            let name = super::store::filename(&e.id());
            if !known.insert(name.clone()) {
                continue; // already up there — the share is content-addressed
            }
            let target = format!("{}{}", share.path, name);
            if let Err(e) = share.request("PUT", &target, &bytes) {
                eprintln!("  [copyparty] upload failed: {e}");
                known.remove(&name); // let a later pass retry it
            }
        }

        std::thread::sleep(period);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_listing_yields_ids_whatever_markup_wraps_them() {
        let id = "0123456789abcdef0123456789abcdef";
        let html = format!(r#"<a href="/bag/{id}.spore">{id}.spore</a> 128 B"#);
        let json = format!(r#"[{{"href":"{id}.spore","sz":128}}]"#);
        let dav = format!("<D:href>/bag/{id}.spore</D:href>");
        for (what, body) in [("html", html), ("json", json), ("dav", dav)] {
            let got = scan_names(body.as_bytes());
            assert_eq!(got, vec![format!("{id}.spore")], "{what} listing");
        }
    }

    #[test]
    fn things_that_merely_end_in_spore_are_not_ids() {
        for junk in ["notanid.spore", "beans.spore", "0123.spore", ".spore"] {
            assert!(scan_names(junk.as_bytes()).is_empty(), "matched {junk}");
        }
        // 32 hex chars is the only shape that counts, and one wrong char kills it.
        let almost = "0123456789abcdef0123456789abcdeg.spore";
        assert!(scan_names(almost.as_bytes()).is_empty(), "non-hex must not match");
    }

    #[test]
    fn a_url_splits_into_host_port_and_a_directory_path() {
        let s = Share::parse("http://box.local:3923/bag").unwrap();
        assert_eq!((s.host.as_str(), s.port, s.path.as_str()), ("box.local", 3923, "/bag/"));
        let d = Share::parse("http://box.local/").unwrap();
        assert_eq!((d.host.as_str(), d.port, d.path.as_str()), ("box.local", 80, "/"));
        assert!(Share::parse("https://box.local/").is_err(), "TLS belongs in a tunnel");
    }
}
