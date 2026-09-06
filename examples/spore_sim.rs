//! `spore-sim` (M11-B) — a deterministic simulator over the **real** crate.
//!
//!     cargo run --example spore_sim              # smoke suite, JSON to stdout
//!     cargo run --example spore_sim -- mixed-mtu # one scenario
//!
//! **Why an example and not a module.** The engine must never reach a device.
//! An `examples/` binary is compiled for the host only, so nothing here can grow
//! the wasm blob, the ESP32 image, or the dependency graph the Xtensa target has
//! to satisfy — and it needs no feature flag to stay out of them. It uses only
//! `std` and the crate's own public API, which is also what makes it honest: it
//! drives `Node::on_rx` and `Node::send` exactly as a bridge does, so what it
//! measures is SPORE and not a model of SPORE.
//!
//! **What it is for.** M11-C and M11-D both have to pick numbers — how much
//! redundancy a hop-local fragment set carries, how many chunks to push with a
//! manifest, whether a lost fragment is better recovered by retry or by a
//! relocated fountain. Picking those by argument is how the fountain-versus-
//! torrent confusion started. This exists so they are picked by measurement.
//!
//! **The scenario that matters most is the one that fails.** `mixed-mtu` is a
//! Wi-Fi island bridged to a LoRa hop, and it is the case the current design
//! silently cannot serve: a node fragments at *its own* MTU, and a fragment
//! cannot be fragmented again, so frames die at the first narrow link they meet
//! and no node on the path can repair it. Nothing in the unit tests would have
//! caught that, because every unit test runs on one notional link. It is
//! reported rather than asserted, so that M11-D flipping it to `delivered: true`
//! is a visible result rather than a red build.
//!
//! Determinism is not decoration. Every node is built with `Node::from_seed`,
//! every coin comes from one seeded LCG, and the event queue breaks ties on a
//! monotonic sequence number — so a run is reproducible from its seed, and a
//! metric that moves means the protocol moved.

use spore::linkfrag::{self, Reassembler};
use spore::*;
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

// ---------------------------------------------------------------- randomness

/// One seeded LCG for the whole run. Not cryptographic and does not pretend to
/// be: it decides which packets drop, and a reproducible loss pattern is worth
/// more here than an unpredictable one.
struct Rng(u64);
impl Rng {
    fn next_u32(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.0 >> 33) as u32
    }
    /// True with probability `pct`/100.
    fn chance(&mut self, pct: u32) -> bool {
        pct > 0 && self.next_u32() % 100 < pct
    }
    fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            self.next_u32() as usize % n
        }
    }
}

// ------------------------------------------------------------------ topology

/// One link between two nodes. `mtu` is the whole point of this simulator: a
/// frame larger than the link's MTU is *not* silently split, it is dropped,
/// because that is exactly what a real radio does with an oversized frame.
#[derive(Clone, Copy)]
struct Link {
    a: usize,
    b: usize,
    mtu: usize,
    loss_pct: u32,
    latency_ms: u64,
}

struct World {
    nodes: Vec<Node>,
    links: Vec<Link>,
    /// node -> [(link index, iface id as that node sees it, peer index)]
    ifaces: Vec<Vec<(usize, Iface, usize)>>,
}

impl World {
    fn new(n: usize, links: Vec<Link>) -> Self {
        Self::with_topics(n, links, &[])
    }

    fn with_topics(n: usize, links: Vec<Link>, topics: &[&str]) -> Self {
        let nodes = (0..n)
            .map(|i| {
                let mut seed = [0u8; 32];
                seed[0] = (i & 0xff) as u8;
                seed[1] = ((i >> 8) & 0xff) as u8;
                seed[2] = 0xA5;
                Node::from_seed("n", topics, &seed)
            })
            .collect();
        let mut ifaces: Vec<Vec<(usize, Iface, usize)>> = vec![Vec::new(); n];
        for (li, l) in links.iter().enumerate() {
            let ia = ifaces[l.a].len() as Iface;
            ifaces[l.a].push((li, ia, l.b));
            let ib = ifaces[l.b].len() as Iface;
            ifaces[l.b].push((li, ib, l.a));
        }
        World { nodes, links, ifaces }
    }

