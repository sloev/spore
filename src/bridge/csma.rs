//! Shape 4: shared buses (walkie-talkie, CB, ham FM) — KISS + CSMA + CRC tail.

use crate::*;

/// Shared buses have no native CRC (spec shape 4): append `SHA-256(frame)[0:4]`
/// so a garbled frame is dropped instead of parsed.
pub fn crc_append(frame: &[u8]) -> Vec<u8> {
    let d = Sha256::digest(frame);
    let mut out = frame.to_vec();
    out.extend_from_slice(&d[..4]);
    out
}
/// Verify and strip a CRC tail; `None` if it doesn't match.
pub fn crc_check(framed: &[u8]) -> Option<&[u8]> {
    if framed.len() < 4 {
        return None;
    }
    let (body, tail) = framed.split_at(framed.len() - 4);
    if Sha256::digest(body)[..4] == *tail {
        Some(body)
    } else {
        None
    }
}

/// Damped flooding / CSMA for shared media (§5.5): before transmitting a
/// flood, wait a random 1–5× airtime; if the same envelope ID is overheard
/// enough times meanwhile (≥ 2 for a flood, ≥ 1 for a directed send) cancel —
/// a neighbour already carried it, so your copy is redundant. Listen-before-
/// talk collapses a broadcast storm to roughly one transmission per frame.
#[derive(Default)]
pub struct Csma {
    pending: HashMap<Id, (u32, u16, u16)>, // id -> (fire_at, heard, cancel_at)
}
impl Csma {
    pub fn new() -> Self {
        Csma { pending: HashMap::new() }
    }
    /// Queue a transmission to fire after `delay`; cancel if overheard enough.
    pub fn schedule(&mut self, id: Id, now: u32, delay: u32, directed: bool) {
        let cancel_at = if directed { 1 } else { 2 };
        self.pending.entry(id).or_insert((now + delay, 0, cancel_at));
    }
    /// We heard this ID on the air (someone else transmitted it).
    pub fn overheard(&mut self, id: &Id) {
        if let Some(p) = self.pending.get_mut(id) {
            p.1 = p.1.saturating_add(1);
        }
    }
    /// IDs whose timer fired and weren't cancelled — transmit these now.
    pub fn ready(&mut self, now: u32) -> Vec<Id> {
        let fired: Vec<Id> = self
            .pending
            .iter()
            .filter(|(_, (at, _, _))| *at <= now)
            .map(|(id, _)| *id)
            .collect();
        let mut send = Vec::new();
        for id in fired {
            let (_, heard, cancel_at) = self.pending.remove(&id).unwrap();
            if heard < cancel_at {
                send.push(id); // not overheard enough -> transmit
            }
        }
        send
    }
    pub fn pending(&self) -> usize {
        self.pending.len()
    }
}
