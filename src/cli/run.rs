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
    let node = Node::new(&cfg.petname, &topic_refs);
    let hub = Hub::new(node);
    println!(
        "SPORE node {} ({}) — {} bridge(s). Ctrl-C to stop.",
        hex8(&hub.addr()),
        cfg.petname,
        cfg.bridges.len()
    );

    let mut handles = Vec::new();
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
                thread::sleep(std::time::Duration::from_millis(500));
            }
        });
    }

    for handle in handles {
        let _ = handle.join();
    }
}
