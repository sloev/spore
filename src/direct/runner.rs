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

use super::{
    Answering, AnyPort, Candidate, Medium, Need, Pipe, RecordType, Reject, Signal, Signalling, UdpPort,
};
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

/// How a pipe's socket was actually established.
///
/// This exists because "the punch worked" and "the punch never ran" produced an
/// identical result: a working pipe on a LAN, where the plain-connect fallback is
/// correct anyway. That indistinguishability hid two bugs (#101, #103). A pipe now
/// says which it got.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Via {
    /// A hole was punched: the peer's probe arrived, so the path is proven open
    /// in both directions before a single record is sent.
    Punched,
    /// The punch did not land and a plain connect was used instead.
    ///
    /// **Not automatically a failure.** For a candidate that already routes — LAN,
    /// global IPv6, a declared overlay — there was nothing to punch and this is
    /// the normal, correct outcome. For a reflexive candidate it means there is
    /// no path, and records will not arrive however healthy the pipe looks.
    FellBack,
}

impl std::fmt::Display for Via {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Via::Punched => "punched",
            Via::FellBack => "no punch, plain connect",
        })
    }
}

/// What just happened, for the runtime to surface however it surfaces things —
/// a daemon logs it, an app puts it on screen. The runner never prints.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// Not Direct signalling. The payload is the app's, untouched.
    NotSignal,
    /// Our end is up. `via` says whether the socket was punched open or fell
    /// back — see [`Via`].
    PipeUp { peer: Addr, pipe_id: [u8; 16], via: Via },
    /// We accepted an offer and the ANSWER is ready to go. The underlay is *not*
    /// open yet: send the bytes, then call [`UdpRunner::settle`], which opens it
    /// and reports [`Event::PipeUp`]. The gap is deliberate — it is what lets our
    /// punch window overlap the peer's instead of preceding it.
    Answering { peer: Addr, pipe_id: [u8; 16] },
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
    /// Boxed, so a UDP pipe and an iroh pipe can live in one map — the medium
    /// list is open, so the type holding them cannot be closed.
    pipes: HashMap<[u8; 16], Pipe<AnyPort>>,
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
    /// Locators the operator declared, e.g. an address on an IP-layer overlay.
    /// Not discoverable: a routing probe follows the default route, so it never
    /// picks an overlay's source address, and an overlay address may sit in a
    /// range a public-internet check rightly rejects (cjdns uses `fc00::/8`).
    extra: Vec<String>,
    /// This host's global IPv6, if it has one — an address with no NAT in front
    /// of it, so it needs neither discovery nor a punch.
    global_v6: Option<String>,
    /// An iroh endpoint, if the runtime supplied one. The core does not build it:
    /// creating an endpoint is async, needs a tokio runtime and talks to relays —
    /// all things a runtime owns and the core does not. Supplying it is the same
    /// bargain as `SpillBackend`, and a build without `bridge-iroh` simply never
    /// has one to supply.
    #[cfg(feature = "bridge-iroh")]
    iroh: Option<(std::sync::Arc<tokio::runtime::Runtime>, ::iroh::Endpoint)>,
    /// How the most recent pipe was established. Kept so a runtime with only a
    /// status line — Android — can surface it too; loud only in the daemon log
    /// would be loud only where the problem was already known.
    last_via: Option<Via>,
    /// The pipe id currently being punched for. Probes are demultiplexed by it,
    /// so it has to be in hand before [`Self::open`] runs.
    punching: [u8; 16],
    /// A pipe we have answered but not yet opened the underlay for.
    ///
    /// This is the whole punch fix. Opening blocks for a punch window, and the
    /// initiator does not begin punching until our ANSWER reaches it — so
    /// opening before answering made the two windows disjoint *by construction*
    /// and the punch could never land, only time out into a plain connect.
    /// Answering first parks the keyed half here; [`Self::settle`] opens it once
    /// the caller has put the ANSWER on the mesh.
    pending: Option<Pending>,
}

