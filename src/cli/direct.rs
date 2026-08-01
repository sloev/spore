//! The daemon's Direct runner — the half of PR8c that makes a pipe startable.
//!
//! `direct::Signalling` decides and this opens: the split the core landed is not
//! a nicety here, it is load-bearing. `UdpPort::connect` needs the peer's
//! locator, and the locator only exists once a candidate has been *chosen*, so
//! the old `Pipe::answer` — which took the port before choosing — could not have
//! been used by this file at all.
//!
//! What this is honestly capable of: a LAN. Candidates are the address the node
//! was told to advertise, so two daemons on the same network find each other and
//! a pipe comes up. Across NAT it will not, because reflexive discovery and
//! hole-punching are P-Direct-NAT and are not built — the daemon says so in its
//! log rather than appearing to try and silently failing.
//!
//! Binary-crate module; no wire contract here.

use spore::bridge::hub::{now, Shared};
use spore::direct::{Candidate, Medium, Need, Pipe, RecordType, Signal, Signalling, UdpPort};

use spore::{addr_of, fl, ty, Addr, Envelope, Src};
use std::collections::HashMap;

/// How long an unanswered offer is kept. A NAT binding dies in 30s–5min and the
/// candidates in an older offer are stale, so the negotiation is better restarted
/// than resumed.
const OFFER_TTL_SECS: u32 = 120;

/// The record MTU claimed for a UDP candidate — one datagram, one record, kept
/// under the usual 1500-byte Ethernet path so a pipe does not depend on
/// fragmentation surviving the network.
const UDP_MTU: u16 = 1200;

fn hex8(a: &Addr) -> String {
    a.iter().map(|b| format!("{b:02x}")).collect()
}

pub(crate) struct Direct {
    sig: Signalling,
    pipes: HashMap<[u8; 16], Pipe<UdpPort>>,
    /// The locator this node hands out, as `ip:port`. Advertised verbatim: until
    /// P-Direct-NAT lands a node cannot discover its own reflexive address, and
    /// guessing one would be the dishonest option.
    advertise: String,
    /// The port bound when *initiating*, so the answering peer can reach the
    /// locator above. A responder dials out from an ephemeral port instead.
    bind_port: u16,
}

impl Direct {
    pub(crate) fn new(me: Addr, advertise: String, bind_port: u16) -> Direct {
        Direct { sig: Signalling::new(me), pipes: HashMap::new(), advertise, bind_port }
    }

    fn candidates(&self) -> Vec<Candidate> {
        vec![Candidate {
            medium: Medium::Udp,
            locator: self.advertise.clone().into_bytes(),
            est_bps: 2_000_000,
            mtu: UDP_MTU,
            rtt_hint_ms: 15,
        }]
    }

    /// The mediums this runtime can actually open. UDP only — the daemon has no
    /// BLE or ESP-NOW radio to offer, and saying otherwise would be a control
    /// with nothing behind it.
    fn willing(&self) -> Vec<Medium> {
        vec![Medium::Udp]
    }

    /// Open our end for a candidate the negotiation chose.
    ///
    /// `local` differs by role on purpose: an initiator binds the port it
    /// advertised so the answer's traffic can arrive there, while a responder
    /// dials out and lets the kernel pick, because it is the one sending first.
    fn open(&self, chosen: &Candidate, local_port: u16) -> Option<UdpPort> {
        if chosen.medium != Medium::Udp {
            eprintln!("  [direct] chose {:?}, which this daemon cannot open", chosen.medium);
            return None;
        }
        let peer = String::from_utf8(chosen.locator.clone()).ok()?;
        match UdpPort::connect(("0.0.0.0", local_port), peer.as_str(), chosen.mtu as usize) {
            Ok(p) => Some(p),
            Err(e) => {
                eprintln!("  [direct] cannot open a pipe to {peer}: {e}");
                None
            }
        }
    }

