//! QUIC peer-to-peer bridge over [iroh](https://github.com/n0-computer/iroh).
//!
//! iroh gives QUIC connections between endpoints identified by a public key, with
//! hole punching and relay fallback when a direct path can't be found. That fills
//! the gap between LAN UDP and Tor/I2P: internet-reachable peer paths without a
//! stable public IP. This bridge is a **normal SPORE bridge** — it moves envelope
//! bytes in and out of a `Node`, nothing more. The router never learns it exists.
//!
//! Envelopes are KISS-framed on one bi-directional QUIC stream, exactly like the
//! TCP/serial stream bridges, so the same [`stream_link`](super::stream_link)
//! framing and the same best-effort store-and-forward semantics apply. QUIC's
//! reliability is not leaned on above the frame: a dropped link simply reconnects
//! and the outbound queue drains when it returns.
//!
//! **Layering.** An iroh `EndpointId` is *not* a SPORE address — keep the layers
//! separate. iroh authenticates the transport peer; SPORE's own seal/sign
//! authenticates the message. A relay, if used, sees ciphertext only (envelopes
//! are already sealed) but still sees metadata and timing; that trust is documented
//! in `BRIDGES.md`. This bridge is experimental (🧪) until exercised on real NATs.
//!
//! Async lives here and nowhere else in the crate. iroh is built on tokio; the rest
//! of SPORE is synchronous and polling. The seam is a private multi-thread tokio
//! runtime whose stream halves are wrapped as blocking [`Read`]/[`Write`], so the
//! shared [`stream_link`] pump drives them without knowing they are async.

use super::hub::Shared;
use super::KissStream;
use crate::{Forward, Iface};
use std::io::{self, Read, Write};
use std::net::SocketAddr;
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::time::Duration;

use iroh::endpoint::{presets, Connection, RecvStream, SendStream};
use iroh::{Endpoint, EndpointAddr, EndpointId, RelayMode, SecretKey, TransportAddr};
use tokio::runtime::{Handle, Runtime};

/// ALPN identifying a SPORE envelope stream. A peer speaking a different protocol
/// on the same endpoint is rejected at the QUIC handshake, before any bytes.
pub const ALPN: &[u8] = b"spore/envelope/1";

/// Which end of the connection this bridge is.
pub enum Role {
    /// Dial a known peer: its `EndpointId` plus any direct socket addresses to try.
    /// With no addresses (and relay/discovery on) iroh finds the peer itself.
    Dial { peer: EndpointId, addrs: Vec<SocketAddr> },
    /// Accept the first incoming connection on our endpoint.
    Listen,
}

/// How to stand up the endpoint and who to talk to.
pub struct Config {
    /// Stable identity secret (32 bytes). `None` mints an ephemeral one — fine for
    /// a dialer, but a listener wants a stable id so peers can find it again.
    pub secret: Option<[u8; 32]>,
    /// `true` disables relay **and** discovery: direct QUIC only, no calls to the
    /// n0 infrastructure. Used for LAN/localhost and for the tests. `false` uses the
    /// n0 relay + discovery defaults so a peer behind a NAT is still reachable.
    pub direct_only: bool,
    /// Local UDP address to bind. `None` picks an ephemeral port.
    pub bind: Option<SocketAddr>,
    pub role: Role,
}

/// Turn any iroh/quinn error into an `io::Error` so the bridge speaks one error
/// type. The distinction between "connect failed" and "link dropped" is carried by
/// *where* it happens (the reconnect loop), not the error's own type.
fn ioerr<E: std::fmt::Display>(e: E) -> io::Error {
    io::Error::other(e.to_string())
}

/// The receive half of a QUIC bi stream, presented as a blocking [`Read`] by
/// driving the async read on the shared runtime. Lives in the [`stream_link`]
/// reader thread, which is an ordinary OS thread — never a runtime worker — so
/// `block_on` here is legal.
struct IrohRead {
    handle: Handle,
    recv: RecvStream,
}