/// A pipe answered but not yet carried — see [`UdpRunner::settle`].
struct Pending {
    peer: Addr,
    answering: Answering,
    chosen: Candidate,
    target: Vec<u8>,
}

impl UdpRunner {
    pub fn new(me: Addr, advertise: impl Into<String>, bind_port: u16) -> UdpRunner {
        UdpRunner {
            sig: Signalling::new(me),
            pipes: HashMap::new(),
            advertise: advertise.into(),
            bind_port,
            reflexive: None,
            extra: Vec::new(),
            last_via: None,
            #[cfg(feature = "bridge-iroh")]
            iroh: None,
            global_v6: crate::bridge::udp::primary_ipv6().map(|a| a.to_string()),
            punching: [0u8; 16],
            pending: None,
        }
    }

    /// The mediums this runtime can open. UDP alone — neither native runtime has
    /// a BLE or ESP-NOW adapter behind `DatagramPort`, and declaring one would be
    /// a candidate the peer could never reach.
    fn willing(&self) -> Vec<Medium> {
        // `mut` is only used under `bridge-iroh`; without it this is a one-element
        // list and the compiler is right to say so.
        #[allow(unused_mut)]
        let mut w = vec![Medium::udp()];
        #[cfg(feature = "bridge-iroh")]
        if self.iroh.is_some() {
            w.push(Medium::new(super::iroh::MEDIUM));
        }
        w
    }

