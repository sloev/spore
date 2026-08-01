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
    /// The LAN locator handed out in offers, as `ip:port`. Configured, not
    /// guessed — a node never invents an address for itself.
    advertise: String,
    /// Bound when *initiating*, so an answering peer can reach the locator above.
    /// A responder dials out from an ephemeral port instead, being the one that
    /// sends first.
    bind_port: u16,
    /// Where a STUN echo last said we appear to be, as `ip:port`. `None` until
    /// asked — a node never guesses its own reflexive address.
    reflexive: Option<String>,
    /// The pipe id currently being punched for. Probes are demultiplexed by it,
    /// so it has to be in hand before [`Self::open`] runs.
    punching: [u8; 16],
}

impl UdpRunner {
    pub fn new(me: Addr, advertise: impl Into<String>, bind_port: u16) -> UdpRunner {
        UdpRunner {
            sig: Signalling::new(me),
            pipes: HashMap::new(),
            advertise: advertise.into(),
            bind_port,
            reflexive: None,
            punching: [0u8; 16],
        }
    }

    /// The mediums this runtime can open. UDP alone — neither native runtime has
    /// a BLE or ESP-NOW adapter behind `DatagramPort`, and declaring one would be
    /// a candidate the peer could never reach.
    fn willing(&self) -> [Medium; 1] {
        [Medium::udp()]
    }

    fn candidates(&self) -> Vec<Candidate> {
        let mut out = vec![Candidate {
            medium: Medium::udp(),
            locator: self.advertise.clone().into_bytes(),
            est_bps: 2_000_000,
            mtu: UDP_MTU,
            rtt_hint_ms: 15,
        }];
        // The reflexive locator goes second, and is ranked worse on purpose: a
        // LAN path that works is always preferable to one that has to survive a
        // NAT, and `choose` breaks ties by latency hint.
        if let Some(r) = &self.reflexive {
            out.push(Candidate {
                medium: Medium::udp(),
                locator: r.clone().into_bytes(),
                est_bps: 2_000_000,
                mtu: UDP_MTU,
                rtt_hint_ms: 60,
            });
        }
        out
    }

    /// Record where a STUN echo says this node appears to be, so offers carry a
    /// candidate a peer outside the LAN could dial.
    ///
    /// **Discovering the address is not the same as the path working.** Without
    /// the coordinated punch (P-Direct-NAT step 3) an inbound datagram to that
    /// mapping is dropped by most NATs, and the mapping itself belongs to the
    /// socket that asked — so a candidate learned on one socket and dialled on
    /// another describes a binding that may no longer exist. This offers the
    /// locator honestly and lets `choose` prefer the LAN path; making it
    /// *connect* is step 3's job, which is where the socket-sharing design
    /// belongs rather than half-built here.
    pub fn set_reflexive(&mut self, seen: Option<std::net::SocketAddr>) {
        self.reflexive = seen.map(|a| a.to_string());
    }

    /// The locator this node hands to a peer as its own — the reflexive one when
    /// known, since that is the address a peer outside the LAN has to dial.
    fn local_locator(&self) -> String {
        self.reflexive.clone().unwrap_or_else(|| self.advertise.clone())
    }

    /// The reflexive locator currently being offered, if any.
    pub fn reflexive(&self) -> Option<&str> {
        self.reflexive.as_deref()
    }

    /// Open our end for a chosen candidate, dialling `target`.
    ///
    /// `target` is passed explicitly rather than read from `chosen.locator`,
    /// because they are not the same address: `chosen` is always one of the
    /// *initiator's* candidates, so the initiator must dial the responder's
    /// locator from the ANSWER instead of its own.
    ///
    /// Punches first (P-Direct-NAT step 3) and keeps the socket that punched — a
    /// NAT mapping belongs to a source port, so the pipe has to run on that exact
    /// socket. If the punch does not land we fall back to a plain connect: on a
    /// LAN there is no NAT to traverse and nothing was needed, and off a LAN the
    /// records will simply not arrive, which the caller reports honestly rather
    /// than the pipe pretending otherwise.
    fn open(&self, chosen: &Candidate, target: &[u8], local_port: u16) -> Option<UdpPort> {
        if chosen.medium != *Medium::UDP {
            return None;
        }
        let peer = String::from_utf8(target.to_vec()).ok()?;
        let mtu = chosen.mtu as usize;
        match super::punch::punch(local_port, peer.as_str(), &self.punching, super::punch::WINDOW) {
            Ok(sock) => UdpPort::from_socket(sock, mtu).ok(),
            Err(_) => UdpPort::connect(("0.0.0.0", local_port), peer.as_str(), mtu).ok(),
        }
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
                self.punching = pipe_id;
                let target = accepted.chosen.locator.clone();
                // The responder binds its advertised port too, so the locator it
                // sends back is one the initiator can actually dial — and so the
                // punch happens on that mapping rather than an ephemeral one.
                let Some(port) = self.open(&accepted.chosen, &target, self.bind_port) else {
                    return (None, Event::CannotOpen { peer, medium: accepted.chosen.medium });
                };
                let me = self.sig.me();
                let local = self.local_locator();
                let (answer, pipe) = Pipe::answer_with(accepted, me, local.as_bytes(), port);
                self.pipes.insert(pipe_id, pipe);
                (Some(Outbound { peer, bytes: answer }), Event::PipeUp { peer, pipe_id })
            }
            Signal::Answered { peer, pending, answer, chosen, dial } => {
                self.punching = pending.pipe_id();
                let Some(port) = self.open(&chosen, &dial, self.bind_port) else {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reflexive_locator_is_offered_but_ranked_below_the_lan_one() {
        let mut r = UdpRunner::new([1u8; 8], "192.168.1.10:7500", 7500);
        assert_eq!(r.candidates().len(), 1, "only the configured locator until we ask");
        assert_eq!(r.reflexive(), None, "never guessed");

        r.set_reflexive(Some("203.0.113.9:41234".parse().unwrap()));
        let c = r.candidates();
        assert_eq!(c.len(), 2);
        assert_eq!(c[0].locator, b"192.168.1.10:7500".to_vec());
        assert_eq!(c[1].locator, b"203.0.113.9:41234".to_vec());
        assert!(
            c[1].rtt_hint_ms > c[0].rtt_hint_ms,
            "a path that has to survive a NAT must never outrank one that does not"
        );

        // `choose` ranks by that hint, so a peer willing to use both picks the LAN.
        let offer = super::super::Offer {
            pipe_id: [0u8; 16],
            from: [1u8; 8],
            to: [2u8; 8],
            need: Need { min_bps: 1_000, mtu_needed: 64, max_latency_ms: None },
            eph_pub: [0u8; 32],
            candidates: c,
        };
        let picked = super::super::choose(&offer, &[Medium::udp()]).unwrap();
        assert_eq!(picked.locator, b"192.168.1.10:7500".to_vec());

        r.set_reflexive(None);
        assert_eq!(r.candidates().len(), 1, "it can be withdrawn again");
    }
}
