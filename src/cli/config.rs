//! Parsing a node's YAML-ish config: identity, topics, and bridge specs.
//!
//! Split out of the 799-line `src/main.rs` (task #23). Binary-crate module;
//! no wire contract here, and `main.rs` is in no frozen file.

// ---------------------------------------------------------------------------
// Config-driven multi-bridge runner. Instead of CLI flags, a node is described
// by a small YAML file listing the bridges to run; every bridge shares one Node
// (via the Hub) and relays to the others. You can list several of the same kind
// — two folders, two TCP links, UDP on different ports:
//
//   petname: riverside
//   topics: [news, weather]
//   bridges:
//     - broadcast          # zero-config: the primary subnet's own broadcast
//     - udp                # or the plain 255.255.255.255 limited broadcast
//     - folder: ./bag
//     - folder: /mnt/usb/spore
//     - tcp: 10.0.0.5:7373
//     - meshtastic
//     - http: 8088
//     - audio              # data-over-sound (f32 PCM on stdin/stdout)
//     - reticulum          # RNS payload via tools/reticulum_companion.py (KISS on stdin/stdout)
//     - ssb: ./ssb-log     # Secure Scuttlebutt append-only log folder
//
// The runners live in `spore::bridge::{udp,tcp,store,meshtastic,audio,reticulum,bag}`;
// this file only parses the config and wires them onto one node.
// ---------------------------------------------------------------------------

#[cfg(not(target_arch = "wasm32"))]
pub(crate) enum Spec {
    Udp(u16),
    Broadcast(Option<u16>),
    Tcp(Option<String>),
    Folder(std::path::PathBuf),
    Meshtastic,
    MeshtasticSerial(Option<String>),
    Ax25Tcp(String),
    Ax25Serial(String),
    Tor(String),
    I2p(String),
    I2pAccept(String),
    Copyparty(String),
    UdpGroup(String, String),
    ReticulumTcp(String),
    ReticulumUdp(String, String),
    Icmp(String),
    Spool(String, String),
    Http(u16),
    Audio,
    Reticulum,
    Ssb(std::path::PathBuf),
    /// The optional iroh QUIC bridge. Held as the raw config string and parsed in
    /// the runner under `#[cfg(feature = "bridge-iroh")]`, so this file needs no
    /// iroh types and the variant compiles whether or not the feature is on. Empty
    /// = listen; otherwise `<endpoint-id>[@addr[,addr]]` = dial.
    Iroh(String),
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) struct Config {
    pub(crate) petname: String,
    pub(crate) topics: Vec<String>,
    pub(crate) bridges: Vec<Spec>,
    /// Where this node is reachable for Direct media, as `ip:port` or a bare
    /// port to pair with the primary IPv4. `None` leaves Direct off entirely —
    /// a node that cannot say where it is has no candidate to offer.
    pub(crate) direct: Option<String>,
    /// A peer to keep a Direct pipe open to, as a 16-hex-digit address. The
    /// daemon has no control surface to start one from, so without this the
    /// initiator half could never run and two daemons would both sit waiting to
    /// be offered a pipe.
    pub(crate) direct_to: Option<String>,
    /// A reflexive echo to ask where we appear to be, as `host:port`. Any SPORE
    /// daemon running `stun:` is one; so is any public STUN server.
    pub(crate) direct_stun: Option<String>,
    /// Run a reflexive echo on this port for other nodes to ask. Default off —
    /// it is a service you choose to offer, though it costs one packet.
    pub(crate) stun: Option<u16>,
}