    /// Keep a pipe up to `peer`, if one was configured.
    ///
    /// Re-offers rather than offering once at startup: at boot the peer's
    /// ANNOUNCE has usually not arrived, so the first offer would go out
    /// cleartext to an unknown prekey — and an offer that is never answered
    /// simply expires, which is the signal to try again.
    pub(crate) fn maintain(&mut self, hub: &Shared, peer: Addr) {
        if !self.pipes.is_empty() || self.sig.pending() > 0 {
            return;
        }
        self.offer(hub, peer);
    }

    /// Propose a pipe to `peer`. The OFFER rides the ordinary sealed DM path.
    pub(crate) fn offer(&mut self, hub: &Shared, peer: Addr) {
        let (bytes, pipe_id) = self.sig.offer(peer, need(), self.candidates(), now());
        let forwards = hub.with_node(|n| {
            let (_, f, _) = n.send_direct(peer, &bytes, now());
            f
        });
        hub.originate(forwards);
        eprintln!("  [direct] OFFER → {} · pipe {}", hex8(&peer), hex4(&pipe_id));
    }

    /// Interpret one envelope the router delivered to us.
    ///
    /// Returns `true` if it was Direct signalling — an ordinary app message is
    /// left alone, which is the whole reason [`direct::is_signal`] exists.
    pub(crate) fn on_delivered(&mut self, hub: &Shared, wire: &[u8]) -> bool {
        let Ok((e, _)) = Envelope::decode(wire) else { return false };
        if e.typ != ty::DATA {
            return false;
        }
        // Only a signed envelope names its sender provably, and the whole
        // negotiation binds to that identity — an unsigned claim is not a peer.
        let Src::Full(pk) = &e.src else { return false };
        if e.flags & fl::SIGNED == 0 || !e.verify() {
            return false;
        }
        let peer = addr_of(pk);

        let plaintext = if e.flags & fl::ENCRYPTED != 0 {
            let ratcheted = e.flags & fl::RATCHET != 0;
            match hub.with_node(|n| n.open_dm(peer, &e.payload, ratcheted, now())) {
                Some(p) => p,
                None => return false,
            }
        } else {
            e.payload.clone()
        };

        let willing = self.willing();
        match self.sig.on_signal(peer, &plaintext, &willing) {
            Signal::Offer { peer, accepted } => {
                let pipe_id = accepted.pipe_id();
                // Choosing happened above; only now is there a locator to dial.
                let Some(port) = self.open(&accepted.chosen, 0) else { return true };
                let (answer, pipe) = Pipe::answer_with(accepted, hub.addr(), port);
                self.reply(hub, peer, &answer);
                eprintln!("  [direct] ANSWER → {} · pipe {} up", hex8(&peer), hex4(&pipe_id));
                self.pipes.insert(pipe_id, pipe);
                true
            }
            Signal::Decline { peer, reply } => {
                self.reply(hub, peer, &reply);
                eprintln!("  [direct] declined an offer from {} — no medium in common", hex8(&peer));
                true
            }
            Signal::Answered { peer, pending, answer, chosen } => {
                let Some(port) = self.open(&chosen, self.bind_port) else { return true };
                match Pipe::finish(pending, &answer, port) {
                    Some(pipe) => {
                        eprintln!("  [direct] pipe {} up with {}", hex4(&pipe.pipe_id()), hex8(&peer));
                        self.pipes.insert(pipe.pipe_id(), pipe);
                    }
                    None => eprintln!("  [direct] {} sent an answer that did not verify", hex8(&peer)),
                }
                true
            }
            Signal::Refused { peer, reason, .. } => {
                eprintln!("  [direct] {} refused: {reason:?}", hex8(&peer));
                true
            }
            Signal::NotSignal => false,
        }
    }

