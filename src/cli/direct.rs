//! The daemon's Direct surface — a thin adapter over `direct::UdpRunner`.
//!
//! The runner holds the negotiation and the sockets; this file supplies only
//! what a daemon alone can: the mesh send, and turning an [`Event`] into a line
//! on stderr. Android drives the same runner through its JNI handle, so the two
//! native runtimes cannot drift into their own versions of the protocol.
//!
//! Candidates are the address the node was told to advertise, plus — if a
//! reflexive echo was named — the address that echo says it sees. Discovering
//! the latter is P-Direct-NAT step 2 and is real; *connecting* over it is step 3
//! (the coordinated punch) and is not built, so a pipe that has to cross a NAT
//! still usually fails. The daemon says which locators it is offering rather
//! than implying either one works.
//!
//! Binary-crate module; no wire contract here.

use spore::bridge::hub::{now, Shared};
use spore::direct::{Event, Outbound, RecordType, UdpRunner};
use spore::{addr_of, fl, ty, Addr, Envelope, Src};

fn hex8(a: &Addr) -> String {
    a.iter().map(|b| format!("{b:02x}")).collect()
}

fn hex4(id: &[u8; 16]) -> String {
    id[..4].iter().map(|b| format!("{b:02x}")).collect()
}

pub(crate) struct Direct {
    run: UdpRunner,
    bind_port: u16,
}

impl Direct {
    pub(crate) fn new(me: Addr, advertise: String, bind_port: u16, also: Vec<String>) -> Direct {
        let mut run = UdpRunner::new(me, advertise, bind_port);
        run.set_extra(also);
        Direct { run, bind_port }
    }

    /// The locators this node is offering, for the operator to see. A path that
    /// is not listed is not on offer — the point is that "why did it not connect"
    /// is answerable without a packet capture.
    pub(crate) fn offering(&self) -> String {
        let mut v = vec![format!("{} (LAN)", self.run.advertise())];
        if let Some(v6) = self.run.global_v6() {
            v.push(format!("[{v6}]:{} (IPv6, no NAT)", self.bind_port));
        }
        for e in self.run.extra() {
            v.push(format!("{e} (declared)"));
        }
        if let Some(r) = self.run.reflexive() {
            v.push(format!("{r} (reflexive, needs a punch)"));
        }
        #[cfg(feature = "bridge-iroh")]
        if let Some(id) = self.run.iroh_id() {
            v.push(format!("{} (iroh, may relay)", &id[..16.min(id.len())]));
        }
        v.join(" · ")
    }

    /// Stand up an iroh endpoint and offer `iroh` as a medium.
    ///
    /// `relay` picks the trust posture, and it is deliberately not defaulted:
    /// `"direct-only"` runs with no relay and no discovery, so nothing is
    /// disclosed to anyone but the peer; `"n0"` opts into n0's public relay,
    /// which is a real third-party disclosure — that relay sees ciphertext,
    /// volume and timing — and so has to be chosen rather than inherited.
    #[cfg(feature = "bridge-iroh")]
    pub(crate) fn enable_iroh(&mut self, relay: &str) -> Option<String> {
        use iroh::{endpoint::presets, Endpoint, RelayMode};
        let rt = std::sync::Arc::new(tokio::runtime::Builder::new_multi_thread().enable_all().build().ok()?);
        let direct_only = relay != "n0";
        let ep = rt
            .block_on(async {
                let b = if direct_only {
                    Endpoint::builder(presets::Minimal).relay_mode(RelayMode::Disabled)
                } else {
                    Endpoint::builder(presets::N0)
                };
                b.alpns(vec![spore::direct::iroh::ALPN.to_vec()]).bind().await
            })
            .ok()?;
        let id = ep.id().to_string();
        self.run.set_iroh(rt, ep);
        Some(id)
    }

    /// Ask a reflexive echo where we appear to be, and offer that too.
    ///
    /// The probe binds the port we advertise, so the mapping described is the one
    /// a peer would dial. It is a *candidate*, not a working path: most NATs drop
    /// an unsolicited inbound datagram, which is what the coordinated punch
    /// (step 3) exists to fix. Offering it costs nothing and `choose` still
    /// prefers the LAN locator.
    pub(crate) fn learn_reflexive(&mut self, server: &str) -> Option<std::net::SocketAddr> {
        let sock = std::net::UdpSocket::bind(("0.0.0.0", self.bind_port)).ok()?;
        let seen = spore::direct::stun::reflexive(&sock, server, std::time::Duration::from_secs(2)).ok()?;
        self.run.set_reflexive(Some(seen));
        Some(seen)
    }