    /// The narrowest link this node can transmit on — what a per-node MTU clamp
    /// would collapse to, and the reason the clamp only helps the node that owns
    /// the narrow link.
    fn narrowest(&self, node: usize) -> usize {
        self.ifaces[node].iter().map(|(li, _, _)| self.links[*li].mtu).min().unwrap_or(1400)
    }
}

// -------------------------------------------------------------------- events

#[derive(PartialEq, Eq)]
struct Job {
    at_ms: u64,
    seq: u64,
    node: usize,
    iface: Iface,
    wire: Vec<u8>,
}
impl Ord for Job {
    fn cmp(&self, o: &Self) -> std::cmp::Ordering {
        (self.at_ms, self.seq).cmp(&(o.at_ms, o.seq))
    }
}
impl PartialOrd for Job {
    fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(o))
    }
}

#[derive(Default)]
struct Metrics {
    frames_tx: u64,
    bytes_tx: u64,
    dropped_loss: u64,
    /// Frames a link refused because they exceeded its MTU. **The mixed-MTU
    /// signal.** Non-zero here with `delivered: false` is the M11-D bug.
    dropped_mtu: u64,
    delivered: u64,
    duplicate_delivered: u64,
    first_delivery_ms: Option<u64>,
}

struct Sim {
    world: World,
    rng: Rng,
    queue: BinaryHeap<Reverse<Job>>,
    seq: u64,
    now_ms: u64,
    epoch_secs: u32,
    m: Metrics,
    /// node -> ids already delivered, so a duplicate is counted rather than
    /// silently absorbed by the node's own dedup.
    seen_delivered: Vec<HashMap<Id, ()>>,
    /// Wall time when the measured phase began, so latency is reported relative
    /// to the send rather than to the start of the universe.
    measure_from_ms: u64,
    /// Per-node link reassembly — the receiving half of M11-D. `None` models the
    /// world before it, where an oversized frame simply does not arrive.
    link_frag: Option<Vec<Reassembler>>,
    next_set_id: u16,
}

impl Sim {
    fn new(world: World, seed: u64) -> Self {
        let n = world.nodes.len();
        Sim {
            world,
            rng: Rng(seed),
            queue: BinaryHeap::new(),
            seq: 0,
            now_ms: 0,
            epoch_secs: 1_700_000_000,
            m: Metrics::default(),
            seen_delivered: vec![HashMap::new(); n],
            measure_from_ms: 0,
            link_frag: None,
            next_set_id: 1,
        }
    }

    /// Turn on link fragmentation: bridges split what their link cannot carry
    /// and the far end reassembles before the node sees anything.
    fn with_link_fragmentation(mut self) -> Self {
        let n = self.world.nodes.len();
        self.link_frag = Some((0..n).map(|_| Reassembler::default()).collect());
        self
    }

    fn now_secs(&self) -> u32 {
        self.epoch_secs + (self.now_ms / 1000) as u32
    }

