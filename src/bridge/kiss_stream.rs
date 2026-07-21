//! Shape 2: byte streams (TCP, serial, RFCOMM, TNCs) — streaming KISS framing.

use crate::kiss;

/// Streaming KISS de-framer. Feed byte slices as they arrive off a stream;
/// get back complete frames. (Unlike `kiss::decode`, this keeps state across
/// reads, so a frame split over two `read()`s still reassembles.)
#[derive(Default)]
pub struct KissStream {
    cur: Vec<u8>,
    in_frame: bool,
    got_cmd: bool,
    esc: bool,
}
impl KissStream {
    pub fn new() -> Self {
        Self::default()
    }
    /// Frame `payload` for transmission on a byte stream.
    pub fn frame(payload: &[u8]) -> Vec<u8> {
        kiss::encode(payload)
    }
    /// Feed raw bytes; return any complete frames they finished.
    pub fn push(&mut self, bytes: &[u8]) -> Vec<Vec<u8>> {
        const FEND: u8 = 0xC0;
        const FESC: u8 = 0xDB;
        const TFEND: u8 = 0xDC;
        const TFESC: u8 = 0xDD;
        let mut out = Vec::new();
        for &b in bytes {
            if b == FEND {
                if self.in_frame && !self.cur.is_empty() {
                    out.push(std::mem::take(&mut self.cur));
                }
                self.in_frame = true;
                self.got_cmd = false;
                self.esc = false;
                self.cur.clear();
                continue;
            }
            if !self.in_frame {
                continue;
            }
            if !self.got_cmd {
                self.got_cmd = true; // skip KISS command byte
                continue;
            }
            if self.esc {
                self.cur.push(if b == TFEND {
                    FEND
                } else if b == TFESC {
                    FESC
                } else {
                    b
                });
                self.esc = false;
            } else if b == FESC {
                self.esc = true;
            } else {
                self.cur.push(b);
            }
        }
        out
    }
}