    fn candidates(&self) -> Vec<Candidate> {
        let mut out = vec![Candidate {
            medium: Medium::udp(),
            locator: self.advertise.clone().into_bytes(),
            est_bps: 2_000_000,
            mtu: UDP_MTU,
            rtt_hint_ms: 15,
        }];
        // A global IPv6 goes second: it has no NAT in front of it, so unlike the
        // reflexive locator it is already the address a peer dials — no discovery,
        // no punch, no relay. Ranked just behind the LAN path (which is still
        // fewer hops when both ends are on one network) and well ahead of
        // reflexive, because `choose` breaks ties by latency hint and this is the
        // one WAN path that needs nothing built to work.
        //
        // Only offered if the host actually has one. A firewall may still drop
        // unsolicited inbound, which is a pinhole a punch can open rather than a
        // mapping that must first be discovered — better odds, not a promise.
        if let Some(v6) = &self.global_v6 {
            out.push(Candidate {
                medium: Medium::udp(),
                locator: format!("[{v6}]:{}", self.bind_port).into_bytes(),
                est_bps: 2_000_000,
                mtu: UDP_MTU,
                rtt_hint_ms: 25,
            });
        }
        // Declared locators sit between: an IP-layer overlay (Yggdrasil, cjdns, a
        // VPN) already routes, so like v6 it needs no traversal — but it is
        // usually several hops of someone else's network, so it should not
        // outrank a v6 address that goes direct.
        for e in &self.extra {
            out.push(Candidate {
                medium: Medium::udp(),
                locator: e.clone().into_bytes(),
                est_bps: 2_000_000,
                mtu: UDP_MTU,
                rtt_hint_ms: 40,
            });
        }
        // iroh goes last of the routable ones and worse than reflexive on the
        // hint, because it is the fallback of fallbacks: it may punch, but it may
        // also relay, and a relayed path is not one hop. Offered only when the
        // runtime actually supplied an endpoint.
        #[cfg(feature = "bridge-iroh")]
        if let Some((_, ep)) = &self.iroh {
            out.push(Candidate {
                medium: Medium::new(super::iroh::MEDIUM),
                locator: ep.id().to_string().into_bytes(),
                est_bps: 2_000_000,
                mtu: UDP_MTU,
                rtt_hint_ms: 90,
            });
        }
        // The reflexive locator goes last, and is ranked worst on purpose: it is
        // the only one that needs a hole punched before it can carry anything.
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

    /// Override the global IPv6 this node offers.
    ///
    /// Discovered from the host at construction, but a runtime may know better —
    /// Android learns its addresses from the connectivity manager — and a test
    /// must be able to pin it, since otherwise what a node offers depends on
    /// whether the machine running the suite happens to have IPv6.
    pub fn set_global_v6(&mut self, v6: Option<std::net::Ipv6Addr>) {
        self.global_v6 = v6.map(|a| a.to_string());
    }

    /// Supply an iroh endpoint, making `iroh` one of the mediums this node can
    /// open — P-Direct-NAT's last rung, for when no LAN, IPv6 or overlay path
    /// exists and both ends are behind NATs.
    ///
    /// Until this is called the medium is simply absent from `willing`, so a peer
    /// offering an iroh candidate is declined honestly rather than accepted and
    /// then found unopenable.
    #[cfg(feature = "bridge-iroh")]
    pub fn set_iroh(&mut self, rt: std::sync::Arc<tokio::runtime::Runtime>, ep: ::iroh::Endpoint) {
        self.iroh = Some((rt, ep));
    }

    /// Declare extra locators to offer — an overlay address, a VPN address, or
    /// anything else this node is reachable at that cannot be discovered.
    ///
    /// They must be **UDP-dialable**: an IP-layer overlay qualifies, a Tor
    /// `.onion` or I2P `.b32` does not, since those are stream rendezvous names
    /// and would need their own medium and adapter rather than a locator.
    pub fn set_extra(&mut self, extra: Vec<String>) {
        self.extra = extra;
    }

    /// The iroh endpoint id being offered as a candidate, if any.
    #[cfg(feature = "bridge-iroh")]
    pub fn iroh_id(&self) -> Option<String> {
        self.iroh.as_ref().map(|(_, ep)| ep.id().to_string())
    }

    /// The extra locators currently being offered.
    pub fn extra(&self) -> &[String] {
        &self.extra
    }

    /// The global IPv6 currently being offered, if any.
    pub fn global_v6(&self) -> Option<&str> {
        self.global_v6.as_deref()
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
    fn open(&self, chosen: &Candidate, target: &[u8], local_port: u16) -> Option<(AnyPort, Via)> {
        #[cfg(feature = "bridge-iroh")]
        if chosen.medium == *super::iroh::MEDIUM {
            return self.open_iroh(chosen, target);
        }
        if chosen.medium != *Medium::UDP {
            return None;
        }
        let peer = String::from_utf8(target.to_vec()).ok()?;
        let mtu = chosen.mtu as usize;
        let boxed = |p: UdpPort| Box::new(p) as AnyPort;
        match super::punch::punch(local_port, peer.as_str(), &self.punching, super::punch::WINDOW) {
            Ok(sock) => UdpPort::from_socket(sock, mtu).ok().map(|p| (boxed(p), Via::Punched)),
            Err(_) => UdpPort::connect(("0.0.0.0", local_port), peer.as_str(), mtu)
                .ok()
                .map(|p| (boxed(p), Via::FellBack)),
        }
    }

    /// Dial the peer's iroh endpoint and wrap the connection as a port.
    ///
    /// iroh does its own punching and, failing that, relays — which is the whole
    /// reason this rung exists. `Via` reports which: a relayed path works but is
    /// not one hop, and the relay sees ciphertext, volume and timing.
    #[cfg(feature = "bridge-iroh")]
    fn open_iroh(&self, chosen: &Candidate, target: &[u8]) -> Option<(AnyPort, Via)> {
        let (rt, ep) = self.iroh.as_ref()?;
        let id: ::iroh::EndpointId = String::from_utf8(target.to_vec()).ok()?.parse().ok()?;
        let conn = rt.block_on(ep.connect(id, super::iroh::ALPN)).ok()?;
        let port = super::iroh::IrohPort::new(rt.clone(), conn, chosen.mtu as usize);
        // A relayed connection is reported as a fallback, not a punch: it carries
        // traffic, but claiming "punched" for a path through someone else's relay
        // is exactly the over-claim this type exists to prevent.
        let via = if port.is_relayed() { Via::FellBack } else { Via::Punched };
        Some((Box::new(port) as AnyPort, via))
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
                let chosen = accepted.chosen.clone();
                let me = self.sig.me();
                let local = self.local_locator();
                // Answer *now*, open on the next `settle`. The initiator cannot
                // start punching until this reaches it, so opening here would put
                // our punch window entirely before theirs.
                let (answer, answering) = accepted.answer(me, local.as_bytes());
                self.pending = Some(Pending { peer, answering, chosen, target });
                (Some(Outbound { peer, bytes: answer }), Event::Answering { peer, pipe_id })
            }
            Signal::Answered { peer, pending, answer, chosen, dial } => {
                self.punching = pending.pipe_id();
                let Some((port, via)) = self.open(&chosen, &dial, self.bind_port) else {
                    return (None, Event::CannotOpen { peer, medium: chosen.medium });
                };
                match Pipe::finish(pending, &answer, port) {
                    Some(pipe) => {
                        let pipe_id = pipe.pipe_id();
                        self.pipes.insert(pipe_id, pipe);
                        self.last_via = Some(via);
                        (None, Event::PipeUp { peer, pipe_id, via })
                    }
                    None => (None, Event::BadAnswer { peer }),
                }
            }
        }
    }