    /// Put one node's forwards on the wire, applying every link's own limits.
    fn emit(&mut self, from: usize, forwards: Vec<Forward>) {
        for f in forwards {
            let (except, only_iface, wire) = match f {
                Forward::Flood { except, bytes } => (except, None, bytes),
                Forward::Directed { iface, bytes, .. } => (NO_IFACE, Some(iface), bytes),
            };
            let peers = self.world.ifaces[from].clone();
            for (li, iface, peer) in peers {
                if iface == except {
                    continue; // never back where it came from
                }
                if let Some(want) = only_iface {
                    if iface != want {
                        continue;
                    }
                }
                let link = self.world.links[li];
                self.m.frames_tx += 1;
                self.m.bytes_tx += wire.len() as u64;

                // Without link fragmentation an oversized frame does not get
                // split by magic — it does not arrive. With it, the bridge
                // splits for this link and the far end puts it back.
                let pieces: Vec<Vec<u8>> = if self.link_frag.is_some() {
                    self.next_set_id = self.next_set_id.wrapping_add(1).max(1);
                    linkfrag::split(&wire, link.mtu, self.next_set_id)
                } else {
                    if wire.len() > link.mtu {
                        self.m.dropped_mtu += 1;
                        continue;
                    }
                    vec![wire.clone()]
                };
                if pieces.len() > 1 {
                    // Charge the extra frames: splitting is not free, and the
                    // point of measuring is to see what it costs.
                    self.m.frames_tx += pieces.len() as u64 - 1;
                    self.m.bytes_tx += (pieces.len() as u64 - 1) * linkfrag::LINK_FRAG_OVERHEAD as u64;
                }
                let mut dropped = false;
                for _ in 0..pieces.len() {
                    if self.rng.chance(link.loss_pct) {
                        dropped = true;
                    }
                }
                if dropped {
                    self.m.dropped_loss += 1;
                    continue;
                }
                // Which iface the *peer* sees this arrive on.
                let peer_iface = self.world.ifaces[peer]
                    .iter()
                    .find(|(l, _, _)| *l == li)
                    .map(|(_, i, _)| *i)
                    .unwrap_or(0);
                for piece in pieces {
                    self.seq += 1;
                    self.queue.push(Reverse(Job {
                        at_ms: self.now_ms + link.latency_ms,
                        seq: self.seq,
                        node: peer,
                        iface: peer_iface,
                        wire: piece,
                    }));
                }
            }
        }
    }

    /// Run until the queue drains or the clock passes `until_ms`.
    fn run(&mut self, until_ms: u64, watch: Option<usize>) {
        while let Some(Reverse(job)) = self.queue.pop() {
            if job.at_ms > until_ms {
                break;
            }
            self.now_ms = job.at_ms;
            let now = self.now_secs();
            // The link layer, below the node: a fragment is not an envelope and
            // the node is never shown one.
            let whole = match &mut self.link_frag {
                Some(r) => match r[job.node].accept(job.iface as u32, &job.wire, now) {
                    Some(w) => w,
                    None => continue,
                },
                None => job.wire.clone(),
            };
            let rx = self.world.nodes[job.node].on_rx(&whole, job.iface, None, now);

            for env in &rx.delivered {
                let id = env.id();
                if self.seen_delivered[job.node].insert(id, ()).is_some() {
                    self.m.duplicate_delivered += 1;
                } else {
                    self.m.delivered += 1;
                    if watch == Some(job.node) && self.m.first_delivery_ms.is_none() {
                        self.m.first_delivery_ms = Some(self.now_ms - self.measure_from_ms);
                    }
                }
            }
            self.emit(job.node, rx.forwards);
        }
    }

    /// Let every node announce and the announces settle, so paths and prekeys
    /// exist before anything is measured. A cold mesh measures bootstrapping,
    /// not delivery.
    fn converge(&mut self) {
        for i in 0..self.world.nodes.len() {
            let now = self.now_secs();
            let f = self.world.nodes[i].build_announce(now);
            self.emit(i, f);
        }
        self.run(self.now_ms + 120_000, None);
    }

    /// Draw the line between bootstrapping and the thing being measured.
    ///
    /// Without this every metric is dominated by ANNOUNCE: convergence on a
    /// 100-node graph delivers ~9900 envelopes, so a scenario that measured
    /// totals would report a healthy mesh whether or not its actual message
    /// arrived. Ask about *this* message, in *this* window.
    fn start_measuring(&mut self) {
        self.m = Metrics::default();
        for s in &mut self.seen_delivered {
            s.clear();
        }
        self.measure_from_ms = self.now_ms;
    }

    /// Nodes that delivered at least one envelope since `start_measuring`,
    /// excluding the originator.
    fn reached(&self, origin: usize) -> usize {
        self.seen_delivered.iter().enumerate().filter(|(i, s)| *i != origin && !s.is_empty()).count()
    }
}

// ----------------------------------------------------------------- scenarios

struct Report {
    name: String,
    note: &'static str,
    reached: usize,
    of: usize,
    m: Metrics,
}