// A deliberately tiny YAML subset: `key: value` lines, `- item` list entries,
// `#` comments, and inline `[a, b]` lists. No external dependency.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn parse_config(text: &str) -> Result<Config, String> {
    #[derive(PartialEq)]
    enum Sec {
        None,
        Topics,
        Bridges,
    }
    let mut petname = "spore".to_string();
    let mut topics: Vec<String> = Vec::new();
    let mut bridges: Vec<Spec> = Vec::new();
    let mut direct: Option<String> = None;
    let mut direct_to: Option<String> = None;
    let mut direct_stun: Option<String> = None;
    let mut stun: Option<u16> = None;
    let mut sec = Sec::None;

    for (i, raw) in text.lines().enumerate() {
        let line = strip_comment(raw);
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let ln = i + 1;
        if let Some(item) = trimmed.strip_prefix('-') {
            let item = item.trim();
            match sec {
                Sec::Bridges => bridges.push(parse_bridge(item).map_err(|e| format!("line {ln}: {e}"))?),
                Sec::Topics => topics.push(item.to_string()),
                Sec::None => return Err(format!("line {ln}: list item `{item}` outside a section")),
            }
            continue;
        }
        let (k, v) = trimmed.split_once(':').ok_or(format!("line {ln}: cannot parse `{trimmed}`"))?;
        let (k, v) = (k.trim(), v.trim());
        match k {
            "petname" => {
                petname = v.to_string();
                sec = Sec::None;
            }
            "topics" => {
                if v.is_empty() {
                    sec = Sec::Topics;
                } else {
                    for t in v.trim_start_matches('[').trim_end_matches(']').split(',') {
                        let t = t.trim();
                        if !t.is_empty() {
                            topics.push(t.to_string());
                        }
                    }
                    sec = Sec::None;
                }
            }
            "bridges" => {
                if !v.is_empty() {
                    return Err(format!("line {ln}: `bridges:` should be a list"));
                }
                sec = Sec::Bridges;
            }
            "direct-stun" => {
                if v.is_empty() {
                    return Err(format!("line {ln}: `direct-stun:` needs a host:port"));
                }
                direct_stun = Some(v.to_string());
                sec = Sec::None;
            }
            "stun" => {
                stun = Some(v.parse().map_err(|_| format!("line {ln}: `stun:` needs a port"))?);
                sec = Sec::None;
            }
            "direct-to" => {
                if v.is_empty() {
                    return Err(format!("line {ln}: `direct-to:` needs a 16-hex-digit peer address"));
                }
                direct_to = Some(v.to_string());
                sec = Sec::None;
            }
            "direct" => {
                if v.is_empty() {
                    return Err(format!("line {ln}: `direct:` needs a port or an ip:port"));
                }
                direct = Some(v.to_string());
                sec = Sec::None;
            }
            other => return Err(format!("line {ln}: unknown key `{other}`")),
        }
    }
    if bridges.is_empty() {
        return Err("no bridges configured".to_string());
    }
    Ok(Config { petname, topics, bridges, direct, direct_to, direct_stun, stun })
}

