//! SPORE reference node + self-contained demo.
//!
//!   cargo run            # in-memory mesh simulation (deterministic)
//!   cargo run -- udp     # a real node on UDP :7373 with LAN broadcast
//!
//! The simulation drives the exact `Node::on_rx` router used in production; the
//! only thing swapped is the transport (an in-memory "ether" instead of UDP).

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
                delivered.push((dst, e));
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
// A real transport: UDP :7373 with LAN broadcast. This is ONE interface; add
// BLE/LoRa/serial the same way and the router is unchanged. Congestion control
// (10% airtime token bucket + CSMA jitter) belongs here, not in the router.
// ---------------------------------------------------------------------------

#[cfg(not(target_arch = "wasm32"))]
fn run_udp() -> std::io::Result<()> {
    use std::net::{Ipv4Addr, SocketAddrV4, UdpSocket};
    use std::time::{SystemTime, UNIX_EPOCH};

    let sock = UdpSocket::bind(("0.0.0.0", 7373))?;
    sock.set_broadcast(true)?;
    let bcast = SocketAddrV4::new(Ipv4Addr::BROADCAST, 7373);
    let mut node = Node::new("udp-node", &["news"]);
    let now = || SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as u32;

    for f in node.build_announce(now()) {
        if let Forward::Flood { bytes, .. } = f {
            sock.send_to(&bytes, bcast)?;
        }
    }
    println!("SPORE node on udp/7373 (addr {}). Ctrl-C to stop.", hexad(&node.addr));

    let mut buf = [0u8; 2048];
    loop {
        let (n, _peer) = sock.recv_from(&mut buf)?;
        let rx = node.on_rx(&buf[..n], 0, None, now());
        for e in &rx.delivered {
            println!("  delivered {} bytes (dest {})", e.payload.len(), hexad(&e.dest));
        }
        for f in rx.forwards {
            let bytes = match f {
                Forward::Flood { bytes, .. } => bytes,
                Forward::Directed { bytes, .. } => bytes,
            };
            sock.send_to(&bytes, bcast)?;
        }
    }
}

fn hexad(a: &Addr) -> String {
    a.iter().map(|b| format!("{b:02x}")).collect()
}

fn main() {
    if std::env::args().any(|a| a == "udp") {
        #[cfg(not(target_arch = "wasm32"))]
        if let Err(e) = run_udp() {
            eprintln!("udp error: {e}");
        }
    } else {
        sim();
    }
}
