use super::*;

/// Application tag marking a datagram payload (§ application layer).
pub const TAG_DGRAM: u8 = 0x04;

/// One end of a UDP-like link. Pure local state: peer address, port, the
/// peer's prekey (to seal to), a TX counter and a 64-wide replay window.
pub struct Session {
    me: Addr,
    peer: Addr,
    port: u16,
    peer_prekey: [u8; 32],
    tx_seq: u64,
    rx_hi: u64,
    rx_win: u64,
}

impl Session {
    pub fn new(me: Addr, peer: Addr, port: u16, peer_prekey: [u8; 32]) -> Self {
        Session { me, peer, port, peer_prekey, tx_seq: 0, rx_hi: 0, rx_win: 0 }
    }
    pub fn me(&self) -> Addr {
        self.me
    }
    pub fn peer(&self) -> Addr {
        self.peer
    }
    pub fn port(&self) -> u16 {
        self.port
    }
    pub fn peer_prekey(&self) -> [u8; 32] {
        self.peer_prekey
    }
    /// Next outbound sequence (1-based; 0 means "nothing sent/seen").
    pub fn next_tx_seq(&mut self) -> u64 {
        self.tx_seq += 1;
        self.tx_seq
    }
    /// DTLS-style sliding replay window over the last 64 sequences. Returns
    /// false for a replayed or too-old datagram; true (and records it) for a
    /// fresh one.
    pub fn accept_rx(&mut self, seq: u64) -> bool {
        const W: u64 = 64;
        if seq == 0 {
            return false;
        }
        if self.rx_hi == 0 {
            self.rx_hi = seq;
            self.rx_win = 1; // bit 0 == rx_hi seen
            return true;
        }
        if seq > self.rx_hi {
            let shift = seq - self.rx_hi;
            self.rx_win = if shift >= W { 1 } else { (self.rx_win << shift) | 1 };
            self.rx_hi = seq;
            return true;
        }
        let diff = self.rx_hi - seq;
        if diff >= W {
            return false; // fell off the window
        }
        let bit = 1u64 << diff;
        if self.rx_win & bit != 0 {
            return false; // replay
        }
        self.rx_win |= bit;
        true
    }
}

// Reliable-stream frames, carried inside a datagram's sealed payload.
const F_DATA: u8 = 0x00; // [0x00][offset:8][len:2][bytes]
const F_ACK: u8 = 0x01; //  [0x01][recv_next:8]

fn data_frame(offset: u64, bytes: &[u8]) -> Vec<u8> {
    let mut f = Vec::with_capacity(11 + bytes.len());
    f.push(F_DATA);
    f.extend_from_slice(&offset.to_be_bytes());
    f.extend_from_slice(&(bytes.len() as u16).to_be_bytes());
    f.extend_from_slice(bytes);
    f
}
fn ack_frame(recv_next: u64) -> Vec<u8> {
    let mut f = Vec::with_capacity(9);
    f.push(F_ACK);
    f.extend_from_slice(&recv_next.to_be_bytes());
    f
}

/// A simple QUIC-style reliable, ordered byte stream over a `Session`.
///
/// Go-Back-N: the sender streams `F_DATA` frames within a fixed window and,
/// on an ACK-progress timeout, rewinds to the last acknowledged offset and
/// resends. The receiver accepts only in-order bytes and cumulatively ACKs
/// the next offset it needs. No fancy congestion control — a fixed window and
/// a fixed retransmit timeout, on purpose.
pub struct Reliable {
    s: Session,
    // send side
    send_base: u64, // absolute offset of the first unacked byte
    send_next: u64, // absolute offset of the next byte to put on the wire
    out: Vec<u8>,   // buffered bytes; out[0] is byte at absolute send_base
    last_progress: u32,
    // recv side
    recv_next: u64,
    inbox: Vec<u8>, // delivered, in-order, awaiting read()
    // params
    max_frame: usize,
    window: usize,
    rto: u32,
}

impl Reliable {
    pub fn new(s: Session, max_frame: usize) -> Self {
        Reliable {
            s,
            send_base: 0,
            send_next: 0,
            out: Vec::new(),
            last_progress: 0,
            recv_next: 0,
            inbox: Vec::new(),
            max_frame: max_frame.max(1),
            window: max_frame.max(1) * 8,
            rto: 1,
        }
    }
    pub fn session(&self) -> &Session {
        &self.s
    }

    /// Queue bytes and send whatever the window allows now.
    pub fn write(&mut self, node: &mut Node, data: &[u8], now: u32) -> Vec<Forward> {
        self.out.extend_from_slice(data);
        self.flush(node, now)
    }

    /// Hand an inbound datagram envelope to the stream. Decrypts it, applies
    /// the frame, and returns any ACK or windowed sends that result.
    pub fn deliver(&mut self, node: &mut Node, e: &Envelope, now: u32) -> Vec<Forward> {
        match node.dg_recv(&mut self.s, e) {
            Some(frame) => self.on_frame(node, &frame, now),
            None => Vec::new(),
        }
    }

    /// Drain the in-order bytes delivered so far.
    pub fn read(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.inbox)
    }

    /// Drive retransmission timers. Call periodically with a monotonic `now`.
    pub fn poll(&mut self, node: &mut Node, now: u32) -> Vec<Forward> {
        if self.send_next > self.send_base && now.saturating_sub(self.last_progress) >= self.rto {
            self.send_next = self.send_base; // Go-Back-N: rewind and resend
            self.last_progress = now;
            return self.flush(node, now);
        }
        Vec::new()
    }

    fn flush(&mut self, node: &mut Node, now: u32) -> Vec<Forward> {
        let mut fwd = Vec::new();
        while (self.send_next - self.send_base) < self.window as u64 {
            let start = (self.send_next - self.send_base) as usize;
            if start >= self.out.len() {
                break;
            }
            let end = (start + self.max_frame).min(self.out.len());
            let frame = data_frame(self.send_next, &self.out[start..end]);
            fwd.append(&mut node.dg_send(&mut self.s, &frame, now));
            self.send_next += (end - start) as u64;
            self.last_progress = now;
        }
        fwd
    }

    fn on_frame(&mut self, node: &mut Node, frame: &[u8], now: u32) -> Vec<Forward> {
        let mut fwd = Vec::new();
        match frame.first().copied() {
            Some(F_DATA) if frame.len() >= 11 => {
                let mut ob = [0u8; 8];
                ob.copy_from_slice(&frame[1..9]);
                let offset = u64::from_be_bytes(ob);
                let len = u16::from_be_bytes([frame[9], frame[10]]) as usize;
                if frame.len() >= 11 + len {
                    if offset == self.recv_next {
                        self.inbox.extend_from_slice(&frame[11..11 + len]);
                        self.recv_next += len as u64;
                    }
                    // Cumulative ACK of the next offset we still need.
                    fwd.append(&mut node.dg_send(&mut self.s, &ack_frame(self.recv_next), now));
                }
            }
            Some(F_ACK) if frame.len() >= 9 => {
                let mut ab = [0u8; 8];
                ab.copy_from_slice(&frame[1..9]);
                let ackn = u64::from_be_bytes(ab);
                if ackn > self.send_base {
                    let adv = ((ackn - self.send_base) as usize).min(self.out.len());
                    self.out.drain(0..adv);
                    self.send_base = ackn;
                    self.last_progress = now;
                    fwd.append(&mut self.flush(node, now)); // window reopened
                }
            }
            _ => {}
        }
        fwd
    }
}