impl Report {
    fn json(&self) -> String {
        let amp = if self.m.delivered > 0 { self.m.frames_tx as f64 / self.m.delivered as f64 } else { 0.0 };
        format!(
            "  {{\"scenario\":\"{}\",\"reached\":{},\"of\":{},\"delivered\":{},\
             \"duplicates\":{},\"frames_tx\":{},\"bytes_tx\":{},\
             \"dropped_by_mtu\":{},\"dropped_by_loss\":{},\
             \"first_delivery_ms\":{},\"frames_per_delivery\":{:.2},\"note\":\"{}\"}}",
            self.name,
            self.reached,
            self.of,
            self.m.delivered,
            self.m.duplicate_delivered,
            self.m.frames_tx,
            self.m.bytes_tx,
            self.m.dropped_mtu,
            self.m.dropped_loss,
            self.m.first_delivery_ms.map(|v| v.to_string()).unwrap_or_else(|| "null".into()),
            amp,
            self.note,
        )
    }
}

/// Five nodes in a line, one uniform MTU. The control: if this does not deliver,
/// nothing measured by the other scenarios means anything.
fn line() -> Report {
    let links = (0..4).map(|i| Link { a: i, b: i + 1, mtu: 1400, loss_pct: 0, latency_ms: 10 }).collect();
    let mut sim = Sim::new(World::new(5, links), 0x5F01E);
    sim.converge();
    sim.start_measuring();
    let dest = sim.world.nodes[4].addr;
    let now = sim.now_secs();
    let f = sim.world.nodes[0].send(dest, b"hello across five".to_vec(), now).expect("fits");
    sim.emit(0, f);
    sim.run(sim.now_ms + 60_000, Some(4));
    let reached = usize::from(!sim.seen_delivered[4].is_empty());
    let _ = &sim.m;
    Report { name: "line".into(), note: "control: 5-node line, uniform MTU", reached, of: 1, m: sim.m }
}

/// **The M11-D case.** A Wi-Fi island (MTU 1400) bridged to a LoRa hop (MTU
/// 237), and a message too big for one Wi-Fi frame. Node 0 fragments at *its
/// own* MTU; those fragments cannot be fragmented again, so they die at the
/// narrow link and no node on the path can repair it.
fn mixed_mtu() -> Report {
    // 0 --wifi-- 1 --wifi-- 2 --LoRa-- 3
    let links = vec![
        Link { a: 0, b: 1, mtu: 1400, loss_pct: 0, latency_ms: 5 },
        Link { a: 1, b: 2, mtu: 1400, loss_pct: 0, latency_ms: 5 },
        Link { a: 2, b: 3, mtu: 237, loss_pct: 0, latency_ms: 120 },
    ];
    let mut sim = Sim::new(World::new(4, links), 0xA11CE);
    sim.converge();
    sim.start_measuring();
    let dest = sim.world.nodes[3].addr;
    let now = sim.now_secs();
    // 4 kB: comfortably over one Wi-Fi frame, so it fragments at 1400.
    let f = sim.world.nodes[0].send(dest, vec![0x5A; 4000], now).expect("fits one set");
    sim.emit(0, f);
    sim.run(sim.now_ms + 120_000, Some(3));
    let reached = usize::from(!sim.seen_delivered[3].is_empty());
    Report {
        name: "mixed-mtu".into(),
        note: "wifi island bridged to LoRa; M11-D must flip reached to 1",
        reached,
        of: 1,
        m: sim.m,
    }
}

/// The same topology, but node 0 has already clamped itself to the narrowest
/// link *it* owns — which it does not own here. Shows that the `n.mtu.min(...)`
/// workaround cannot help a node upstream of the narrow hop.
fn mixed_mtu_clamped() -> Report {
    let links = vec![
        Link { a: 0, b: 1, mtu: 1400, loss_pct: 0, latency_ms: 5 },
        Link { a: 1, b: 2, mtu: 1400, loss_pct: 0, latency_ms: 5 },
        Link { a: 2, b: 3, mtu: 237, loss_pct: 0, latency_ms: 120 },
    ];
    let mut sim = Sim::new(World::new(4, links), 0xB0B);
    // Every node clamps to its own narrowest link, exactly as the bridges do.
    for i in 0..sim.world.nodes.len() {
        sim.world.nodes[i].mtu = sim.world.narrowest(i);
    }
    sim.converge();
    sim.start_measuring();
    let dest = sim.world.nodes[3].addr;
    let now = sim.now_secs();
    let f = sim.world.nodes[0].send(dest, vec![0x5A; 4000], now).expect("fits one set");
    sim.emit(0, f);
    sim.run(sim.now_ms + 120_000, Some(3));
    let reached = usize::from(!sim.seen_delivered[3].is_empty());
    Report {
        name: "mixed-mtu-clamped".into(),
        note: "each node clamped to its own narrowest link; sender still owns none of the narrow hop",
        reached,
        of: 1,
        m: sim.m,
    }
}

