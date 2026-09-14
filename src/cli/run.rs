//! The config-driven daemon: stand up every bridge and pump the node.
//!
//! Split out of the 799-line `src/main.rs` (task #23). Binary-crate module;
//! no wire contract here, and `main.rs` is in no frozen file.

use super::config::{Config, Spec};
use spore::*;

#[cfg(not(target_arch = "wasm32"))]
fn hex8(a: &Addr) -> String {
    a.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn run_config(cfg: Config) {
    use spore::bridge::hub::{now, Hub};
    use spore::congestion::Trickle;
    use std::thread;

    let topic_refs: Vec<&str> = cfg.topics.iter().map(|s| s.as_str()).collect();

    // Identity, prekey ring and store, in that order — the same three nutrients
    // Android and ESP32 supply, and the same order they depend on each other in.
    // Until this existed the daemon called `Node::new` and nothing else, so a
    // restart produced a new address and an empty store: the reference node was
    // the one runtime that could not be left running.
    let home = super::home::dir();
    let (seed, fresh) = match super::home::load_or_create_seed(&home) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("cannot open {}: {e}", home.display());
            return;
        }
    };
    let mut node = Node::from_seed(&cfg.petname, &topic_refs, &seed);

    // The seed restores the address; the ring restores what can still be opened.
    // A node that keeps one and loses the other keeps its name and silently
    // drops inbound mail sealed to any prekey it had rotated to — which, with
    // daily rotation, is most of it.
    let ring_path = super::home::prekey_path(&home);
    let restored_ring =
        std::fs::read(&ring_path).ok().map(|blob| node.restore_prekey_ring(&blob)).unwrap_or(false);
    if let Err(e) = std::fs::write(&ring_path, node.prekey_ring()) {
        eprintln!("  [warn] cannot write {}: {e}", ring_path.display());
    }

    let store = super::home::store_dir(&home);
    let adopted = match node.set_spill_dir(&store, now()) {
        Ok(n) => n,
        Err(e) => {
            eprintln!("  [warn] no spill directory at {}: {e}", store.display());
            0
        }
    };

    let hub = Hub::new(node);

    // **One delivery sink, installed before any bridge**, so nothing delivered
    // is missed and so the console sees mail whether or not Direct is on. It
    // used to be installed only when Direct was configured, which meant a node
    // running plain UDP received messages and showed nobody — the reason #303's
    // Demo A could not be performed.
    let (deliver_tx, deliver_rx) = std::sync::mpsc::channel::<Vec<u8>>();
    hub.set_delivery_sink(deliver_tx);

    // The shared application layer, so `/history` is the same store the browser
    // and the phone use rather than a fourth copy of a conversation list.
    #[cfg(feature = "communicator")]
    let comm = std::sync::Arc::new(std::sync::Mutex::new(spore::communicator::Communicator::new()));

    println!(
        "SPORE node {} ({}) — {} bridge(s). Ctrl-C to stop.",
        hex8(&hub.addr()),
        cfg.petname,
        cfg.bridges.len()
    );
    println!(
        "  [home] {}  identity: {}{}  store: {} adopted",
        home.display(),
        if fresh { "new" } else { "restored" },
        if restored_ring { ", prekey ring restored" } else { "" },
        adopted
    );

    let mut handles = Vec::new();

    // The reflexive echo (P-Direct-NAT step 2). Stateless: one packet in, one
    // out, nothing retained — which is why it is cheap enough to just run, and
    // why every SPORE daemon offering it keeps the network from depending on a
    // third party's STUN server for something the protocol does for itself.
    if let Some(port) = cfg.stun {
        println!("  [stun] reflexive echo on :{port}");
        handles.push(thread::spawn(move || match std::net::UdpSocket::bind(("0.0.0.0", port)) {
            Ok(sock) => {
                let mut buf = [0u8; 1024];
                loop {
                    match sock.recv_from(&mut buf) {
                        Ok((n, from)) => {
                            spore::direct::stun::serve_one(&sock, &buf[..n], from);
                        }
                        Err(e) => {
                            eprintln!("  [stun] {e}");
                            return;
                        }
                    }
                }
            }
            Err(e) => eprintln!("  [stun] cannot bind :{port}: {e}"),
        }));
    }

    // Direct: a second plane, off unless the config says where this node can be
    // reached.
    let mut direct_tx: Option<std::sync::mpsc::Sender<Vec<u8>>> = None;
    match cfg.direct.as_deref().map(super::direct::locator) {
        None => {}
        Some(None) => eprintln!(
            "  [direct] off: `direct:` needs an explicit ip:port, or a bare port \
             with a discoverable primary IPv4"
        ),
        Some(Some((advertise, bind_port))) => {
            // Direct gets a *forwarded* copy. It used to own the sink outright,
            // which is why installing a console meant choosing between the two.
            let (tx, rx) = std::sync::mpsc::channel();
            direct_tx = Some(tx);
            println!("  [direct] reachable at {advertise} (UDP)");
            let mut d = super::direct::Direct::new(hub.addr(), advertise, bind_port, cfg.direct_also.clone());
            // Ask where we appear from outside, if an echo was named. Best
            // effort and non-fatal: a node that cannot reach one simply offers
            // its LAN locator, which is exactly what it did before.
            if let Some(server) = cfg.direct_stun.as_deref() {
                match d.learn_reflexive(server) {
                    Some(seen) => println!("  [direct] reflexive locator {seen} (via {server})"),
                    None => eprintln!("  [direct] {server} did not answer — offering the LAN locator only"),
                }
            }
            // iroh: the last rung, and the only one that can involve a third
            // party. The posture is printed at the moment it takes effect, not
            // buried in a config comment.
            #[cfg(feature = "bridge-iroh")]
            if let Some(relay) = cfg.direct_iroh.as_deref() {
                match d.enable_iroh(relay) {
                    Some(id) => {
                        println!("  [direct] iroh endpoint {id}");
                        if relay == "n0" {
                            println!(
                                "  [direct] iroh relay: n0's public relay — a third party that \
                                 sees ciphertext, volume and timing when a path is relayed"
                            );
                        } else {
                            println!("  [direct] iroh relay: none (direct-only)");
                        }
                    }
                    None => eprintln!("  [direct] iroh endpoint failed to bind — medium not offered"),
                }
            }
            #[cfg(not(feature = "bridge-iroh"))]
            if cfg.direct_iroh.is_some() {
                eprintln!("  [direct] `direct-iroh:` ignored — this build has no `bridge-iroh` feature");
            }
            println!("  [direct] offering {}", d.offering());
            // Said once, plainly: knowing the outside address is not the same as
            // being reachable at it. The punch now runs on both ends at once
            // (#114) and each pipe reports which it got — `punched` or `no punch,
            // plain connect` — so this is a statement of what to watch for rather
            // than a standing disclaimer. A global IPv6, where one exists, still
            // needs neither.
            eprintln!(
                "  [direct] note: each pipe prints how it was established; \
                 `no punch, plain connect` on a reflexive candidate means no path"
            );
            let dial = match cfg.direct_to.as_deref() {
                None => None,
                Some(s) => match super::direct::peer_addr(s) {
                    Some(a) => {
                        println!("  [direct] will keep a pipe to {s}");
                        Some(a)
                    }
                    None => {
                        eprintln!("  [direct] ignoring `direct-to: {s}` — not a 16-hex-digit address");
                        None
                    }
                },
            };
            let h = hub.clone();
            handles.push(thread::spawn(move || {
                let mut last_expire = now();
                let mut last_offer = 0u32;
                loop {
                    // A short timeout rather than a blocking recv: the open pipes
                    // are owned by this thread and have to be drained here too.
                    match rx.recv_timeout(std::time::Duration::from_millis(50)) {
                        Ok(wire) => {
                            d.on_delivered(&h, &wire);
                        }
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                    }
                    d.poll();
                    let t = now();
                    if t.saturating_sub(last_expire) >= 30 {
                        last_expire = t;
                        d.expire();
                    }
                    // Offering is paced, not continuous: an unanswered offer has
                    // to be given time to expire before another is worth making.
                    if let Some(peer) = dial {
                        if t.saturating_sub(last_offer) >= 30 {
                            last_offer = t;
                            d.maintain(&h, peer);
                        }
                    }
                }
            }));
        }
    }

    for spec in cfg.bridges {
        let h = hub.clone();
        let handle = match spec {
            // No per-bridge stop control from the CLI yet (Ctrl-C ends the whole
            // process); each gets its own flag that is simply never set.
            Spec::Udp(port) => {
                let (iface, rx) = hub.register();
                thread::spawn(move || {
                    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
                    if let Err(e) = spore::bridge::udp::run(h, iface, rx, port, stop) {
                        eprintln!("  [udp] {e}");
                    }
                })
            }
            Spec::Broadcast(port) => {
                let (iface, rx) = hub.register();
                thread::spawn(move || {
                    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
                    if let Err(e) = spore::bridge::udp::run_primary(h, iface, rx, port, stop) {
                        eprintln!("  [udp] {e}");
                    }
                })
            }
            Spec::Tcp(target) => {
                let (iface, rx) = hub.register();
                thread::spawn(move || {
                    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
                    if let Err(e) = spore::bridge::tcp::run(h, iface, rx, target, stop) {
                        eprintln!("  [tcp] {e}");
                    }
                })
            }
            Spec::Folder(dir) => {
                let (iface, rx) = hub.register();
                thread::spawn(move || {
                    if let Err(e) = spore::bridge::store::run(h, iface, rx, dir) {
                        eprintln!("  [folder] {e}");
                    }
                })
            }
            Spec::Meshtastic => {
                let (iface, rx) = hub.register_limited(spore::bridge::meshtastic::BULK_BYTES_PER_SEC);
                thread::spawn(move || {
                    if let Err(e) = spore::bridge::meshtastic::run(h, iface, rx) {
                        eprintln!("  [meshtastic] {e}");
                    }
                })
            }
            Spec::MeshtasticSerial(path) => {
                let (iface, rx) = hub.register_limited(spore::bridge::meshtastic::BULK_BYTES_PER_SEC);
                thread::spawn(move || {
                    let r = match &path {
                        Some(p) => spore::bridge::meshtastic::run_serial(h, iface, rx, p),
                        None => spore::bridge::meshtastic::run_pipe(h, iface, rx),
                    };
                    if let Err(e) = r {
                        eprintln!("  [meshtastic] {e}");
                    }
                })
            }
            Spec::Ax25Tcp(target) => {
                let (iface, rx) = hub.register_limited(spore::bridge::ax25::BULK_BYTES_PER_SEC);
                thread::spawn(move || {
                    if let Err(e) = spore::bridge::ax25::run_tcp(h, iface, rx, &target) {
                        eprintln!("  [ax25] {e}");
                    }
                })
            }
            Spec::Ax25Serial(path) => {
                let (iface, rx) = hub.register_limited(spore::bridge::ax25::BULK_BYTES_PER_SEC);
                thread::spawn(move || {
                    if let Err(e) = spore::bridge::ax25::run_serial(h, iface, rx, &path) {
                        eprintln!("  [ax25] {e}");
                    }
                })
            }
            Spec::Tor(target) => {
                let (iface, rx) = hub.register();
                thread::spawn(move || {
                    if let Err(e) = spore::bridge::tor::run(h, iface, rx, &target) {
                        eprintln!("  [tor] {e}");
                    }
                })
            }
            Spec::I2p(target) => {
                let (iface, rx) = hub.register();
                thread::spawn(move || {
                    if let Err(e) = spore::bridge::i2p::run(h, iface, rx, &target) {
                        eprintln!("  [i2p] {e}");
                    }
                })
            }
            Spec::Iroh(v) => {
                // The iroh bridge is optional and async; only spawn it when the
                // `bridge-iroh` feature is compiled in. Without it, a config that
                // names `iroh` gets a clear message rather than a silent no-op.
                #[cfg(feature = "bridge-iroh")]
                {
                    let (iface, rx) = hub.register();
                    thread::spawn(move || match spore::bridge::iroh::parse_config(&v) {
                        Ok(cfg) => {
                            if let Err(e) = spore::bridge::iroh::run(h, iface, rx, cfg) {
                                eprintln!("  [iroh] {e}");
                            }
                        }
                        Err(e) => eprintln!("  [iroh] bad config `{v}`: {e}"),
                    })
                }
                #[cfg(not(feature = "bridge-iroh"))]
                {
                    let _ = &h;
                    thread::spawn(move || {
                        eprintln!("  [iroh] built without the `bridge-iroh` feature; ignoring `{v}`");
                    })
                }
            }
            Spec::ReticulumTcp(target) => {
                let (iface, rx) = hub.register_limited(spore::bridge::reticulum::BULK_BYTES_PER_SEC);
                thread::spawn(move || {
                    if let Err(e) = spore::bridge::reticulum::run_tcp(h, iface, rx, &target) {
                        eprintln!("  [reticulum] {e}");
                    }
                })
            }
            Spec::ReticulumUdp(bind, peer) => {
                let (iface, rx) = hub.register_limited(spore::bridge::reticulum::BULK_BYTES_PER_SEC);
                thread::spawn(move || {
                    if let Err(e) = spore::bridge::reticulum::run_udp(h, iface, rx, &bind, &peer) {
                        eprintln!("  [reticulum] {e}");
                    }
                })
            }
            Spec::Icmp(peer) => {
                let (iface, rx) = hub.register();
                thread::spawn(move || {
                    #[cfg(target_os = "linux")]
                    if let Err(e) = spore::bridge::icmp::run(h, iface, rx, &peer) {
                        eprintln!("  [icmp] {e}");
                    }
                    #[cfg(not(target_os = "linux"))]
                    {
                        let _ = (h, iface, rx, peer);
                        eprintln!("  [icmp] raw ICMP is Linux-only");
                    }
                })
            }
            Spec::Spool(tx, rx) => {
                let (iface, rxc) = hub.register();
                thread::spawn(move || {
                    if let Err(e) = spore::bridge::spool::run(h, iface, rxc, tx.into(), rx.into()) {
                        eprintln!("  [spool] {e}");
                    }
                })
            }
            Spec::I2pAccept(sam) => {
                let (iface, rx) = hub.register();
                thread::spawn(move || {
                    if let Err(e) = spore::bridge::i2p::run_accept(h, iface, rx, &sam) {
                        eprintln!("  [i2p] {e}");
                    }
                })
            }
            Spec::UdpGroup(bind, group) => {
                let (iface, rx) = hub.register();
                thread::spawn(move || {
                    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
                    if let Err(e) = spore::bridge::udp::run_group(h, iface, rx, &bind, &group, stop) {
                        eprintln!("  [udp] {e}");
                    }
                })
            }
            Spec::Copyparty(url) => {
                let (iface, rx) = hub.register();
                thread::spawn(move || {
                    let every = std::time::Duration::from_secs(5);
                    if let Err(e) = spore::bridge::copyparty::run(h, iface, rx, &url, every) {
                        eprintln!("  [copyparty] {e}");
                    }
                })
            }
            Spec::Http(port) => {
                let iface = hub.register_pull();
                thread::spawn(move || {
                    if let Err(e) = spore::bridge::bag::run_http(h, iface, port) {
                        eprintln!("  [http] {e}");
                    }
                })
            }
            Spec::Audio => {
                let (iface, rx) = hub.register_limited(spore::bridge::audio::BULK_BYTES_PER_SEC);
                thread::spawn(move || {
                    if let Err(e) = spore::bridge::audio::run_pipe(h, iface, rx) {
                        eprintln!("  [audio] {e}");
                    }
                })
            }
            Spec::Reticulum => {
                let (iface, rx) = hub.register_limited(spore::bridge::reticulum::BULK_BYTES_PER_SEC);
                thread::spawn(move || {
                    if let Err(e) = spore::bridge::reticulum::run_pipe(h, iface, rx) {
                        eprintln!("  [reticulum] {e}");
                    }
                })
            }
            Spec::Ssb(dir) => {
                let (iface, rx) = hub.register();
                thread::spawn(move || {
                    if let Err(e) = spore::bridge::ssb::run(h, iface, rx, dir) {
                        eprintln!("  [ssb] {e}");
                    }
                })
            }
        };
        handles.push(handle);
    }

    // One beacon loop floods this node's ANNOUNCE on every interface, Trickle-paced.
    {
        let h = hub.clone();
        thread::spawn(move || {
            // Two cadences, because §4 defines two frames and they cost very
            // different amounts. The HELLO is link-local (hops 0) and rides the
            // Trickle schedule in the timer's own unit — seconds; passing the
            // spec's "5 → 80 min" as the bare numbers 5 and 80 is what made this
            // beacon 60× too chatty (S-023). The flooded ANNOUNCE is mesh-wide and
            // is held to the documented ceiling of one an hour.
            let mut trickle = Trickle::new(now(), HELLO_MIN_SECS, HELLO_MAX_SECS);
            let mut last_flood = 0u32; // 0 => flood once at startup
            loop {
                let t = now();
                if trickle.due(t) {
                    trickle.fired(t);
                    h.hello();
                }
                if t.saturating_sub(last_flood) >= ANNOUNCE_FLOOD_MIN_SECS {
                    last_flood = t;
                    h.beacon();
                }
                // Expiry sweep, prekey rotation, and ACKREQ resend. Until this
                // was here the daemon did none of them unless traffic arrived,
                // so a quiet node never pruned, never advanced its forward
                // secrecy, and never retried an unacked send.
                h.tick();
                thread::sleep(std::time::Duration::from_millis(500));
            }
        });
    }

    // The inbox. Prints what arrived, files it in the shared thread store, and
    // hands Direct its copy if Direct is running.
    //
    // The sink itself is installed unconditionally, because Direct needs it even
    // in a build with no application layer; what is gated is the half that has
    // somewhere to put a conversation.
    {
        #[cfg(feature = "communicator")]
        let c = comm.clone();
        handles.push(thread::spawn(move || {
            while let Ok(wire) = deliver_rx.recv() {
                #[cfg(feature = "communicator")]
                if let Ok(mut c) = c.lock() {
                    super::console::on_delivered(&mut c, &wire);
                }
                if let Some(tx) = &direct_tx {
                    // A dead Direct thread must not stop the console.
                    let _ = tx.send(wire);
                }
            }
        }));
    }

    // The console owns this thread from here. It returns if stdin closes — a
    // service, a pipe, `< /dev/null` — and the node keeps relaying either way,
    // which is why the bridge threads are joined after it rather than before.
    #[cfg(feature = "communicator")]
    super::console::run(hub.clone(), &comm, &home);

    for handle in handles {
        let _ = handle.join();
    }
}