    fn reply(&self, hub: &Shared, peer: Addr, bytes: &[u8]) {
        let forwards = hub.with_node(|n| {
            let (_, f, _) = n.send_direct(peer, bytes, now());
            f
        });
        hub.originate(forwards);
    }

    /// Drain every open pipe. Nothing consumes Direct traffic in the daemon yet —
    /// there is no app on top of it — so records are logged and dropped rather
    /// than pretending to route somewhere.
    pub(crate) fn poll(&mut self) {
        for (id, pipe) in self.pipes.iter_mut() {
            while let Some((ty, bytes)) = pipe.poll() {
                match ty {
                    RecordType::Keepalive => {}
                    _ => eprintln!("  [direct] pipe {} ← {:?} ({} bytes)", hex4(id), ty, bytes.len()),
                }
            }
        }
    }

    /// Drop offers nobody answered, so a daemon offering pipes to an unreachable
    /// peer does not hold their ephemeral secrets forever.
    pub(crate) fn expire(&mut self) {
        let dropped = self.sig.expire(now(), OFFER_TTL_SECS);
        if dropped > 0 {
            eprintln!("  [direct] {dropped} unanswered offer(s) expired");
        }
    }
}

fn need() -> Need {
    Need { min_bps: 5_000, mtu_needed: 64, max_latency_ms: Some(150) }
}

fn hex4(id: &[u8; 16]) -> String {
    id[..4].iter().map(|b| format!("{b:02x}")).collect()
}

/// Work out what to advertise from the config value: either an explicit
/// `ip:port`, or a bare port to be paired with this host's primary IPv4.
///
/// Returns `None` when only a port was given and no primary IPv4 could be found:
/// a node that cannot say where it is has nothing to offer, and advertising
/// `0.0.0.0` would produce a candidate that can never be dialled.
/// Parse a 16-hex-digit peer address, the form the daemon prints for itself.
pub(crate) fn peer_addr(s: &str) -> Option<Addr> {
    let s = s.trim();
    if s.len() != 16 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let mut a = [0u8; 8];
    for (i, byte) in a.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(a)
}

pub(crate) fn locator(spec: &str) -> Option<(String, u16)> {
    if let Some((_, p)) = spec.rsplit_once(':') {
        let port = p.parse().ok()?;
        return Some((spec.to_string(), port));
    }
    let port: u16 = spec.parse().ok()?;
    let ip = spore::bridge::udp::primary_ipv4()?;
    Some((format!("{ip}:{port}"), port))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locator_takes_an_explicit_address_or_derives_one() {
        assert_eq!(locator("203.0.113.5:7500"), Some(("203.0.113.5:7500".into(), 7500)));
        // A bare port needs a primary IPv4; in a sandbox there may be none, and
        // then there is honestly nothing to advertise.
        if let Some((adv, port)) = locator("7500") {
            assert_eq!(port, 7500);
            assert!(adv.ends_with(":7500"), "derived locator carries the port: {adv}");
            assert!(!adv.starts_with("0.0.0.0"), "never advertise a wildcard as a candidate");
        }
        assert_eq!(locator("not-a-port"), None);
    }

    #[test]
    fn peer_addr_round_trips_the_form_the_daemon_prints() {
        let a: Addr = [0xa1, 0xb2, 0xc3, 0xd4, 0xe5, 0xf6, 0x07, 0x18];
        assert_eq!(peer_addr(&hex8(&a)), Some(a));
        assert_eq!(peer_addr("  a1b2c3d4e5f60718  "), Some(a), "surrounding space is config noise");

        // Anything that is not exactly eight bytes of hex is rejected rather
        // than padded or truncated into a different node's address.
        assert_eq!(peer_addr("a1b2c3d4e5f607"), None, "too short");
        assert_eq!(peer_addr("a1b2c3d4e5f6071800"), None, "too long");
        assert_eq!(peer_addr("a1b2c3d4e5f6071g"), None, "not hex");
        assert_eq!(peer_addr(""), None);
    }
}