/// **The M11-D fix, measured.** Identical topology and identical message to
/// `mixed-mtu`, with link fragmentation on: bridges split what their link cannot
/// carry, the far end reassembles, and the node never sees a piece.
///
/// Note what is *not* changed to make this work. The sender still fragments
/// end-to-end at its own MTU, exactly as it does today — those fragments are now
/// simply split again for the narrow hop and put back together on the other
/// side. That is the point: per-hop splitting repairs the path without the
/// sender knowing anything about a link three hops away.
fn mixed_mtu_linkfrag() -> Report {
    let links = vec![
        Link { a: 0, b: 1, mtu: 1400, loss_pct: 0, latency_ms: 5 },
        Link { a: 1, b: 2, mtu: 1400, loss_pct: 0, latency_ms: 5 },
        Link { a: 2, b: 3, mtu: 237, loss_pct: 0, latency_ms: 120 },
    ];
    let mut sim = Sim::new(World::new(4, links), 0xA11CE).with_link_fragmentation();
    sim.converge();
    sim.start_measuring();
    let dest = sim.world.nodes[3].addr;
    let now = sim.now_secs();
    let f = sim.world.nodes[0].send(dest, vec![0x5A; 4000], now).expect("fits one set");
    sim.emit(0, f);
    sim.run(sim.now_ms + 120_000, Some(3));
    let reached = usize::from(!sim.seen_delivered[3].is_empty());
    Report {
        name: "mixed-mtu-linkfrag".into(),
        note: "same topology and message, link fragmentation on",
        reached,
        of: 1,
        m: sim.m,
    }
}

/// A hundred nodes, a random connected graph, lossy links: delivery probability
/// and flood amplification for a public post.
fn lossy_mesh(loss_pct: u32) -> Report {
    const N: usize = 100;
    let mut rng = Rng(0xC0FFEE ^ loss_pct as u64);
    let mut links = Vec::new();
    // A spanning path guarantees connectivity, then extra chords for redundancy.
    for i in 0..N - 1 {
        links.push(Link { a: i, b: i + 1, mtu: 1400, loss_pct, latency_ms: 8 });
    }
    for _ in 0..N {
        let a = rng.below(N);
        let b = rng.below(N);
        if a != b {
            links.push(Link { a, b, mtu: 1400, loss_pct, latency_ms: 8 });
        }
    }
    // Everyone follows the topic. A publish is delivered only to subscribers,
    // so a mesh where nobody subscribed measures nothing at all.
    let mut sim = Sim::new(World::with_topics(N, links, &["weather"]), 0xD15EA5E ^ loss_pct as u64);
    sim.converge();
    sim.start_measuring();
    let now = sim.now_secs();
    let f = sim.world.nodes[0].publish("weather", b"squall on the ridge".to_vec(), now);
    sim.emit(0, f);
    sim.run(sim.now_ms + 120_000, None);
    let reached = sim.reached(0);
    let note: &'static str = match loss_pct {
        0 => "100 nodes, no loss",
        10 => "100 nodes, 10% loss",
        _ => "100 nodes, 50% loss",
    };
    Report { name: format!("lossy-mesh-{loss_pct}pct"), note, reached, of: N - 1, m: sim.m }
}

