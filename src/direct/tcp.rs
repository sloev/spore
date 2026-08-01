//! A real TCP adapter for [`DatagramPort`].
//!
//! TCP is a byte stream, not a datagram medium, so this adapter has one job the UDP
//! one does not: **frame**. Each sealed record is written with a 4-byte big-endian
//! length prefix, and reads accumulate bytes until a whole length-prefixed record
//! is present. That restores the one-`send`-one-record shape [`DatagramPort`]
//! promises on top of a stream that may split or coalesce writes arbitrarily.
//!
//! It stays best-effort at the record layer despite TCP's reliability: the pipe's
//! sequence numbers and per-record AEAD do not assume in-order, gap-free delivery,
//! and layering an ordered stream on top would reintroduce the head-of-line
//! blocking Direct exists to avoid for media. What TCP buys here is traversal and
//! reach where UDP is blocked, not stream semantics leaking up into the pipe.
//!
//! Non-blocking throughout: a queued write that the kernel can't take yet is held
//! in an out-buffer and flushed on the next call, so neither `send` nor `try_recv`
//! ever blocks the poll loop. `std::net` is unavailable on `wasm32`, so the adapter
//! is gated off that target.

use super::DatagramPort;
use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};

/// Width of the length prefix. `u32` is far more than any record needs but keeps
/// the framing aligned and unambiguous.
const LEN_PREFIX: usize = 4;

/// A [`DatagramPort`] over a TCP stream, with length-prefixed record framing.
pub struct TcpPort {
    stream: TcpStream,
    mtu: usize,
    /// Bytes read from the socket that do not yet form a complete record.
    inbuf: Vec<u8>,
    /// Bytes queued for send that the non-blocking socket has not yet accepted.
    outbuf: Vec<u8>,
    /// Scratch buffer for one `read` syscall.
    rbuf: Vec<u8>,
}

impl TcpPort {
    /// Connect to `peer` and wrap the stream, advertising `mtu` as the usable body
    /// size.
    pub fn connect(peer: impl ToSocketAddrs, mtu: usize) -> io::Result<TcpPort> {
        Self::from_stream(TcpStream::connect(peer)?, mtu)
    }

    /// Wrap an established stream (e.g. one just returned by `TcpListener::accept`).
    /// Disables Nagle so a small record is put on the wire immediately rather than
    /// waiting to coalesce — latency is the whole point of Direct — and switches the
    /// stream to non-blocking.
    pub fn from_stream(stream: TcpStream, mtu: usize) -> io::Result<TcpPort> {
        stream.set_nodelay(true)?;
        stream.set_nonblocking(true)?;
        Ok(TcpPort {
            stream,
            mtu,
            inbuf: Vec::new(),
            outbuf: Vec::new(),
            rbuf: vec![0u8; mtu + LEN_PREFIX + 64],
        })
    }

    /// The local address of the stream.
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.stream.local_addr()
    }

    /// Largest framed record we will accept off the wire: a full-MTU body plus the
    /// record overhead, with slack. A length prefix claiming more than this is
    /// treated as a corrupt or hostile stream — see `try_recv`.
    fn max_frame(&self) -> usize {
        self.mtu + 64
    }

    /// Push whatever the socket will currently take from the out-buffer. A partial
    /// write leaves the remainder queued for next time; `WouldBlock` and transient
    /// errors just stop the flush without losing anything.
    fn pump_out(&mut self) {
        while !self.outbuf.is_empty() {
            match self.stream.write(&self.outbuf) {
                Ok(0) => break,
                Ok(n) => {
                    self.outbuf.drain(..n);
                }
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(_) => break,
            }
        }
    }

    /// Drain everything the socket currently has into the in-buffer.
    fn fill_in(&mut self) {
        loop {
            match self.stream.read(&mut self.rbuf) {
                Ok(0) => break, // peer closed; nothing more will arrive
                Ok(n) => self.inbuf.extend_from_slice(&self.rbuf[..n]),
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(_) => break,
            }
        }
    }
}

impl DatagramPort for TcpPort {
    fn mtu(&self) -> usize {
        self.mtu
    }