impl Read for IrohRead {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self.handle.block_on(self.recv.read(buf)) {
            // `None` is a clean end-of-stream; the pump reads it as EOF (link gone).
            Ok(None) => Ok(0),
            Ok(Some(n)) => Ok(n),
            Err(e) => Err(ioerr(e)),
        }
    }
}

/// The send half, presented as a blocking [`Write`]. `flush` is a no-op: QUIC has
/// no user-visible buffer to flush and `write_all` has already handed the bytes to
/// the stack by the time it returns.
struct IrohWrite {
    handle: Handle,
    send: SendStream,
}

impl Write for IrohWrite {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.handle.block_on(self.send.write_all(buf)).map_err(ioerr)?;
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Parse a daemon config value into a [`Config`].
///
/// - empty → **listen** for a peer, with the n0 relay + discovery on so a NATed
///   peer can still reach us (an ephemeral identity; its id is logged on start).
/// - `<endpoint-id>` → **dial** that peer, letting relay + discovery locate it.
/// - `<endpoint-id>@<addr>[,<addr>…]` → **dial** with explicit direct UDP
///   addresses and relay/discovery **off** — LAN / known-address use.
///
/// The endpoint id is iroh's hex-encoded public key, as printed on start.
pub fn parse_config(v: &str) -> Result<Config, String> {
    let v = v.trim();
    if v.is_empty() {
        return Ok(Config { secret: None, direct_only: false, bind: None, role: Role::Listen });
    }
    let (id_str, addrs) = match v.split_once('@') {
        Some((id, rest)) => {
            let addrs = rest
                .split(',')
                .map(|a| a.trim().parse::<SocketAddr>().map_err(|_| format!("bad iroh addr `{a}`")))
                .collect::<Result<Vec<_>, _>>()?;
            (id.trim(), addrs)
        }
        None => (v, Vec::new()),
    };
    let peer = id_str.parse::<EndpointId>().map_err(|_| format!("bad iroh endpoint id `{id_str}`"))?;
    let direct_only = !addrs.is_empty();
    Ok(Config { secret: None, direct_only, bind: None, role: Role::Dial { peer, addrs } })
}

/// Build the endpoint once. Its identity and bound port persist across reconnects;
/// only the connection is re-established.
async fn build_endpoint(cfg: &Config) -> io::Result<Endpoint> {
    let sk = match cfg.secret {
        Some(b) => SecretKey::from_bytes(&b),
        None => SecretKey::generate(),
    };
    // `Minimal` installs the ring crypto provider on the builder and adds no
    // discovery and no relay; pair it with `RelayMode::Disabled` for a genuinely
    // offline, direct-only endpoint. `N0` opts into the n0 relay + discovery (and the
    // same crypto provider) so a NATed peer stays reachable.
    let mut b = if cfg.direct_only {
        Endpoint::builder(presets::Minimal).relay_mode(RelayMode::Disabled)
    } else {
        Endpoint::builder(presets::N0)
    };
    b = b.secret_key(sk).alpns(vec![ALPN.to_vec()]);
    if let Some(addr) = cfg.bind {
        b = b.bind_addr(addr).map_err(ioerr)?;
    }
    b.bind().await.map_err(ioerr)
}

/// Establish one connection for the current role and open the single bi stream the
/// bridge multiplexes envelopes over.
async fn connect_streams(ep: &Endpoint, cfg: &Config) -> io::Result<(SendStream, RecvStream)> {
    let conn: Connection = match &cfg.role {
        Role::Dial { peer, addrs } => {
            let addr = EndpointAddr::from_parts(*peer, addrs.iter().copied().map(TransportAddr::Ip));
            let conn = ep.connect(addr, ALPN).await.map_err(ioerr)?;
            // The dialer opens; the listener's `accept_bi` completes once we write.
            let (send, recv) = conn.open_bi().await.map_err(ioerr)?;
            return Ok((send, recv));
        }
        Role::Listen => {
            let incoming = ep.accept().await.ok_or_else(|| io::Error::other("endpoint closed"))?;
            incoming.accept().map_err(ioerr)?.await.map_err(ioerr)?
        }
    };
    let (send, recv) = conn.accept_bi().await.map_err(ioerr)?;
    Ok((send, recv))
}

/// Outcome of one connection's lifetime, so the reconnect loop knows whether to
/// come back.
enum Ended {
    /// The hub is gone (process shutting down): stop for good.
    HubGone,
    /// The link dropped or errored: reconnect.
    LinkLost,
}

/// Pump one live connection until it ends. Mirrors [`stream_link::run_split`]: a
/// reader thread turns received frames into `on_rx`, and this thread frames each
/// outbound `Forward` onto the stream. Takes `rx` by reference so the reconnect
/// loop can call it again with the same queue.
fn pump(
    hub: &Shared,
    iface: Iface,
    rx: &Receiver<Forward>,
    handle: &Handle,
    send: SendStream,
    recv: RecvStream,
) -> Ended {
    let rhub = hub.clone();
    let rhandle = handle.clone();
    let reader = std::thread::spawn(move || {
        let mut ir = IrohRead { handle: rhandle, recv };
        let mut ks = KissStream::new();
        let mut buf = [0u8; 4096];
        loop {
            match ir.read(&mut buf) {
                Ok(0) | Err(_) => break, // EOF or link error
                Ok(n) => {
                    for frame in ks.push(&buf[..n]) {
                        rhub.on_rx(iface, &frame, None);
                    }
                }
            }
        }
    });

    let mut w = IrohWrite { handle: handle.clone(), send };
    let ended = loop {
        let Ok(f) = rx.recv() else { break Ended::HubGone }; // hub dropped its sender
        let (Forward::Flood { bytes, .. } | Forward::Directed { bytes, .. }) = f;
        if w.write_all(&KissStream::frame(&bytes)).is_err() {
            break Ended::LinkLost;
        }
    };
    // Close our send side and let the reader wind down. On a lost link the reader
    // has already errored; on hub-shutdown it may be blocked, but the process is
    // exiting so a detached read is harmless.
    let _ = w.send.finish();
    let _ = reader.join();
    ended
}

/// Run the iroh bridge: build the endpoint, then connect-and-pump with exponential
/// backoff, exactly like the TCP/Tor stream bridges. Returns only when the hub is
/// gone.
pub fn run(hub: Shared, iface: Iface, rx: Receiver<Forward>, cfg: Config) -> io::Result<()> {
    let rt: Arc<Runtime> = Arc::new(tokio::runtime::Builder::new_multi_thread().enable_all().build()?);
    let handle = rt.handle().clone();

    let endpoint = rt.block_on(build_endpoint(&cfg))?;
    println!("  [iroh] iface {iface} endpoint id {}", endpoint.id());
    if matches!(cfg.role, Role::Listen) {
        for a in endpoint.bound_sockets() {
            println!("  [iroh] iface {iface} listening on {a}");
        }
    }

    const FIRST: Duration = Duration::from_secs(2);
    const CAP: Duration = Duration::from_secs(60);
    let mut wait = FIRST;
    loop {
        match rt.block_on(connect_streams(&endpoint, &cfg)) {
            Ok((send, recv)) => {
                wait = FIRST; // a good connect resets the backoff
                match pump(&hub, iface, &rx, &handle, send, recv) {
                    Ended::HubGone => return Ok(()),
                    Ended::LinkLost => eprintln!("  [iroh] iface {iface} link lost, reconnecting"),
                }
            }
            Err(e) => eprintln!("  [iroh] iface {iface} connect failed: {e}"),
        }
        eprintln!("  [iroh] iface {iface} retrying in {}s", wait.as_secs());
        std::thread::sleep(wait);
        wait = (wait * 2).min(CAP);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge::hub::Hub;
    use crate::Node;
    use std::sync::mpsc::channel;
    use std::time::Instant;

    /// Two endpoints on localhost, relay and discovery disabled, exchange a sealed
    /// SPORE envelope over real QUIC. The listener binds first so the dialer has a
    /// concrete `id + addr` to reach; a public message sent on the dialer's node is
    /// carried across the pipe and delivered on the listener's node.
    #[test]
    fn config_strings_parse_to_the_right_role() {
        // Empty = listen, relay on.
        let c = parse_config("").unwrap();
        assert!(matches!(c.role, Role::Listen) && !c.direct_only);

        // Bare endpoint id = dial via relay/discovery (no direct addrs).
        let id = "ea4a6c63e29c520abef5507b132ec5f9954776aebebe7b92421eea691446d22c";
        let c = parse_config(id).unwrap();
        match c.role {
            Role::Dial { addrs, .. } => assert!(addrs.is_empty() && !c.direct_only),
            _ => panic!("expected dial"),
        }

        // id@addr,addr = dial direct-only with those addresses.
        let c = parse_config(&format!("{id}@127.0.0.1:5000,127.0.0.1:5001")).unwrap();
        match c.role {
            Role::Dial { addrs, .. } => assert_eq!(addrs.len(), 2),
            _ => panic!("expected dial"),
        }
        assert!(c.direct_only, "explicit addresses imply direct-only");

        assert!(parse_config("not-a-key").is_err());
        assert!(parse_config(&format!("{id}@nonsense")).is_err());
    }

    #[test]
    fn an_envelope_crosses_two_iroh_endpoints_on_localhost() {
        // --- Listener endpoint, so we can learn its id + bound address. ---
        let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap();
        let listen_secret = [7u8; 32];
        let listen_ep = rt
            .block_on(build_endpoint(&Config {
                secret: Some(listen_secret),
                direct_only: true,
                bind: Some("127.0.0.1:0".parse().unwrap()),
                role: Role::Listen,
            }))
            .unwrap();
        let listen_id = listen_ep.id();
        let listen_addr = listen_ep.bound_sockets()[0];
        drop(listen_ep); // the listener bridge re-binds this identity+port below
        rt.shutdown_background();

        // --- Listener node + hub + iroh bridge, with a delivery sink to observe. ---
        let l_hub = Hub::new(Node::new("listener", &[]));
        let (l_iface, l_rx) = l_hub.register();
        let (deliver_tx, deliver_rx) = channel();
        l_hub.set_delivery_sink(deliver_tx);
        {
            let hub = l_hub.clone();
            std::thread::spawn(move || {
                let _ = run(
                    hub,
                    l_iface,
                    l_rx,
                    Config {
                        secret: Some(listen_secret),
                        direct_only: true,
                        bind: Some(listen_addr),
                        role: Role::Listen,
                    },
                );
            });
        }

        // --- Dialer node + hub + iroh bridge aimed at the listener. ---
        let d_hub = Hub::new(Node::new("dialer", &[]));
        let (d_iface, d_rx) = d_hub.register();
        {
            let hub = d_hub.clone();
            std::thread::spawn(move || {
                let _ = run(
                    hub,
                    d_iface,
                    d_rx,
                    Config {
                        secret: None,
                        direct_only: true,
                        bind: Some("127.0.0.1:0".parse().unwrap()),
                        role: Role::Dial { peer: listen_id, addrs: vec![listen_addr] },
                    },
                );
            });
        }

        // Give both bridges a moment to bind and connect, then originate a public
        // message on the dialer. It floods to the dialer's iroh iface, crosses the
        // QUIC stream, and is delivered on the listener's node.
        let deadline = Instant::now() + Duration::from_secs(20);
        let mut delivered = None;
        while Instant::now() < deadline {
            l_hub.send(crate::ZERO_DEST, b"hello over iroh quic".to_vec()).unwrap_or(());
            d_hub.send(crate::ZERO_DEST, b"hello over iroh quic".to_vec()).unwrap_or(());
            if let Ok(wire) = deliver_rx.recv_timeout(Duration::from_millis(500)) {
                delivered = Some(wire);
                break;
            }
        }
        let wire = delivered.expect("listener delivered a message carried over iroh");
        let (env, _) = crate::Envelope::decode(&wire).expect("delivered bytes decode as an envelope");
        assert!(
            env.payload.windows(b"hello over iroh quic".len()).any(|w| w == b"hello over iroh quic"),
            "the delivered envelope carries the message that crossed the pipe"
        );
    }
}
