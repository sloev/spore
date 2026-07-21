//! Shape 5: shared stores over any bag transport (HTTP, folder, pastebin, BBS).

use crate::*;

/// The three transport-agnostic operations of a "bag" — a container that
/// carries envelopes between two nodes (spec Page 2's HTTP bag API, but the
/// same three ops serve a folder, a pastebin, or a BBS).
pub enum Bag {
    /// Incoming envelopes (one or more concatenated wire forms).
    Push(Vec<u8>),
    /// Advertise what we hold: return our stored IDs (16 B each).
    Inv,
    /// Fetch by ID: body is concatenated 16-B IDs; return their envelopes.
    Want(Vec<u8>),
}

/// Apply a bag operation. Returns `(forwards, response_body)` — forwards are
/// any relays the push triggered (run them on your other interfaces), and
/// the body is what to send back to the bag peer (empty for `Push`).
pub fn bag(node: &mut Node, op: Bag, iface: Iface, now: u32) -> (Vec<Forward>, Vec<u8>) {
    match op {
        Bag::Push(body) => {
            let mut fwd = Vec::new();
            let mut off = 0;
            while off < body.len() {
                match Envelope::decode(&body[off..]) {
                    Ok((_, n)) => {
                        let mut rx = node.on_rx(&body[off..off + n], iface, None, now);
                        fwd.append(&mut rx.forwards);
                        off += n;
                    }
                    Err(_) => break,
                }
            }
            (fwd, Vec::new())
        }
        Bag::Inv => (Vec::new(), node.stored_ids()),
        Bag::Want(ids) => {
            let mut body = Vec::new();
            for chunk in ids.chunks(16) {
                if chunk.len() == 16 {
                    let mut id = [0u8; 16];
                    id.copy_from_slice(chunk);
                    if let Some(w) = node.get_wire(&id) {
                        body.extend_from_slice(&w);
                    }
                }
            }
            (Vec::new(), body)
        }
    }
}

/// HTTP bag bridge runner (pull-only): serve `POST /spore/push`, `GET
/// /spore/inv`, `POST /spore/want` against the shared node. A push relays onto
/// the node's other interfaces; the bridge itself never has anything pushed to
/// it, so it registers with `Hub::register_pull` and takes no forward queue.
#[cfg(not(target_arch = "wasm32"))]
pub fn run_http(hub: super::hub::Shared, iface: Iface, port: u16) -> std::io::Result<()> {
    use std::net::TcpListener;
    let listener = TcpListener::bind(("0.0.0.0", port))?;
    println!("  [http] iface {iface} bag on :{port} (push/inv/want)");
    for stream in listener.incoming() {
        let mut s = match stream {
            Ok(s) => s,
            Err(_) => continue,
        };
        if let Err(e) = serve(&hub, iface, &mut s) {
            eprintln!("  [http] {e}");
        }
    }
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn serve(hub: &super::hub::Shared, iface: Iface, s: &mut std::net::TcpStream) -> std::io::Result<()> {
    use std::io::{Read, Write};

    let mut buf = Vec::new();
    let mut tmp = [0u8; 4096];
    let head_end = loop {
        if let Some(p) = find_sub(&buf, b"\r\n\r\n") {
            break p;
        }
        let n = s.read(&mut tmp)?;
        if n == 0 {
            return Ok(());
        }
        buf.extend_from_slice(&tmp[..n]);
    };
    let head = String::from_utf8_lossy(&buf[..head_end]).to_string();
    let mut lines = head.lines();
    let mut parts = lines.next().unwrap_or("").split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let path = parts.next().unwrap_or("").to_string();
    let mut content_len = 0usize;
    for l in lines {
        if let Some(v) = l.to_ascii_lowercase().strip_prefix("content-length:") {
            content_len = v.trim().parse().unwrap_or(0);
        }
    }
    let mut body = buf[head_end + 4..].to_vec();
    while body.len() < content_len {
        let n = s.read(&mut tmp)?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&tmp[..n]);
    }
    body.truncate(content_len);

    let route = path.split('?').next().unwrap_or(&path);
    let op = match (method.as_str(), route) {
        ("POST", "/spore/push") => Some(Bag::Push(body)),
        ("GET", "/spore/inv") => Some(Bag::Inv),
        ("POST", "/spore/want") => Some(Bag::Want(body)),
        _ => None,
    };
    let (status, resp) = match op {
        Some(op) => {
            // Run the bag op on the shared node; relay any forwards to the
            // other interfaces (a push over HTTP spreads onto radio, folders…).
            let (forwards, resp) = hub.with_node(|n| bag(n, op, iface, super::hub::now()));
            hub.originate(forwards);
            ("200 OK", resp)
        }
        None => ("404 Not Found", Vec::new()),
    };
    let header = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/x-spore\r\nContent-Length: {}\r\n\
         Access-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n",
        resp.len()
    );
    s.write_all(header.as_bytes())?;
    s.write_all(&resp)?;
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn find_sub(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}