    fn send(&mut self, frame: &[u8]) -> io::Result<()> {
        self.pump_out();
        let mut msg = Vec::with_capacity(LEN_PREFIX + frame.len());
        msg.extend_from_slice(&(frame.len() as u32).to_be_bytes());
        msg.extend_from_slice(frame);

        // If nothing is backed up, try to write straight through; otherwise preserve
        // ordering by appending behind the backlog.
        if self.outbuf.is_empty() {
            match self.stream.write(&msg) {
                Ok(n) if n == msg.len() => Ok(()),
                Ok(n) => {
                    self.outbuf.extend_from_slice(&msg[n..]);
                    Ok(())
                }
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    self.outbuf.extend_from_slice(&msg);
                    Ok(())
                }
                Err(e) => Err(e),
            }
        } else {
            self.outbuf.extend_from_slice(&msg);
            Ok(())
        }
    }

    fn try_recv(&mut self) -> Option<Vec<u8>> {
        self.pump_out();
        self.fill_in();

        if self.inbuf.len() < LEN_PREFIX {
            return None;
        }
        let len = u32::from_be_bytes(self.inbuf[..LEN_PREFIX].try_into().unwrap()) as usize;

        // A length prefix bigger than any record could legitimately be is either
        // corruption or an attempt to make us buffer unboundedly while the sender
        // dribbles bytes. We cannot resynchronise a stream whose framing we no
        // longer trust, so drop what we have and stop feeding this link; the pipe
        // simply sees no more frames, which is the safe failure.
        if len > self.max_frame() {
            self.inbuf.clear();
            return None;
        }
        if self.inbuf.len() < LEN_PREFIX + len {
            return None; // record not fully arrived yet
        }
        let frame = self.inbuf[LEN_PREFIX..LEN_PREFIX + len].to_vec();
        self.inbuf.drain(..LEN_PREFIX + len);
        Some(frame)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::direct::{Answer, Candidate, Medium, Need, Offer, Pipe, RecordType};
    use std::net::TcpListener;
    use std::time::Duration;

    /// A connected client/server stream pair on loopback.
    fn stream_pair() -> (TcpStream, TcpStream) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let client = TcpStream::connect(addr).unwrap();
        let (server, _) = listener.accept().unwrap();
        (client, server)
    }

    /// Read one framed record, retrying briefly since the bytes cross the kernel.
    fn recv_frame(port: &mut TcpPort) -> Vec<u8> {
        for _ in 0..400 {
            if let Some(f) = port.try_recv() {
                return f;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("no frame arrived within the deadline");
    }

    fn recv_record(pipe: &mut Pipe<TcpPort>) -> (RecordType, Vec<u8>) {
        for _ in 0..400 {
            if let Some(r) = pipe.poll() {
                return r;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("no record arrived within the deadline");
    }

    #[test]
    fn tcp_carries_records_both_ways_over_real_sockets() {
        let (client, server) = stream_pair();
        let init_port = TcpPort::from_stream(client, 1200).unwrap();
        let resp_port = TcpPort::from_stream(server, 1200).unwrap();

        let (offer_bytes, pending) = Pipe::<TcpPort>::offer(
            [0xA1u8; 8],
            [0xB2u8; 8],
            [7u8; 16],
            Need { min_bps: 1, mtu_needed: 1, max_latency_ms: None },
            vec![Candidate {
                medium: Medium::tcp(),
                locator: b"127.0.0.1".to_vec(),
                est_bps: 1_000_000,
                mtu: 1200,
                rtt_hint_ms: 1,
            }],
        );
        let offer = Offer::decode(&offer_bytes).unwrap();
        let (answer_bytes, resp_pipe) = Pipe::answer(&offer, [0xB2u8; 8], &[Medium::tcp()], resp_port);
        let mut resp_pipe = resp_pipe.unwrap();
        let answer = Answer::decode(&answer_bytes).unwrap();
        let mut init_pipe = Pipe::finish(pending, &answer, init_port).unwrap();

        init_pipe.send(RecordType::Data, b"north pier at midnight").unwrap();
        assert_eq!(recv_record(&mut resp_pipe), (RecordType::Data, b"north pier at midnight".to_vec()));

        resp_pipe.send(RecordType::Media, b"copy that").unwrap();
        assert_eq!(recv_record(&mut init_pipe), (RecordType::Media, b"copy that".to_vec()));
    }

    #[test]
    fn tcp_framing_separates_records_a_stream_may_coalesce() {
        // Two records written back-to-back land as one read on the peer; the length
        // prefixes must still split them into exactly the two originals.
        let (client, server) = stream_pair();
        let mut tx = TcpPort::from_stream(client, 1200).unwrap();
        let mut rx = TcpPort::from_stream(server, 1200).unwrap();

        tx.send(b"one").unwrap();
        tx.send(b"two-is-longer").unwrap();

        assert_eq!(recv_frame(&mut rx), b"one");
        assert_eq!(recv_frame(&mut rx), b"two-is-longer");
    }

    #[test]
    fn tcp_refuses_an_oversized_length_prefix() {
        // A peer that claims a record far larger than any MTU is corrupt or hostile;
        // the framer must drop what it holds rather than buffer toward that claim.
        let (mut raw, server) = stream_pair();
        let mut rx = TcpPort::from_stream(server, 1200).unwrap();

        raw.write_all(&u32::MAX.to_be_bytes()).unwrap();
        raw.write_all(b"garbage that will never reach the claimed length").unwrap();
        std::thread::sleep(Duration::from_millis(30));

        assert!(rx.try_recv().is_none(), "an impossible length is refused");
        assert!(rx.inbuf.is_empty(), "and the untrusted buffer is dropped, not grown");
    }
}
