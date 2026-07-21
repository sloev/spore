//! SPORE reference node + self-contained demo.
//!
//!   cargo run                 # in-memory mesh simulation
//!   cargo run -- udp          # a real node on UDP :7373 with LAN broadcast
//!   cargo run -- http         # an HTTP "bag" bridge on :7373 (push/inv/want)
//!   cargo run -- folder DIR   # a shared-store bridge over a folder of *.spore
//!   cargo run -- tcp [HOST]   # a KISS-over-TCP stream bridge (listen, or connect)
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
    use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};
    use std::time::{SystemTime, UNIX_EPOCH};

    let sock = UdpSocket::bind(("0.0.0.0", 7373))?;
    sock.set_broadcast(true)?;
    let bcast: SocketAddr = SocketAddrV4::new(Ipv4Addr::BROADCAST, 7373).into();
    let mut node = Node::new("udp-node", &["news"]);
    // The one generic resolver, instantiated for this medium: U = SocketAddr.
    // Every other bridge uses the same table with its own U (MAC, node id, …).
    let mut neighbors: bridge::Neighbors<SocketAddr> = bridge::Neighbors::new(2 * 3600);
    let now = || SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as u32;

    for f in node.build_announce(now()) {
        if let Forward::Flood { bytes, .. } = f {
            sock.send_to(&bytes, bcast)?;
        }
    }
    println!("SPORE node on udp/7373 (addr {}). Ctrl-C to stop.", hexad(&node.addr));

    let mut buf = [0u8; 2048];
    loop {
        let (n, peer) = sock.recv_from(&mut buf)?;
        let t = now();
        // ARP-style snoop: bind the signed sender's SPORE address to its socket,
        // and reuse it as the router's neighbour hint.
        let nbr = neighbors.snoop(&buf[..n], peer, t);
        let rx = node.on_rx(&buf[..n], 0, nbr, t);
        for e in &rx.delivered {
            println!("  delivered {} bytes (dest {})", e.payload.len(), hexad(&e.dest));
        }
        for f in rx.forwards {
            match f {
                Forward::Flood { bytes, .. } => {
                    sock.send_to(&bytes, bcast)?;
                }
                Forward::Directed { nbr, bytes, .. } => {
                    // Resolve the SPORE next-hop to a socket: unicast if we've
                    // learned one, else broadcast (which still reaches it).
                    match nbr.and_then(|a| neighbors.resolve(&a, t)) {
                        Some(dst) => sock.send_to(&bytes, dst)?,
                        None => sock.send_to(&bytes, bcast)?,
                    };
                }
            }
        }
    }
}

fn hexad(a: &Addr) -> String {
    a.iter().map(|b| format!("{b:02x}")).collect()
}

// ---------------------------------------------------------------------------
// Bridge: HTTP "bag" API (spec Page 2). HTTP is just a transport for envelopes,
// no more special than UDP or a folder. Three endpoints move a bag of mail:
//   POST /spore/push   body = envelope wire(s)     -> stores + relays them
//   GET  /spore/inv    -> our stored IDs (16 B ea)
//   POST /spore/want   body = IDs (16 B ea)         -> their envelopes
// ---------------------------------------------------------------------------

#[cfg(not(target_arch = "wasm32"))]
fn run_http() -> std::io::Result<()> {
    use std::net::TcpListener;
    use std::time::{SystemTime, UNIX_EPOCH};

    let mut node = Node::new("http-node", &["news"]);
    let listener = TcpListener::bind(("0.0.0.0", 7373))?;
    println!(
        "SPORE HTTP bag bridge on http://0.0.0.0:7373  (addr {})\n  POST /spore/push · GET /spore/inv · POST /spore/want",
        hexad(&node.addr)
    );
    for stream in listener.incoming() {
        let mut s = match stream {
            Ok(s) => s,
            Err(_) => continue,
        };
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as u32;
        if let Err(e) = serve_http(&mut node, &mut s, now) {
            eprintln!("http: {e}");
        }
    }
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn serve_http(node: &mut Node, s: &mut std::net::TcpStream, now: u32) -> std::io::Result<()> {
    use std::io::{Read, Write};

    // Read the request head, then the Content-Length body.
    let mut buf = Vec::new();
    let mut tmp = [0u8; 4096];
    let head_end = loop {
        if let Some(p) = find_sub(&buf, b"\r\n\r\n") {
            break p;
        }
        let n = s.read(&mut tmp)?;
        if n == 0 {
            return Ok(());
        }
        buf.extend_from_slice(&tmp[..n]);
    };
    let head = String::from_utf8_lossy(&buf[..head_end]).to_string();
    let mut lines = head.lines();
    let reqline = lines.next().unwrap_or("");
    let mut parts = reqline.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let path = parts.next().unwrap_or("").to_string();
    let mut content_len = 0usize;
    for l in lines {
        if let Some(v) = l.to_ascii_lowercase().strip_prefix("content-length:") {
            content_len = v.trim().parse().unwrap_or(0);
        }
    }
    let mut body = buf[head_end + 4..].to_vec();
    while body.len() < content_len {
        let n = s.read(&mut tmp)?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&tmp[..n]);
    }
    body.truncate(content_len);

    let route = path.split('?').next().unwrap_or(&path);
    let op = match (method.as_str(), route) {
        ("POST", "/spore/push") => Some(bridge::Bag::Push(body)),
        ("GET", "/spore/inv") => Some(bridge::Bag::Inv),
        ("POST", "/spore/want") => Some(bridge::Bag::Want(body)),
        _ => None,
    };
    let (status, resp) = match op {
        Some(op) => {
            let (_forwards, resp) = bridge::bag(node, op, 0, now);
            ("200 OK", resp)
        }
        None => ("404 Not Found", Vec::new()),
    };
    let header = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/x-spore\r\nContent-Length: {}\r\n\
         Access-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n",
        resp.len()
    );
    s.write_all(header.as_bytes())?;
    s.write_all(&resp)?;
    Ok(())
}

