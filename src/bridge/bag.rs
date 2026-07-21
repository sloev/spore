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
