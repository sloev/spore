//! SPORE reference node + self-contained demo.
//!
//!   cargo run                 # in-memory mesh simulation
//!   cargo run -- udp          # a real node on UDP :7373 with LAN broadcast
//!   cargo run -- http         # an HTTP "bag" bridge on :7373 (push/inv/want)
//!   cargo run -- folder DIR   # a shared-store bridge over a folder of *.spore
//!   cargo run -- tcp [HOST]   # a KISS-over-TCP stream bridge (listen, or connect)
//!   cargo run -- meshtastic   # bridge to a Meshtastic WiFi-UDP broadcast node
//!
//! The simulation drives the exact `Node::on_rx` router used in production; each
//! `-- <mode>` swaps in a different bridge. The router never changes — a bridge
//! only moves envelope bytes in and out of the node.

use spore::*;
use std::collections::VecDeque;

const NOW: u32 = 1_700_000_000;

// ---------------------------------------------------------------------------
// In-memory ether: nodes wired by an adjacency list, one shared iface (0).
// ---------------------------------------------------------------------------

struct World {
    nodes: Vec<Node>,
    names: Vec<String>,
    adj: Vec<Vec<usize>>,
    floods: usize,
    directed: usize,
}

impl World {
    fn line(spec: &[(&str, &[&str])]) -> World {
        let nodes: Vec<Node> = spec.iter().map(|(n, t)| Node::new(n, t)).collect();
        let names: Vec<String> = spec.iter().map(|(n, _)| n.to_string()).collect();
        let mut adj = vec![Vec::new(); nodes.len()];
        for i in 0..nodes.len().saturating_sub(1) {
            adj[i].push(i + 1);
            adj[i + 1].push(i);
        }
        World { nodes, names, adj, floods: 0, directed: 0 }
    }

    fn idx(&self, name: &str) -> usize {
        self.names.iter().position(|n| n == name).unwrap()
    }

    fn enqueue(&mut self, q: &mut VecDeque<(usize, Vec<u8>, Option<Addr>)>, from: usize, f: Forward) {
        let from_addr = self.nodes[from].addr;
        match f {
            Forward::Flood { bytes, .. } => {
                self.floods += 1;
                for &n in &self.adj[from] {
                    q.push_back((n, bytes.clone(), Some(from_addr)));
                }
            }
            Forward::Directed { nbr, bytes, .. } => {
                self.directed += 1;
                for &n in &self.adj[from] {
                    if Some(self.nodes[n].addr) == nbr {
                        q.push_back((n, bytes.clone(), Some(from_addr)));
                    }
                }
            }
        }
    }

    /// Run to quiescence. Returns (node index, delivered envelope).
    fn run(&mut self, seeds: Vec<(usize, Forward)>) -> Vec<(usize, Envelope)> {
        let mut q = VecDeque::new();
        for (from, f) in seeds {
            self.enqueue(&mut q, from, f);
        }
        let mut delivered = Vec::new();
        while let Some((dst, bytes, from)) = q.pop_front() {
            let rx = self.nodes[dst].on_rx(&bytes, 0, from, NOW);
            for e in rx.delivered {
                // Mix behaviour (§9): if this is an onion layer for us, peel it
                // and re-inject the inner envelope as our own traffic.
                if let Some(inner) = self.nodes[dst].onion_peel(&e) {
                    let r2 = self.nodes[dst].on_rx(&inner, 0, None, NOW);
                    for e2 in r2.delivered {
                        delivered.push((dst, e2));
                    }
                    for f in r2.forwards {
                        self.enqueue(&mut q, dst, f);
                    }
                } else {
                    delivered.push((dst, e));
                }
            }
            for f in rx.forwards {
                self.enqueue(&mut q, dst, f);
            }
        }
        delivered
    }
}

fn first_bytes(forwards: &[Forward]) -> Vec<u8> {
    match &forwards[0] {
        Forward::Flood { bytes, .. } => bytes.clone(),
        Forward::Directed { bytes, .. } => bytes.clone(),
    }
}