fn find_sub(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

// ---------------------------------------------------------------------------
// Bridge: shared-store folder. The folder of `<hexid>.spore` files *is* the
// network — read it to receive, write to it to send. A USB stick, Syncthing,
// or Dropbox turns two folders on two machines into one SPORE link.
// ---------------------------------------------------------------------------

#[cfg(not(target_arch = "wasm32"))]
fn run_folder(dir: &str) -> std::io::Result<()> {
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as u32;
    let mut node = Node::new("folder-node", &["news"]);
    let p = Path::new(dir);

    // Read the folder (receive), seed one message, then write our store back.
    let rx = bridge::store::import(p, &mut node, 0, now)?;
    for e in &rx.delivered {
        println!("  imported {} bytes (dest {})", e.payload.len(), hexad(&e.dest));
    }
    node.originate(topic_of("news"), b"hello from a folder bridge".to_vec(), now);
    let wrote = bridge::store::export_all(p, &node)?;
    println!(
        "folder bridge '{dir}': imported {} envelope(s), exported {} new file(s)",
        rx.delivered.len(),
        wrote
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Bridge: KISS over a TCP byte stream (shape 2). Point-to-point: `tcp` listens
// for one peer, `tcp HOST:PORT` connects. Shows congestion control live —
// Trickle paces the HELLO beacon, a token bucket caps relayed bytes (§5.4a/b).
// ---------------------------------------------------------------------------

#[cfg(not(target_arch = "wasm32"))]
fn run_tcp(target: Option<&str>) -> std::io::Result<()> {
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    let now = || SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as u32;
    let mut stream = match target {
        Some(addr) => {
            println!("connecting to {addr} …");
            TcpStream::connect(addr)?
        }
        None => {
            let l = TcpListener::bind(("0.0.0.0", 7373))?;
            println!("listening on tcp/7373 for one peer …");
            l.accept()?.0
        }
    };
    stream.set_read_timeout(Some(Duration::from_millis(500)))?;

    let mut node = Node::new("tcp-node", &["news"]);
    let mut ks = bridge::KissStream::new();
    // Congestion control, made visible: pace beacons, cap relays.
    let mut trickle = congestion::Trickle::new(now(), 5, 80); // seconds here (spec: minutes)
    let mut relay = congestion::TokenBucket::ten_percent(1200); // ~1.2 kB/s link
    println!("SPORE tcp bridge up (addr {}). Ctrl-C to stop.", hexad(&node.addr));

    let beacon = |node: &mut Node, s: &mut TcpStream, t: u32| -> std::io::Result<()> {
        for f in node.build_announce(t) {
            if let Forward::Flood { bytes, .. } = f {
                s.write_all(&bridge::KissStream::frame(&bytes))?;
            }
        }
        Ok(())
    };
    beacon(&mut node, &mut stream, now())?;

    let mut buf = [0u8; 2048];
    loop {
        match stream.read(&mut buf) {
            Ok(0) => {
                println!("peer closed");
                break;
            }
            Ok(n) => {
                for frame in ks.push(&buf[..n]) {
                    let t = now();
                    let rx = node.on_rx(&frame, 0, None, t);
                    for e in &rx.delivered {
                        println!("  delivered {} bytes (dest {})", e.payload.len(), hexad(&e.dest));
                    }
                    for f in rx.forwards {
                        let bytes = match f {
                            Forward::Flood { bytes, .. } => bytes,
                            Forward::Directed { bytes, .. } => bytes,
                        };
                        // §5.4a token bucket: skip the relay if over budget.
                        if relay.allow(bytes.len() as u32, t) {
                            stream.write_all(&bridge::KissStream::frame(&bytes))?;
                        }
                    }
                }
            }
            Err(ref e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut => {}
            Err(e) => return Err(e),
        }
        // §5.4b Trickle: re-beacon on the doubling interval.
        let t = now();
        if trickle.due(t) {
            trickle.fired(t);
            beacon(&mut node, &mut stream, t)?;
        }
    }
    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(|s| s.as_str()) {
        #[cfg(not(target_arch = "wasm32"))]
        Some("udp") => {
            if let Err(e) = run_udp() {
                eprintln!("udp error: {e}");
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        Some("tcp") => {
            if let Err(e) = run_tcp(args.get(2).map(|s| s.as_str())) {
                eprintln!("tcp error: {e}");
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        Some("http") => {
            if let Err(e) = run_http() {
                eprintln!("http error: {e}");
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        Some("folder") => {
            let dir = args.get(2).map(|s| s.as_str()).unwrap_or("spore");
            if let Err(e) = run_folder(dir) {
                eprintln!("folder error: {e}");
            }
        }
        _ => sim(),
    }
}