/// **The M11-I case.** A file three hops away: the root has flooded, so every
/// node knows the magnet, and the fetcher asks for the chunks.
///
/// It does not arrive, and the reason is structural rather than lossy. WANT is
/// consumed at the neighbour and never relayed, so the request reaches a node
/// that holds no chunks and dies there. Custody push is unicast mail to a
/// person, not "anyone who wants this id". If the seeder does not walk toward
/// you, the file does not move.
///
/// Reported, not asserted: M11-I flipping this to 1 is the result to look for.
fn file_multihop() -> Report {
    let links = (0..3).map(|i| Link { a: i, b: i + 1, mtu: 1400, loss_pct: 0, latency_ms: 10 }).collect();
    let mut sim = Sim::new(World::new(4, links), 0xF11E);
    let now = sim.now_secs();
    let bytes = vec![0xAB; 6000];
    let (magnet, fwds) = sim.world.nodes[0].publish_file("far.bin", &bytes, ZERO_DEST, now);
    sim.emit(0, fwds);
    sim.run(sim.now_ms + 60_000, None);

    // Everyone knows it exists — the flooded root doing its job — which is what
    // makes the failure specific: this is not "the mesh never heard of it".
    let knows = (0..4).filter(|i| sim.world.nodes[*i].file_name(&magnet).is_some()).count();
    assert_eq!(knows, 4, "the root should reach every node; the scenario is meaningless otherwise");

    sim.start_measuring();
    // Ask repeatedly: one fetch names one frame of ids, and a tree resolves
    // top-down — the root's children have to arrive before the chunks beneath
    // them can even be named. A real client polls the same way.
    for _ in 0..12 {
        let want = sim.world.nodes[3].fetch_n(&magnet, 4);
        if want.is_empty() {
            break;
        }
        sim.emit(3, want);
        sim.run(sim.now_ms + 30_000, None);
    }

    let reached = usize::from(sim.world.nodes[3].has_file(&magnet));
    // The middle nodes fetched on the far node's behalf, so they hold what they
    // passed along — a helper relay caches. That is what makes a second fetcher
    // on this path cheap, and it is why the interest table serves *all* waiters
    // for an id rather than only the first.
    let cached: usize = (1..3).map(|i| sim.world.nodes[i].store_len()).sum();
    let note: &'static str = if reached == 1 {
        "a file crossed three hops; the middle nodes cached what they carried"
    } else {
        "all four know the magnet; the fetcher three hops away gets nothing — M11-I"
    };
    assert!(
        reached == 0 || cached > 2,
        "helper relays should hold what they passed on, got {cached} envelopes across the middle"
    );
    Report { name: "file-multihop".into(), note, reached, of: 1, m: sim.m }
}

/// Two clusters joined by a single node: does a partition heal through one
/// bridge, and what does it cost?
///
/// Eleven nodes, not twenty-one. The first version of this scenario was a
/// 21-node line and delivered nothing — not a bug, a **measured property**: a
/// line of 21 has diameter 20 and envelopes start at `hops: 16`, so the far end
/// is unreachable by construction. See `hop-limit`, which measures that on
/// purpose instead of stumbling into it.
fn partition() -> Report {
    // 0..4 | bridge 5 | 6..10 — diameter 10, inside the hop budget.
    let mut links = Vec::new();
    for i in 0..4 {
        links.push(Link { a: i, b: i + 1, mtu: 1400, loss_pct: 5, latency_ms: 8 });
    }
    links.push(Link { a: 4, b: 5, mtu: 1400, loss_pct: 5, latency_ms: 40 });
    links.push(Link { a: 5, b: 6, mtu: 1400, loss_pct: 5, latency_ms: 40 });
    for i in 6..10 {
        links.push(Link { a: i, b: i + 1, mtu: 1400, loss_pct: 5, latency_ms: 8 });
    }
    let mut sim = Sim::new(World::new(11, links), 0xFACADE);
    sim.converge();
    sim.start_measuring();
    let dest = sim.world.nodes[10].addr;
    let now = sim.now_secs();
    let f = sim.world.nodes[0].send(dest, b"across the join".to_vec(), now).expect("fits");
    sim.emit(0, f);
    sim.run(sim.now_ms + 180_000, Some(10));
    let reached = usize::from(!sim.seen_delivered[10].is_empty());
    Report {
        name: "partition".into(),
        note: "two 5-node clusters joined by one bridge, 5% loss",
        reached,
        of: 1,
        m: sim.m,
    }
}