#[cfg(not(target_arch = "wasm32"))]
fn parse_bridge(s: &str) -> Result<Spec, String> {
    if let Some((k, v)) = s.split_once(':') {
        let (k, v) = (k.trim(), v.trim());
        match k {
            "udp" => Ok(Spec::Udp(v.parse().map_err(|_| format!("bad udp port `{v}`"))?)),
            "broadcast" | "lan" => {
                Ok(Spec::Broadcast(Some(v.parse().map_err(|_| format!("bad broadcast port `{v}`"))?)))
            }
            "tcp" => Ok(Spec::Tcp(if v.is_empty() { None } else { Some(v.to_string()) })),
            "folder" if !v.is_empty() => Ok(Spec::Folder(v.into())),
            "folder" => Err("`folder:` needs a path".into()),
            "http" => Ok(Spec::Http(v.parse().map_err(|_| format!("bad http port `{v}`"))?)),
            "ssb" if !v.is_empty() => Ok(Spec::Ssb(v.into())),
            "ssb" => Err("`ssb:` needs a log directory".into()),
            "ax25" | "kiss" if !v.is_empty() => Ok(if v.starts_with('/') {
                Spec::Ax25Serial(v.to_string())
            } else {
                Spec::Ax25Tcp(v.to_string())
            }),
            "iroh" if !v.is_empty() => Ok(Spec::Iroh(v.to_string())),
            "tor" | "onion" if !v.is_empty() => Ok(Spec::Tor(v.to_string())),
            "reticulum-tcp" | "rns-tcp" if !v.is_empty() => Ok(Spec::ReticulumTcp(v.to_string())),
            "reticulum-udp" | "rns-udp" if v.contains("->") => {
                let (b, p) = v.split_once("->").unwrap();
                Ok(Spec::ReticulumUdp(b.trim().to_string(), p.trim().to_string()))
            }
            "icmp" | "ping" if !v.is_empty() => Ok(Spec::Icmp(v.to_string())),
            // `spool: TX -> RX` — outbound and inbound directories, moved by
            // NNCP, UUCP, rsync or a USB stick.
            "spool" | "nncp" | "uucp" if v.contains("->") => {
                let (t, r) = v.split_once("->").unwrap();
                Ok(Spec::Spool(t.trim().to_string(), r.trim().to_string()))
            }
            "spool" | "nncp" | "uucp" => Err("needs TX -> RX (spool: ./out -> ./in)".into()),
            "i2p" if !v.is_empty() => Ok(Spec::I2p(v.to_string())),
            "i2p-accept" => Ok(Spec::I2pAccept(v.to_string())),
            "copyparty" | "webdav" if !v.is_empty() => Ok(Spec::Copyparty(v.to_string())),
            // `group: BIND -> GROUP` — an explicit pair, IPv4 or IPv6. What an
            // overlay (Yggdrasil, cjdns) or a multi-homed mesh node needs.
            "group" if v.contains("->") => {
                let (b, g) = v.split_once("->").unwrap();
                Ok(Spec::UdpGroup(b.trim().to_string(), g.trim().to_string()))
            }
            "group" => Err("`group` needs BIND -> GROUP (group: [::]:7373 -> [ff02::7373]:7373)".into()),
            "meshtastic-serial" | "mesh-serial" if !v.is_empty() => {
                Ok(Spec::MeshtasticSerial(Some(v.to_string())))
            }
            "meshtastic-serial" | "mesh-serial" => Ok(Spec::MeshtasticSerial(None)),
            other => Err(format!("unknown bridge `{other}`")),
        }
    } else {
        match s {
            "udp" => Ok(Spec::Udp(7373)),
            "broadcast" | "lan" => Ok(Spec::Broadcast(None)),
            "tcp" => Ok(Spec::Tcp(None)),
            "meshtastic" | "mesh" => Ok(Spec::Meshtastic),
            "meshtastic-serial" | "mesh-serial" => Ok(Spec::MeshtasticSerial(None)),
            "http" => Ok(Spec::Http(7373)),
            "audio" | "sound" => Ok(Spec::Audio),
            "reticulum" | "rns" => Ok(Spec::Reticulum),
            "iroh" => Ok(Spec::Iroh(String::new())), // bare `iroh` = listen for a peer
            "iroh-listen" => Ok(Spec::Iroh(String::new())),
            "ax25" | "kiss" => Err("`ax25` needs a TNC (ax25: HOST:PORT, or a /dev path)".into()),
            "tor" | "onion" => Err("`tor` needs an onion (tor: abc…xyz.onion[:port])".into()),
            "i2p" => Err("`i2p` needs a destination (i2p: <b32>.b32.i2p)".into()),
            "reticulum-tcp" | "rns-tcp" => Err("`reticulum-tcp` needs the companion's HOST:PORT".into()),
            "reticulum-udp" | "rns-udp" => Err("`reticulum-udp` needs BIND -> PEER".into()),
            "icmp" | "ping" => Err("`icmp` needs a peer IPv4 (icmp: 192.168.1.42)".into()),
            "i2p-accept" => Ok(Spec::I2pAccept(String::new())),
            "copyparty" | "webdav" => Err("`copyparty` needs a URL (copyparty: http://host/bag/)".into()),
            "folder" => Err("`folder` needs a path (folder: DIR)".into()),
            "ssb" => Err("`ssb` needs a log directory (ssb: DIR)".into()),
            other => Err(format!("unknown bridge `{other}`")),
        }
    }
}

// Strip a trailing `# comment` (only when the `#` starts the line or follows
// whitespace, so a `#` inside a value survives).
#[cfg(not(target_arch = "wasm32"))]
fn strip_comment(line: &str) -> &str {
    match line.find('#') {
        Some(p) if p == 0 || line[..p].ends_with(char::is_whitespace) => &line[..p],
        _ => line,
    }
}
