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

    let mut w = World::line(&[
        ("A", &["news"]),
        ("B", &["news"]),
        ("C", &["news"]),
        ("D", &["news"]),
    ]);
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
    let known = (0..w.nodes.len())
        .filter(|&i| i != a && w.nodes[a].peer_prekey(&w.nodes[i].addr).is_some())
        .count();
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
    let who: Vec<&str> = del.iter().filter(|(_, e)| e.id() == pub_id).map(|(n, _)| w.names[*n].as_str()).collect();
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
    println!("[3] SEALED unicast A->D delivered to: {:?}  ({} directed hops, {} floods)", names_only, w.directed, w.floods);

    // D opens the sealed payload it received; nobody else can decrypt it.
    if let Some((_, sealed_payload)) = recipients.first() {
        let opened = w.nodes[d].open(sealed_payload);
        println!("    D decrypts payload: {:?}", opened.as_deref().map(|b| String::from_utf8_lossy(b).into_owned()));
    }

    // 4) High-level send(): one call moves a 6 KB object the caller never had
    //    to fragment. It auto-splits into fountain chunks; every node on the
    //    line reassembles and signature-verifies the original.
    w.floods = 0;
    let big = vec![0xABu8; 6000];
    let sf = w.nodes[a].send(topic_of("news"), big.clone(), NOW);
    let chunks = sf.len();
    let del = w.run(sf.into_iter().map(|f| (a, f)).collect());
    let got: Vec<&str> =
        del.iter().filter(|(_, e)| e.payload == big).map(|(n, _)| w.names[*n].as_str()).collect();
    println!(
        "[4] SEND {} B object -> {} fragments; reassembled + verified by: {:?}",
        big.len(),
        chunks,
        got
    );

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
    let opened = del
        .iter()
        .filter(|(n, _)| *n == d)
        .find_map(|(_, e)| w.nodes[d].open(&e.payload));
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
    let count = (wire.len() + cs - 1) / cs;
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
// Multi-bridge runner. One shared Node behind a Hub; every bridge named on the
// command line runs in its own thread and relays to the others — so one process
// can bridge a LAN, a folder on a USB stick, a TCP link, and a Meshtastic mesh
// at once:
//
//   cargo run -- udp folder ./bag tcp 10.0.0.5:7373 meshtastic
//
// The runners live in `spore::bridge::{udp,tcp,store,meshtastic,bag}`; this is
// only the CLI that wires them onto one node.
// ---------------------------------------------------------------------------

#[cfg(not(target_arch = "wasm32"))]
enum Spec {
    Udp,
    Tcp(Option<String>),
    Folder(std::path::PathBuf),
    Meshtastic,
    Http(u16),
}

#[cfg(not(target_arch = "wasm32"))]
fn parse_specs(args: &[String]) -> Result<Vec<Spec>, String> {
    let mut specs = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let tok = args[i].as_str();
        i += 1;
        let spec = match tok {
            "udp" => Spec::Udp,
            "meshtastic" | "mesh" => Spec::Meshtastic,
            "folder" => {
                let dir = args.get(i).ok_or("`folder` needs a directory")?.clone();
                i += 1;
                Spec::Folder(dir.into())
            }
            "tcp" => match args.get(i) {
                // an optional HOST:PORT to connect to; otherwise listen
                Some(a) if a.contains(':') => {
                    i += 1;
                    Spec::Tcp(Some(a.clone()))
                }
                _ => Spec::Tcp(None),
            },
            "http" => {
                let port = match args.get(i).and_then(|p| p.parse::<u16>().ok()) {
                    Some(p) => {
                        i += 1;
                        p
                    }
                    None => 7373,
                };
                Spec::Http(port)
            }
            other => return Err(format!("unknown bridge `{other}`")),
        };
        specs.push(spec);
    }
    Ok(specs)
}

#[cfg(not(target_arch = "wasm32"))]
fn hex8(a: &Addr) -> String {
    a.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(not(target_arch = "wasm32"))]
fn run_bridges(specs: Vec<Spec>) {
    use spore::bridge::hub::{now, Hub};
    use spore::congestion::Trickle;
    use std::thread;

    let node = Node::new("spore", &["news"]);
    let hub = Hub::new(node);
    println!("SPORE node {} — {} bridge(s). Ctrl-C to stop.", hex8(&hub.addr()), specs.len());

    let mut handles = Vec::new();
    for spec in specs {
        let h = hub.clone();
        let handle = match spec {
            Spec::Udp => {
                let (iface, rx) = hub.register();
                thread::spawn(move || {
                    if let Err(e) = spore::bridge::udp::run(h, iface, rx) {
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
                let (iface, rx) = hub.register();
                thread::spawn(move || {
                    if let Err(e) = spore::bridge::meshtastic::run(h, iface, rx) {
                        eprintln!("  [meshtastic] {e}");
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
        sim();
        return;
    }
    #[cfg(not(target_arch = "wasm32"))]
    match parse_specs(&args) {
        Ok(specs) => run_bridges(specs),
        Err(e) => {
            eprintln!("error: {e}");
            eprintln!("usage: spore [udp] [tcp [HOST:PORT]] [folder DIR] [meshtastic] [http [PORT]]");
            eprintln!("       spore            # run the in-memory demo");
        }
    }
}