/// How far an envelope actually travels. A line of `len` nodes, one DM end to
/// end, no loss — so the only thing that can stop it is the hop budget.
///
/// Worth measuring rather than asserting from the constant: `hops: 16` is the
/// *initial* value and §5 decrements per relay, so the reachable diameter is a
/// property of the forwarding rules, not of the number alone. A protocol change
/// that quietly costs an extra hop shows up here and nowhere else.
fn hop_limit(len: usize) -> Report {
    let links =
        (0..len - 1).map(|i| Link { a: i, b: i + 1, mtu: 1400, loss_pct: 0, latency_ms: 2 }).collect();
    let mut sim = Sim::new(World::new(len, links), 0x409E);
    sim.converge();
    sim.start_measuring();
    let dest = sim.world.nodes[len - 1].addr;
    let now = sim.now_secs();
    let f = sim.world.nodes[0].send(dest, b"how far".to_vec(), now).expect("fits");
    sim.emit(0, f);
    sim.run(sim.now_ms + 120_000, Some(len - 1));
    let reached = usize::from(!sim.seen_delivered[len - 1].is_empty());
    Report {
        name: format!("hop-limit-{len}"),
        note: if reached == 1 { "reached the far end" } else { "hop budget exhausted before the far end" },
        reached,
        of: 1,
        m: sim.m,
    }
}

// --------------------------------------------------------------------- main

fn main() {
    let which = std::env::args().nth(1).unwrap_or_else(|| "smoke".into());
    let reports: Vec<Report> = match which.as_str() {
        "line" => vec![line()],
        "mixed-mtu" => vec![mixed_mtu(), mixed_mtu_clamped(), mixed_mtu_linkfrag()],
        "lossy" => vec![lossy_mesh(0), lossy_mesh(10), lossy_mesh(50)],
        "partition" => vec![partition()],
        "files" => vec![file_multihop()],
        "hop-limit" => vec![hop_limit(8), hop_limit(17), hop_limit(19)],
        _ => vec![
            line(),
            mixed_mtu(),
            mixed_mtu_clamped(),
            mixed_mtu_linkfrag(),
            lossy_mesh(0),
            lossy_mesh(10),
            partition(),
            file_multihop(),
            hop_limit(17),
            hop_limit(19),
        ],
    };

    println!("[");
    for (i, r) in reports.iter().enumerate() {
        println!("{}{}", r.json(), if i + 1 < reports.len() { "," } else { "" });
    }
    println!("]");

    // Regression thresholds. Only on the scenarios whose behaviour is known
    // good — `mixed-mtu` is *reported*, not asserted, because it is the bug and
    // failing the build on a known bug teaches people to ignore the build.
    let mut bad = Vec::new();
    for r in &reports {
        match r.name.as_str() {
            // 17 nodes is 16 relays, exactly the hop budget: it must arrive, and
            // if a forwarding change ever costs one more hop this is where it shows.
            // The M11-D acceptance test: with link fragmentation the narrow hop
            // is no longer fatal, and if that ever stops being true it is a
            // regression rather than a known bug.
            "line" | "partition" | "hop-limit-17" | "mixed-mtu-linkfrag" | "file-multihop" => {
                if r.reached != r.of {
                    bad.push(format!("{}: reached {} of {}", r.name, r.reached, r.of));
                }
            }
            n if n.starts_with("lossy-mesh") => {
                // Even at 50% per-link loss a flood should reach most of a
                // redundant graph; well under half means something regressed.
                let floor = r.of / 2;
                if r.reached < floor {
                    bad.push(format!("{} ({}): reached {} of {}", r.name, r.note, r.reached, r.of));
                }
            }
            _ => {}
        }
    }
    if !bad.is_empty() {
        eprintln!("\nspore-sim regressions:");
        for b in &bad {
            eprintln!("  {b}");
        }
        std::process::exit(1);
    }
    eprintln!("\nspore-sim OK — thresholds met");
}