fn sim() {
    println!("SPORE demo — line topology  A — B — C — D\n");

    let mut w = World::line(&[("A", &["news"]), ("B", &["news"]), ("C", &["news"]), ("D", &["news"])]);
    let (a, d) = (w.idx("A"), w.idx("D"));

    // 1) Every node floods a signed ANNOUNCE -> seeds prekeys + paths.
    let mut seeds = Vec::new();
    for i in 0..w.nodes.len() {
        for f in w.nodes[i].build_announce(NOW) {
            seeds.push((i, f));
        }
    }
    w.run(seeds);
    let d_addr = w.nodes[d].addr;
    let known =
        (0..w.nodes.len()).filter(|&i| i != a && w.nodes[a].peer_prekey(&w.nodes[i].addr).is_some()).count();
    println!(
        "[1] ANNOUNCE round done. A learned {} peer prekeys; has D's prekey: {}",
        known,
        w.nodes[a].peer_prekey(&d_addr).is_some()
    );

    // 2) A originates a signed PUBLIC message -> floods to everyone.
    w.floods = 0;
    let pf = w.nodes[a].originate(ZERO_DEST, b"the dam holds".to_vec(), NOW);
    let pub_id = Envelope::decode(&first_bytes(&pf)).unwrap().0.id();
    let del = w.run(pf.into_iter().map(|f| (a, f)).collect());
    let who: Vec<&str> =
        del.iter().filter(|(_, e)| e.id() == pub_id).map(|(n, _)| w.names[*n].as_str()).collect();
    println!("[2] PUBLIC flood from A delivered to: {:?}  ({} flood sends)", who, w.floods);

    // 3) A seals to D's prekey and unicasts to D's address. Every node learned
    //    a path toward D in step 1, so this is routed hop-by-hop (Directed).
    w.floods = 0;
    w.directed = 0;
    let d_prekey = w.nodes[a].peer_prekey(&d_addr).expect("A learned D's prekey");
    let sealed = seal(b"meet at the north pier, midnight", &d_prekey);
    let uf = w.nodes[a].originate(d_addr, sealed, NOW);
    let uni_id = Envelope::decode(&first_bytes(&uf)).unwrap().0.id();
    let del = w.run(uf.into_iter().map(|f| (a, f)).collect());
    let recipients: Vec<(&str, Vec<u8>)> = del
        .iter()
        .filter(|(_, e)| e.id() == uni_id)
        .map(|(n, e)| (w.names[*n].as_str(), e.payload.clone()))
        .collect();
    let names_only: Vec<&str> = recipients.iter().map(|(n, _)| *n).collect();
    println!(
        "[3] SEALED unicast A->D delivered to: {:?}  ({} directed hops, {} floods)",
        names_only, w.directed, w.floods
    );

    // D opens the sealed payload it received; nobody else can decrypt it.
    if let Some((_, sealed_payload)) = recipients.first() {
        let opened = w.nodes[d].open(sealed_payload);
        println!(
            "    D decrypts payload: {:?}",
            opened.as_deref().map(|b| String::from_utf8_lossy(b).into_owned())
        );
    }

    // 4) High-level send(): one call moves a 6 KB object the caller never had
    //    to fragment. It auto-splits into fountain chunks; every node on the
    //    line reassembles and signature-verifies the original.
    w.floods = 0;
    let big = vec![0xABu8; 6000];
    let sf = w.nodes[a].send(topic_of("news"), big.clone(), NOW).expect("demo payload fits one fountain set");
    let chunks = sf.len();
    let del = w.run(sf.into_iter().map(|f| (a, f)).collect());
    let got: Vec<&str> =
        del.iter().filter(|(_, e)| e.payload == big).map(|(n, _)| w.names[*n].as_str()).collect();
    println!("[4] SEND {} B object -> {} fragments; reassembled + verified by: {:?}", big.len(), chunks, got);

    // 5) Fountain over a 40%-loss ONE-WAY link (no feedback): S -> R.
    fountain_demo();

    // 6) A UDP-like datagram session, routed multi-hop A -> D and decrypted
    //    only by D. This is the primitive SSH-over-SPORE (Mosh-style) builds on.
    w.floods = 0;
    w.directed = 0;
    let a_addr = w.nodes[a].addr;
    let mut sad = w.nodes[a].dial(d_addr, 22).expect("A knows D's prekey");
    let dgf = w.nodes[a].dg_send(&mut sad, b"interactive hello over a udp-like link", NOW);
    let del = w.run(dgf.into_iter().map(|f| (a, f)).collect());
    let opened = del
        .iter()
        .find(|(n, e)| *n == d && e.payload.first() == Some(&session::TAG_DGRAM))
        .and_then(|(_, e)| {
            let mut sda = w.nodes[d].dial(a_addr, 22)?;
            w.nodes[d].dg_recv(&mut sda, e)
        });
    println!(
        "[6] DATAGRAM session A->D over {} directed hops: D decrypts {:?}",
        w.directed,
        opened.as_deref().map(|b| String::from_utf8_lossy(b).into_owned())
    );

    // 7) FILE: A publishes a content-addressed file; its magnet is the manifest
    //    ID. The small manifest floods to everyone; B then pulls the data chunks
    //    from a neighbour and verifies each one against the signed manifest.
    let file: Vec<u8> = (0..9000u32).map(|i| (i.wrapping_mul(31)) as u8).collect();
    let (magnet, mf) = w.nodes[a].publish_file("field-notes.txt", &file, ZERO_DEST, NOW);
    w.run(mf.into_iter().map(|f| (a, f)).collect()); // all nodes absorb the manifest
    let bx = w.idx("B");
    let want = w.nodes[bx].fetch(&magnet);
    w.run(want.into_iter().map(|f| (bx, f)).collect()); // chunks pulled to B
    let ok = w.nodes[bx].file_bytes(&magnet).as_deref() == Some(&file[..]);
    let mp: String = magnet[..6].iter().map(|b| format!("{b:02x}")).collect();
    println!(
        "[7] FILE 'field-notes.txt' ({} B) published as magnet {}…; B pulled + verified: {}",
        file.len(),
        mp,
        ok
    );

    // 8) MIX onion (§9): A sends an anonymous message to D wrapped through mixes
    //    B and C. Each mix peels exactly one layer; the innermost is public and
    //    sealed to D, so every node carries it but only D can open it.
    let (bx, cx) = (w.idx("B"), w.idx("C"));
    let b_pk = w.nodes[a].peer_prekey(&w.nodes[bx].addr).unwrap();
    let c_pk = w.nodes[a].peer_prekey(&w.nodes[cx].addr).unwrap();
    let d_pk = w.nodes[a].peer_prekey(&d_addr).unwrap();
    let mut inner = Envelope::new(ty::DATA, ZERO_DEST, NOW + 3600, seal(b"burn the ledgers", &d_pk));
    inner.flags |= fl::ENCRYPTED;
    let hops = [(w.nodes[bx].addr, b_pk), (w.nodes[cx].addr, c_pk)];
    let onion = mix::onion_wrap(&inner, &hops, NOW + 3600).unwrap();
    let del = w.run(vec![(a, Forward::Flood { except: NO_IFACE, bytes: onion.wire() })]);
    let opened = del.iter().filter(|(n, _)| *n == d).find_map(|(_, e)| w.nodes[d].open(&e.payload));
    println!(
        "[8] MIX onion A~>D via {} mixes (B,C): D opens {:?}",
        hops.len(),
        opened.as_deref().map(|b| String::from_utf8_lossy(b).into_owned())
    );

    // 9) RECEIPT (§8): A sends an ACKREQ unicast to D; D auto-replies a signed
    //    receipt that floods back, so A learns the message was delivered.
    let rf = w.nodes[a].originate_ackreq(d_addr, b"confirm you got this".to_vec(), NOW);
    let rid = Envelope::decode(&first_bytes(&rf)).unwrap().0.id();
    w.run(rf.into_iter().map(|f| (a, f)).collect());
    println!("[9] RECEIPT: A's ACKREQ message to D acknowledged: {}", w.nodes[a].acked(&rid));

    // 10) ENCRYPTED TOPIC (§7): A floods a message on topic "news" sealed under a
    //     pre-shared key. Every node carries it; only key-holders can read it.
    let psk = [0x42u8; 32];
    let tf = w.nodes[a].originate(topic_of("news"), topic_seal(b"safehouse moved", &psk), NOW);
    let tid = Envelope::decode(&first_bytes(&tf)).unwrap().0.id();
    let del = w.run(tf.into_iter().map(|f| (a, f)).collect());
    let payload = del.iter().find(|(_, e)| e.id() == tid).map(|(_, e)| e.payload.clone()).unwrap();
    let holder = topic_open(&payload, &psk).map(|b| String::from_utf8_lossy(&b).into_owned());
    let outsider = topic_open(&payload, &[0u8; 32]).is_some();
    println!("[10] ENCRYPTED TOPIC: key-holder reads {:?}; outsider can read: {}", holder, outsider);

    // 11) RPC (L4): A calls a service at D and gets a reply routed back.
    let (rid2, qf) = w.nodes[a].request(
        d_addr,
        rpc::Request { method: "GET".into(), path: "/status".into(), body: vec![] },
        NOW,
    );
    w.run(qf.into_iter().map(|f| (a, f)).collect()); // request reaches D, auto-queued
    let dd = w.idx("D");
    let mut resp_forwards = Vec::new();
    for (from, id, req) in w.nodes[dd].poll_requests() {
        let body = format!("serving {} {}", req.method, req.path).into_bytes();
        let rr = w.nodes[dd].respond(from, id, rpc::Response { status: 200, body }, NOW);
        resp_forwards.extend(rr.into_iter().map(|f| (dd, f)));
    }
    w.run(resp_forwards); // reply routes back to A
    let got = w.nodes[a].take_response(rid2);
    println!(
        "[11] RPC A->D GET /status -> {:?}",
        got.map(|r| (r.status, String::from_utf8_lossy(&r.body).into_owned()))
    );

    // 12) FEED (L5): everyone subscribes to "alerts"; A publishes one event.
    for i in 0..w.nodes.len() {
        w.nodes[i].subscribe("alerts");
    }
    let pf = w.nodes[a].publish("alerts", b"the tide is turning".to_vec(), NOW);
    w.run(pf.into_iter().map(|f| (a, f)).collect());
    let readers: Vec<&str> = (0..w.nodes.len())
        .filter(|&i| !w.nodes[i].poll_feed().is_empty())
        .map(|i| w.names[i].as_str())
        .collect();
    println!("[12] FEED 'alerts' published by A; received by: {:?}", readers);
}