    /// Open the underlay for a pipe we have already answered.
    ///
    /// Call it **immediately after putting the ANSWER on the mesh** — that is the
    /// entire point of the split. For a punched candidate this blocks for a punch
    /// window, during which the peer, having just received the ANSWER, is
    /// punching back. `None` if nothing was pending.
    ///
    /// [`Self::poll`] calls this too, so a runtime that forgets still completes
    /// its pipes — a beat later, and with the punch window shifted by however long
    /// it took to get round to polling. Late is survivable; never is not.
    pub fn settle(&mut self) -> Option<Event> {
        let Pending { peer, answering, chosen, target } = self.pending.take()?;
        let pipe_id = answering.pipe_id();
        self.punching = pipe_id;
        // The responder binds its advertised port too, so the locator it sent
        // back is one the initiator can actually dial — and so the punch happens
        // on that mapping rather than an ephemeral one.
        let Some((port, via)) = self.open(&chosen, &target, self.bind_port) else {
            return Some(Event::CannotOpen { peer, medium: chosen.medium });
        };
        self.pipes.insert(pipe_id, answering.over(port));
        self.last_via = Some(via);
        Some(Event::PipeUp { peer, pipe_id, via })
    }

    /// Drain every open pipe. The caller decides what a record means; nothing
    /// above the pipe exists in either native runtime yet.
    ///
    /// Settles first: a pipe answered but never opened would otherwise sit
    /// invisible in a runtime that only polls.
    pub fn poll(&mut self) -> Vec<([u8; 16], RecordType, Vec<u8>)> {
        let _ = self.settle();
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

    /// How the most recent pipe was established, if there has been one.
    pub fn last_via(&self) -> Option<Via> {
        self.last_via
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
        // Pin it: otherwise this asserts something about the machine running the
        // suite rather than about the code.
        r.set_global_v6(None);
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

#[cfg(test)]
mod carriage_tests {
    use super::*;
    use std::net::UdpSocket;

    fn free_port() -> u16 {
        UdpSocket::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
    }

    /// The test whose absence hid the ANSWER-locator bug: not "did both ends
    /// agree keys" but "did a record actually arrive".
    ///
    /// Two runners over real sockets, the SPDR bytes carried between them by
    /// hand the way a runtime carries them over the mesh.
    /// A global IPv6 has no NAT in front of it, so it must outrank the reflexive
    /// locator — that one needs a hole punched before it carries anything.
    #[test]
    fn a_global_ipv6_outranks_the_reflexive_locator() {
        let mut r = UdpRunner::new([1u8; 8], "192.168.1.10:7500", 7500);
        r.set_global_v6(Some("2001:db8::42".parse().unwrap()));
        r.set_reflexive(Some("203.0.113.9:41234".parse().unwrap()));

        let c = r.candidates();
        assert_eq!(c.len(), 3);
        assert_eq!(c[0].locator, b"192.168.1.10:7500".to_vec(), "LAN first");
        assert_eq!(c[1].locator, b"[2001:db8::42]:7500".to_vec(), "then v6, bracketed for a port");
        assert_eq!(c[2].locator, b"203.0.113.9:41234".to_vec(), "reflexive last");
        assert!(
            c[1].rtt_hint_ms < c[2].rtt_hint_ms,
            "an address needing no punch must outrank one that does"
        );

        // `choose` ranks by that hint, so a peer that can use both picks v6 over
        // a locator whose path does not exist yet.
        let offer = super::super::Offer {
            pipe_id: [0u8; 16],
            from: [1u8; 8],
            to: [2u8; 8],
            need: Need { min_bps: 1_000, mtu_needed: 64, max_latency_ms: None },
            eph_pub: [0u8; 32],
            candidates: vec![c[2].clone(), c[1].clone()], // reflexive offered first
        };
        let picked = super::super::choose(&offer, &[Medium::udp()]).unwrap();
        assert_eq!(picked.locator, b"[2001:db8::42]:7500".to_vec(), "order offered must not decide it");

        r.set_global_v6(None);
        assert_eq!(r.candidates().len(), 2, "a host without global v6 offers none");
    }

    /// The whole ladder, in the order `choose` will walk it: paths that already
    /// route come before the one that still has to be punched open.
    #[test]
    fn declared_overlay_locators_rank_between_ipv6_and_reflexive() {
        let mut r = UdpRunner::new([1u8; 8], "192.168.1.10:7500", 7500);
        r.set_global_v6(Some("2001:db8::42".parse().unwrap()));
        r.set_extra(vec!["[200:abcd::1]:7500".into(), "10.8.0.3:7500".into()]);
        r.set_reflexive(Some("203.0.113.9:41234".parse().unwrap()));

        let c = r.candidates();
        let hints: Vec<u16> = c.iter().map(|x| x.rtt_hint_ms).collect();
        assert_eq!(c.len(), 5, "LAN + v6 + two declared + reflexive");
        assert!(hints.windows(2).all(|w| w[0] <= w[1]), "emitted worst-last: {hints:?}");
        assert_eq!(c[2].locator, b"[200:abcd::1]:7500".to_vec());
        assert_eq!(c[3].locator, b"10.8.0.3:7500".to_vec());
        assert!(
            c[2].rtt_hint_ms > c[1].rtt_hint_ms,
            "an overlay is several hops of someone else's network; direct v6 wins"
        );
        assert!(c[2].rtt_hint_ms < c[4].rtt_hint_ms, "but it already routes, unlike the reflexive locator");

        // Offered in reverse, `choose` still picks the LAN path.
        let mut reversed = c.clone();
        reversed.reverse();
        let offer = super::super::Offer {
            pipe_id: [0u8; 16],
            from: [1u8; 8],
            to: [2u8; 8],
            need: Need { min_bps: 1_000, mtu_needed: 64, max_latency_ms: None },
            eph_pub: [0u8; 32],
            candidates: reversed,
        };
        let picked = super::super::choose(&offer, &[Medium::udp()]).unwrap();
        assert_eq!(picked.locator, b"192.168.1.10:7500".to_vec());
    }

    /// A medium with nothing behind it must not be declared. Without an endpoint
    /// supplied by the runtime, `iroh` is absent from both the offer and the
    /// willing set — so a peer offering one is declined with a reason rather than
    /// accepted and then found unopenable.
    #[test]
    fn iroh_is_absent_until_a_runtime_supplies_an_endpoint() {
        let r = UdpRunner::new([1u8; 8], "192.168.1.10:7500", 7500);
        assert!(!r.candidates().iter().any(|c| c.medium == *"iroh"), "no endpoint, no candidate");
        assert!(!r.willing().iter().any(|m| *m == *"iroh"), "and nothing declared we cannot open");

        // An offer of only iroh is therefore declined, not silently accepted.
        let mut b = UdpRunner::new([2u8; 8], "192.168.1.11:7501", 7501);
        let offer = super::super::Offer {
            pipe_id: [1u8; 16],
            from: [1u8; 8],
            to: [2u8; 8],
            need: Need { min_bps: 1_000, mtu_needed: 64, max_latency_ms: None },
            eph_pub: [0u8; 32],
            candidates: vec![Candidate {
                medium: Medium::new("iroh"),
                locator: b"some-endpoint-id".to_vec(),
                est_bps: 1_000_000,
                mtu: 1200,
                rtt_hint_ms: 90,
            }],
        };
        let (out, ev) = b.on_plaintext([1u8; 8], &offer.encode(), 1_700_000_000);
        assert!(matches!(ev, Event::Declined { .. }), "declined honestly: {ev:?}");
        assert!(out.is_some(), "and the peer is told why, rather than left waiting");
    }

    #[test]
    fn a_record_crosses_between_two_runners_over_real_sockets() {
        let (a_addr, b_addr) = ([0xA1u8; 8], [0xB2u8; 8]);
        let (a_port, b_port) = (free_port(), free_port());
        let mut a = UdpRunner::new(a_addr, format!("127.0.0.1:{a_port}"), a_port);
        let mut b = UdpRunner::new(b_addr, format!("127.0.0.1:{b_port}"), b_port);

        // A offers; B answers. In a runtime these bytes ride send_direct.
        let (offer, _) = a.offer(b_addr, 1_700_000_000);
        let (answer, ev) = b.on_plaintext(a_addr, &offer.bytes, 1_700_000_000);
        assert!(
            matches!(ev, Event::Answering { .. }),
            "B answers before it opens — that ordering is the punch fix: {ev:?}"
        );
        let answer = answer.expect("B replies with an ANSWER");

        // Both ends now punch *at the same time*, which is the entire point and
        // is why this needs two threads: each `punch` blocks for its window, and
        // two blocking windows on one thread are by definition disjoint. Two
        // processes get this for free; a test has to ask for it.
        //
        // Before the fix, B opened inside `on_plaintext` — so B's window ran and
        // timed out *before* the ANSWER was even sent, A's began afterwards, and
        // no overlap was possible however long either waited. Both ends fell back
        // to a plain connect, which still carries records on loopback, which is
        // exactly why the bug survived a test that only checked that bytes moved.
        let settling = std::thread::spawn(move || {
            let ev = b.settle().expect("B had a pipe pending");
            (b, ev)
        });
        let (none, a_ev) = a.on_plaintext(b_addr, &answer.bytes, 1_700_000_000);
        let (mut b, b_ev) = settling.join().expect("B's punch thread");

        assert!(none.is_none(), "an answer needs no reply");
        assert!(
            matches!(a_ev, Event::PipeUp { via: Via::Punched, .. }),
            "A's end must be punched open, not fallen back to a plain connect: {a_ev:?}"
        );
        assert!(
            matches!(b_ev, Event::PipeUp { via: Via::Punched, .. }),
            "B's end must be punched open too — one-sided success is the failure \
             mode TAIL_PROBES exists to prevent: {b_ev:?}"
        );

        // The part that was never checked before: bytes, not key agreement.
        let a_pipe = a.pipes.values_mut().next().expect("A holds a pipe");
        a_pipe.send(RecordType::Data, b"north pier at midnight").unwrap();

        let mut got = None;
        for _ in 0..200 {
            for (_, ty, bytes) in b.poll() {
                if ty == RecordType::Data {
                    got = Some(bytes);
                }
            }
            if got.is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(
            got.as_deref(),
            Some(&b"north pier at midnight"[..]),
            "a record must actually arrive — agreeing keys is not carriage"
        );
    }
}
