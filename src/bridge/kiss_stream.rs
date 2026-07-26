//! Shape 2: byte streams (TCP, serial, RFCOMM, TNCs) — streaming KISS framing.

use crate::kiss;

/// The largest frame this will assemble before giving up on it.
///
/// KISS carries one envelope, and `Node::send` fragments to the MTU, so a real
/// frame is at most a few KB — 64 KiB is 46× the default MTU and still bounded.
/// Without a bound, a peer that opens a frame and then never closes it makes us
/// buffer for as long as it keeps typing, which is a remote out-of-memory with
/// no authentication required.
pub const MAX_FRAME: usize = 64 * 1024;

/// Streaming KISS de-framer. Feed byte slices as they arrive off a stream;
/// get back complete frames. (Unlike `kiss::decode`, this keeps state across
/// reads, so a frame split over two `read()`s still reassembles.)
#[derive(Default)]
pub struct KissStream {
    cur: Vec<u8>,
    in_frame: bool,
    got_cmd: bool,
    esc: bool,
    /// This frame outgrew [`MAX_FRAME`]; ignore it until the next delimiter.
    overflow: bool,
    /// How many frames have been dropped for overflow — a bridge can log it, and
    /// a steady climb means either a hostile peer or a badly framed device.
    dropped: u64,
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
                // A delimiter always resynchronises, which is what makes an
                // overrun recoverable: the oversized frame is abandoned and the
                // next one is read normally.
                if self.in_frame && !self.overflow && !self.cur.is_empty() {
                    out.push(std::mem::take(&mut self.cur));
                }
                self.in_frame = true;
                self.got_cmd = false;
                self.esc = false;
                self.overflow = false;
                self.cur.clear();
                self.cur.shrink_to(MAX_FRAME.min(4096));
                continue;
            }
            if self.overflow {
                continue; // still skipping the frame that ran away
            }
            if !self.in_frame {
                continue;
            }
            if !self.got_cmd {
                self.got_cmd = true; // skip KISS command byte
                continue;
            }
            if self.cur.len() >= MAX_FRAME {
                self.overflow = true;
                self.dropped += 1;
                self.cur.clear();
                self.cur.shrink_to(4096);
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

impl KissStream {
    /// Frames abandoned for exceeding [`MAX_FRAME`] since this de-framer started.
    pub fn dropped(&self) -> u64 {
        self.dropped
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A peer that opens a frame and never closes it must not be able to make us
    /// buffer without limit — that is a remote out-of-memory needing no
    /// authentication, reachable on every stream bridge.
    #[test]
    fn an_endless_frame_is_abandoned_rather_than_buffered() {
        let mut ks = KissStream::new();
        ks.push(&[0xC0, 0x00]); // open a frame
        let junk = vec![0x41u8; 64 * 1024];
        for _ in 0..64 {
            assert!(ks.push(&junk).is_empty(), "an unterminated frame must yield nothing");
        }
        assert!(ks.dropped() >= 1, "the runaway frame should have been dropped");

        // …and the stream recovers: the next properly delimited frame decodes.
        let good = KissStream::frame(b"still here");
        let got = ks.push(&good);
        assert_eq!(got, vec![b"still here".to_vec()], "a delimiter resynchronises");
    }

    /// The bound must not clip traffic that is actually legal.
    #[test]
    fn a_frame_at_the_limit_still_arrives() {
        let mut ks = KissStream::new();
        let big = vec![0x5Au8; MAX_FRAME - 1];
        let got = ks.push(&KissStream::frame(&big));
        assert_eq!(got.len(), 1, "a frame just under the cap is delivered");
        assert_eq!(got[0].len(), big.len());
    }

    /// Escaped bytes count toward the limit as decoded bytes, so an attacker
    /// cannot double the memory cost by escaping everything.
    #[test]
    fn escaping_does_not_buy_extra_room() {
        let mut ks = KissStream::new();
        ks.push(&[0xC0, 0x00]);
        // Every byte an escape pair: 2 bytes on the wire, 1 byte buffered. Past
        // the cap it must drop — the wire length being double is not extra room.
        // `repeat_n` is 1.82; MSRV is 1.75 (Cargo.toml `rust-version`).
        let esc: Vec<u8> = std::iter::repeat([0xDB, 0xDC]).take(MAX_FRAME + 8).flatten().collect();
        ks.push(&esc);
        assert!(ks.dropped() >= 1, "escaped bytes must still hit the cap");
    }
}
