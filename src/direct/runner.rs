//! A UDP Direct runner both native runtimes drive identically.
//!
//! [`Signalling`](crate::direct::Signalling) decides and something has to open —
//! but "something" was about to be the daemon *and* Android, separately, which is
//! exactly the per-platform punch logic `ROADMAP.md`'s engineering pattern
//! forbids. So the opening lives here, once, and a runtime supplies only what it
//! alone can: the mesh send, the clock, and how to report what happened.
//!
//! Deliberately **not** dependent on `Node` or `Hub`. It takes the plaintext of a
//! delivered DM and hands back the bytes to send in reply, so a caller holding
//! its node behind a JNI handle and a caller holding it behind a `Mutex` are the
//! same caller here. `std::net` means no wasm; a browser's ladder ends before UDP
//! anyway (see the P-Direct-NAT track).

use super::{Candidate, Medium, Need, Pipe, RecordType, Reject, Signal, Signalling, UdpPort};
use crate::Addr;
use std::collections::HashMap;

/// The record MTU claimed for a UDP candidate — one datagram, one record, kept
/// under the usual 1500-byte path so a pipe does not depend on fragmentation.
pub const UDP_MTU: u16 = 1200;

/// How long an unanswered offer is kept. A NAT binding dies in 30s–5min, so an
/// older offer's candidates are stale and it is better restarted than resumed.
pub const OFFER_TTL_SECS: u32 = 120;

/// SPDR bytes the caller must carry to `peer` over the mesh (`send_direct`).
pub struct Outbound {
    pub peer: Addr,
    pub bytes: Vec<u8>,
}

/// What just happened, for the runtime to surface however it surfaces things —
/// a daemon logs it, an app puts it on screen. The runner never prints.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// Not Direct signalling. The payload is the app's, untouched.
    NotSignal,
    /// We answered an offer and our end is up.
    PipeUp { peer: Addr, pipe_id: [u8; 16] },
    /// We could not open the medium that won, so no pipe. The reply, if any, has
    /// already been handed back.
    CannotOpen { peer: Addr, medium: Medium },
    /// We declined — nothing offered was something we can open.
    Declined { peer: Addr },
    /// The peer refused our offer, and why.
    Refused { peer: Addr, reason: Reject },
    /// An answer arrived for a pipe we offered, but it did not verify.
    BadAnswer { peer: Addr },
}

pub struct UdpRunner {
    sig: Signalling,
    pipes: HashMap<[u8; 16], Pipe<UdpPort>>,
    /// The locator handed out in offers, as `ip:port`. Advertised verbatim: a
    /// node cannot yet discover its own reflexive address, and guessing one is
    /// the dishonest option.
    advertise: String,
    /// Bound when *initiating*, so an answering peer can reach the locator above.
    /// A responder dials out from an ephemeral port instead, being the one that
    /// sends first.
    bind_port: u16,
}

impl UdpRunner {
    pub fn new(me: Addr, advertise: impl Into<String>, bind_port: u16) -> UdpRunner {
        UdpRunner { sig: Signalling::new(me), pipes: HashMap::new(), advertise: advertise.into(), bind_port }
    }

    /// The mediums this runtime can open. UDP alone — neither native runtime has
    /// a BLE or ESP-NOW adapter behind `DatagramPort`, and declaring one would be
    /// a candidate the peer could never reach.
    fn willing(&self) -> [Medium; 1] {
        [Medium::udp()]
    }

    fn candidates(&self) -> Vec<Candidate> {
        vec![Candidate {
            medium: Medium::udp(),
            locator: self.advertise.clone().into_bytes(),
            est_bps: 2_000_000,
            mtu: UDP_MTU,
            rtt_hint_ms: 15,
        }]
    }

    fn open(&self, chosen: &Candidate, local_port: u16) -> Option<UdpPort> {
        if chosen.medium != *Medium::UDP {
            return None;
        }
        let peer = String::from_utf8(chosen.locator.clone()).ok()?;
        UdpPort::connect(("0.0.0.0", local_port), peer.as_str(), chosen.mtu as usize).ok()
    }

    /// Propose a pipe to `peer`. The caller sends the bytes over the mesh.
    pub fn offer(&mut self, peer: Addr, now: u32) -> (Outbound, [u8; 16]) {
        let need = Need { min_bps: 5_000, mtu_needed: 64, max_latency_ms: Some(150) };
        let (bytes, pipe_id) = self.sig.offer(peer, need, self.candidates(), now);
        (Outbound { peer, bytes }, pipe_id)
    }

    /// Feed the plaintext of a delivered DM from `peer`.
    ///
    /// Returns anything to send back and what happened. [`Event::NotSignal`]
    /// means the payload was an ordinary message and the caller should handle it
    /// as it always did — this never swallows app traffic.
    pub fn on_plaintext(&mut self, peer: Addr, plaintext: &[u8], now: u32) -> (Option<Outbound>, Event) {
        let _ = now;
        let willing = self.willing();
        match self.sig.on_signal(peer, plaintext, &willing) {
            Signal::NotSignal => (None, Event::NotSignal),
            Signal::Decline { peer, reply } => {
                (Some(Outbound { peer, bytes: reply }), Event::Declined { peer })
            }
            Signal::Refused { peer, reason, .. } => (None, Event::Refused { peer, reason }),
            Signal::Offer { peer, accepted } => {
                let pipe_id = accepted.pipe_id();
                // Choosing happened inside `on_signal`; only now is there a
                // locator to dial, which is why deciding and opening are split.
                let Some(port) = self.open(&accepted.chosen, 0) else {
                    return (None, Event::CannotOpen { peer, medium: accepted.chosen.medium });
                };
                let me = self.sig.me();
                let (answer, pipe) = Pipe::answer_with(accepted, me, port);
                self.pipes.insert(pipe_id, pipe);
                (Some(Outbound { peer, bytes: answer }), Event::PipeUp { peer, pipe_id })
            }
            Signal::Answered { peer, pending, answer, chosen } => {
                let Some(port) = self.open(&chosen, self.bind_port) else {
                    return (None, Event::CannotOpen { peer, medium: chosen.medium });
                };
                match Pipe::finish(pending, &answer, port) {
                    Some(pipe) => {
                        let pipe_id = pipe.pipe_id();
                        self.pipes.insert(pipe_id, pipe);
                        (None, Event::PipeUp { peer, pipe_id })
                    }
                    None => (None, Event::BadAnswer { peer }),
                }
            }
        }
    }

    /// Drain every open pipe. The caller decides what a record means; nothing
    /// above the pipe exists in either native runtime yet.
    pub fn poll(&mut self) -> Vec<([u8; 16], RecordType, Vec<u8>)> {
        let mut out = Vec::new();
        for (id, pipe) in self.pipes.iter_mut() {
            while let Some((ty, bytes)) = pipe.poll() {
                out.push((*id, ty, bytes));
            }
        }
        out
    }

    /// Drop offers nobody answered, so a runtime offering pipes to an unreachable
    /// peer does not hold their ephemeral secrets forever.
    pub fn expire(&mut self, now: u32) -> usize {
        self.sig.expire(now, OFFER_TTL_SECS)
    }

    /// Open pipes, and offers still waiting for an answer.
    pub fn status(&self) -> (usize, usize) {
        (self.pipes.len(), self.sig.pending())
    }

    /// Whether a pipe is already up. Used to avoid re-offering over a live one.
    pub fn has_pipe(&self) -> bool {
        !self.pipes.is_empty()
    }

    pub fn advertise(&self) -> &str {
        &self.advertise
    }
}