fn fountain_demo() {
    let mut src = Node::new("S", &[]);
    let sf = src.originate(ZERO_DEST, vec![0x5Au8; 4000], NOW);
    let wire = first_bytes(&sf);
    let id = Envelope::decode(&wire).unwrap().0.id();

    let cs = 200usize;
    let count = wire.len().div_ceil(cs);
    let indices: Vec<u8> = (0..(count as u8 + 60)).collect(); // data + repair chunks
    let frags = fragment(&wire, cs, 16, NOW + 7 * 86400, ZERO_DEST, id, &indices);

    let mut fo = Fountain::new();
    let mut rng: u64 = 0xC0FFEE;
    let (mut sent, mut fed) = (0usize, 0usize);
    let mut recovered = None;
    for fr in &frags {
        sent += 1;
        rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1);
        if (rng >> 33) % 100 < 40 {
            continue; // 40% packet loss, no retransmit possible
        }
        fed += 1;
        let (idx, cnt) = (fr.payload[16], fr.payload[17]);
        let chunk = fr.payload[18..].to_vec();
        if let Some(w) = fo.add(&id, idx, cnt, chunk) {
            recovered = Some(w);
            break;
        }
    }
    let w = recovered.expect("reassembled");
    let ok = w == wire && Envelope::decode(&w).unwrap().0.verify();
    println!(
        "[5] FOUNTAIN over 40% loss, one-way: {} data chunks needed; {} survived of {} sent; \
         reassembled + signature-verified: {}",
        count, fed, sent, ok
    );
}

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
enum Spec {
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
}