    /// Keep a pipe up to `peer`, if one was configured.
    ///
    /// Re-offers rather than offering once at startup: at boot the peer's
    /// ANNOUNCE has usually not arrived, so the first offer would go out
    /// cleartext to an unknown prekey — and an offer nobody answers simply
    /// expires, which is the signal to try again.
    pub(crate) fn maintain(&mut self, hub: &Shared, peer: Addr) {
        let (pipes, pending) = self.run.status();
        if pipes > 0 || pending > 0 {
            return;
        }
        let (out, pipe_id) = self.run.offer(peer, now());
        eprintln!("  [direct] OFFER → {} · pipe {}", hex8(&peer), hex4(&pipe_id));
        self.send(hub, out);
    }

    /// Interpret one envelope the router delivered to us. `true` if it was Direct
    /// signalling — an ordinary app message is left entirely alone.
    pub(crate) fn on_delivered(&mut self, hub: &Shared, wire: &[u8]) -> bool {
        let Ok((e, _)) = Envelope::decode(wire) else { return false };
        if e.typ != ty::DATA {
            return false;
        }
        // Only a signed envelope names its sender provably, and the negotiation
        // binds to that identity — an unsigned `src` is a claim, not a peer.
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

        let (out, event) = self.run.on_plaintext(peer, &plaintext, now());
        if let Some(o) = out {
            self.send(hub, o);
        }
        self.report(&event);
        // The ANSWER is on the mesh now, so the peer is about to start punching:
        // this is the moment to punch back, and it is why opening is not done
        // inside `on_plaintext`. Blocks for a punch window on a punched candidate.
        if matches!(event, Event::Answering { .. }) {
            if let Some(settled) = self.run.settle() {
                self.report(&settled);
            }
        }
        !matches!(event, Event::NotSignal)
    }

    fn report(&self, event: &Event) {
        match event {
            Event::NotSignal => {}
            Event::Answering { peer, pipe_id } => {
                eprintln!("  [direct] answered {} for pipe {} — opening", hex8(peer), hex4(pipe_id))
            }
            Event::PipeUp { peer, pipe_id, via } => {
                eprintln!("  [direct] pipe {} up with {} ({via})", hex4(pipe_id), hex8(peer))
            }
            Event::Declined { peer } => {
                eprintln!("  [direct] declined an offer from {} — no medium in common", hex8(peer))
            }
            Event::Refused { peer, reason } => {
                eprintln!("  [direct] {} refused: {reason:?}", hex8(peer))
            }
            Event::CannotOpen { peer, medium } => {
                eprintln!("  [direct] cannot open {medium} for {}", hex8(peer))
            }
            Event::BadAnswer { peer } => {
                eprintln!("  [direct] {} sent an answer that did not verify", hex8(peer))
            }
        }
    }

    fn send(&self, hub: &Shared, out: Outbound) {
        let forwards = hub.with_node(|n| {
            let (_, f, _) = n.send_direct(out.peer, &out.bytes, now());
            f
        });
        hub.originate(forwards);
    }

    /// Drain open pipes. Nothing consumes Direct traffic in the daemon yet —
    /// there is no app on top — so records are logged and dropped rather than
    /// pretending to route somewhere.
    pub(crate) fn poll(&mut self) {
        for (id, ty, bytes) in self.run.poll() {
            if ty != RecordType::Keepalive {
                eprintln!("  [direct] pipe {} ← {:?} ({} bytes)", hex4(&id), ty, bytes.len());
            }
        }
    }

    pub(crate) fn expire(&mut self) {
        let dropped = self.run.expire(now());
        if dropped > 0 {
            eprintln!("  [direct] {dropped} unanswered offer(s) expired");
        }
    }
}

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

/// Work out what to advertise from the config value: either an explicit
/// `ip:port`, or a bare port to be paired with this host's primary IPv4.
///
/// `None` when only a port was given and no primary IPv4 could be found: a node
/// that cannot say where it is has nothing to offer, and advertising `0.0.0.0`
/// would produce a candidate that can never be dialled.
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