#[cfg(not(target_arch = "wasm32"))]
struct Config {
    petname: String,
    topics: Vec<String>,
    bridges: Vec<Spec>,
}

// A deliberately tiny YAML subset: `key: value` lines, `- item` list entries,
// `#` comments, and inline `[a, b]` lists. No external dependency.
#[cfg(not(target_arch = "wasm32"))]
fn parse_config(text: &str) -> Result<Config, String> {
    #[derive(PartialEq)]
    enum Sec {
        None,
        Topics,
        Bridges,
    }
    let mut petname = "spore".to_string();
    let mut topics: Vec<String> = Vec::new();
    let mut bridges: Vec<Spec> = Vec::new();
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
            other => return Err(format!("line {ln}: unknown key `{other}`")),
        }
    }
    if bridges.is_empty() {
        return Err("no bridges configured".to_string());
    }
    Ok(Config { petname, topics, bridges })
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

#[cfg(not(target_arch = "wasm32"))]
fn hex8(a: &Addr) -> String {
    a.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(not(target_arch = "wasm32"))]
fn run_config(cfg: Config) {
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
            Spec::Udp(port) => {
                let (iface, rx) = hub.register();
                thread::spawn(move || {
                    if let Err(e) = spore::bridge::udp::run(h, iface, rx, port) {
                        eprintln!("  [udp] {e}");
                    }
                })
            }
            Spec::Broadcast(port) => {
                let (iface, rx) = hub.register();
                thread::spawn(move || {
                    if let Err(e) = spore::bridge::udp::run_primary(h, iface, rx, port) {
                        eprintln!("  [udp] {e}");
                    }
                })
            }
            Spec::Tcp(target) => {
                let (iface, rx) = hub.register();
                thread::spawn(move || {
                    if let Err(e) = spore::bridge::tcp::run(h, iface, rx, target) {
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
                    if let Err(e) = spore::bridge::udp::run_group(h, iface, rx, &bind, &group) {
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
            let mut trickle = Trickle::new(now(), 5, 80);
            loop {
                let t = now();
                if trickle.due(t) {
                    trickle.fired(t);
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

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        sim(); // no config -> the in-memory demo
        return;
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let path = &args[0];
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("cannot read config `{path}`: {e}");
                eprintln!("usage: spore <config.yaml>   (or `spore` for the in-memory demo)");
                return;
            }
        };
        match parse_config(&text) {
            Ok(cfg) => run_config(cfg),
            Err(e) => eprintln!("config error: {e}"),
        }
    }
}
