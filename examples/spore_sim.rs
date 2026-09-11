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
    /// Repair symbols per fragmented set. Needs no return path, no timer and no
    /// sender buffer, so it works on broadcast and simplex media too.
    repair: usize,
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
            repair: 0,
        }
    }

    /// Turn on link fragmentation: bridges split what their link cannot carry
    /// and the far end reassembles before the node sees anything.
    fn with_link_fragmentation(mut self) -> Self {
        let n = self.world.nodes.len();
        self.link_frag = Some((0..n).map(|_| Reassembler::default()).collect());
        self
    }

    /// Send `r` extra copies of randomly chosen pieces alongside each split set.
    fn with_repair(mut self, r: usize) -> Self {
        self.repair = r;
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
                    // Real repair symbols now, not repetition: any n of the
                    // n+r sent will do, rather than needing the duplicate to
                    // land on the gap.
                    linkfrag::split_with_repair(&wire, link.mtu, self.next_set_id, self.repair)
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
                // Which iface the *peer* sees this arrive on.
                let peer_iface = self.world.ifaces[peer]
                    .iter()
                    .find(|(l, _, _)| *l == li)
                    .map(|(_, i, _)| *i)
                    .unwrap_or(0);
                // Loss is per *piece*, not per envelope. A radio drops frames,
                // not messages, and the difference is the whole question: an
                // envelope split into n pieces survives only if all n arrive, so
                // a link that loses 10% of frames loses far more than 10% of
                // fragmented envelopes. Dropping the set as a unit would give
                // the right answer today and make recovery unmeasurable.
                for piece in pieces {
                    if self.rng.chance(link.loss_pct) {
                        self.m.dropped_loss += 1;
                        continue;
                    }
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
    note: String,
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
    let f =
        sim.world.nodes[0].send(dest, b"hello across five".to_vec(), now).expect("well under the ceiling");
    sim.emit(0, f);
    sim.run(sim.now_ms + 60_000, Some(4));
    let reached = usize::from(!sim.seen_delivered[4].is_empty());
    let _ = &sim.m;
    Report {
        name: "line".into(),
        note: "control: 5-node line, uniform MTU".to_string(),
        reached,
        of: 1,
        m: sim.m,
    }
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
    // 4 kB: one envelope now, over one Wi-Fi frame. The link splits it.
    let f = sim.world.nodes[0].send(dest, vec![0x5A; 4000], now).expect("well under the ceiling");
    sim.emit(0, f);
    sim.run(sim.now_ms + 120_000, Some(3));
    let reached = usize::from(!sim.seen_delivered[3].is_empty());
    Report {
        name: "mixed-mtu".into(),
        note: "wifi island bridged to LoRa; M11-D must flip reached to 1".to_string(),
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
    let f = sim.world.nodes[0].send(dest, vec![0x5A; 4000], now).expect("well under the ceiling");
    sim.emit(0, f);
    sim.run(sim.now_ms + 120_000, Some(3));
    let reached = usize::from(!sim.seen_delivered[3].is_empty());
    Report {
        name: "mixed-mtu-clamped".into(),
        note: "each node clamped to its own narrowest link; sender still owns none of the narrow hop"
            .to_string(),
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
    let f = sim.world.nodes[0].send(dest, vec![0x5A; 4000], now).expect("well under the ceiling");
    sim.emit(0, f);
    sim.run(sim.now_ms + 120_000, Some(3));
    let reached = usize::from(!sim.seen_delivered[3].is_empty());
    Report {
        name: "mixed-mtu-linkfrag".into(),
        note: "same topology and message, link fragmentation on".to_string(),
        reached,
        of: 1,
        m: sim.m,
    }
}

/// **What splitting costs on a lossy link (M11-C).**
///
/// An envelope cut into n pieces survives only if all n arrive, so a link that
/// drops `loss_pct` of *frames* drops far more than that of *fragmented
/// envelopes* — `(1 - p)^n`, which falls off a cliff. There is no recovery at
/// the link layer yet: a lost piece silently costs the whole envelope for that
/// hop, and the sender never learns.
///
/// Run over many attempts, because one trial of a coin flip measures nothing.
fn linkfrag_loss(loss_pct: u32, attempts: usize) -> Report {
    let mut arrived = 0usize;
    let mut m = Metrics::default();
    for trial in 0..attempts {
        let links = vec![Link { a: 0, b: 1, mtu: 237, loss_pct, latency_ms: 20 }];
        let mut sim = Sim::new(World::new(2, links), 0x10551 ^ trial as u64).with_link_fragmentation();
        sim.converge();
        sim.start_measuring();
        let dest = sim.world.nodes[1].addr;
        let now = sim.now_secs();
        // 900 B stays under the node's own MTU, so `Node::send` does *not*
        // fountain-fragment it end to end and this measures the link layer
        // alone. A larger payload measures both codes at once and the numbers
        // stop matching any single theory.
        let f = sim.world.nodes[0].send(dest, vec![0x33; 900], now).expect("well under the ceiling");
        sim.emit(0, f);
        sim.run(sim.now_ms + 60_000, Some(1));
        if !sim.seen_delivered[1].is_empty() {
            arrived += 1;
        }
        m.frames_tx += sim.m.frames_tx;
        m.bytes_tx += sim.m.bytes_tx;
        m.dropped_loss += sim.m.dropped_loss;
    }
    let note: String = match loss_pct {
        0 => "900 B over a 237-byte link, no loss".to_string(),
        5 => "900 B over a 237-byte link, 5% frame loss".to_string(),
        10 => "900 B over a 237-byte link, 10% frame loss".to_string(),
        _ => "900 B over a 237-byte link, 20% frame loss".to_string(),
    };
    Report { name: format!("linkfrag-loss-{loss_pct}pct"), note, reached: arrived, of: attempts, m }
}

/// The same measurement with `r` repair symbols per set — the erasure code, so
/// any n of the n+r sent will reconstruct. Compare against the repetition
/// numbers this replaced: at 10% loss and 40% extra traffic, repetition
/// recovered 70%.
fn linkfrag_repair(loss_pct: u32, repair: usize, attempts: usize) -> Report {
    let mut arrived = 0usize;
    let mut m = Metrics::default();
    for trial in 0..attempts {
        let links = vec![Link { a: 0, b: 1, mtu: 237, loss_pct, latency_ms: 20 }];
        let mut sim = Sim::new(World::new(2, links), 0x10551 ^ trial as u64)
            .with_link_fragmentation()
            .with_repair(repair);
        sim.converge();
        sim.start_measuring();
        let dest = sim.world.nodes[1].addr;
        let now = sim.now_secs();
        let f = sim.world.nodes[0].send(dest, vec![0x33; 900], now).expect("well under the ceiling");
        sim.emit(0, f);
        sim.run(sim.now_ms + 60_000, Some(1));
        if !sim.seen_delivered[1].is_empty() {
            arrived += 1;
        }
        m.frames_tx += sim.m.frames_tx;
        m.bytes_tx += sim.m.bytes_tx;
        m.dropped_loss += sim.m.dropped_loss;
    }
    Report {
        name: format!("linkfrag-{loss_pct}pct-repair{repair}"),
        note: "900 B over a 237-byte link, erasure-coded repair".to_string(),
        reached: arrived,
        of: attempts,
        m,
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
    let note: String = match loss_pct {
        0 => "100 nodes, no loss".to_string(),
        10 => "100 nodes, 10% loss".to_string(),
        _ => "100 nodes, 50% loss".to_string(),
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
    // **With link fragmentation.** Chunks are a protocol-fixed size now (M11-M)
    // rather than cut to the publisher's MTU, so they are routinely larger than
    // a link's frame and rely on the bridge splitting them. That is the whole
    // point — a publisher cannot see the narrowest hop on a path — but it makes
    // per-hop splitting a *dependency* of the file layer rather than an
    // optimisation: without it, this scenario delivers nothing.
    let mut sim = Sim::new(World::new(4, links), 0xF11E).with_link_fragmentation();
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
    let note: String = if reached == 1 {
        "a file crossed three hops; the middle nodes cached what they carried".to_string()
    } else {
        "all four know the magnet; the fetcher three hops away gets nothing — M11-I".to_string()
    };
    assert!(
        reached == 0 || cached > 2,
        "helper relays should hold what they passed on, got {cached} envelopes across the middle"
    );
    Report { name: "file-multihop".into(), note, reached, of: 1, m: sim.m }
}

/// What does one forged WANT cost a mesh? (M11-L)
///
/// Recursive pull's amplification story was *reasoned*: a node adopts only ids a
/// manifest it holds names, the interest table dedupes so traffic is linear in
/// nodes reached rather than exponential in paths to them, and `DEFAULT_WANT_DEPTH`
/// bounds how far one neighbour's curiosity travels. Two of those three are
/// enforced by the node. The third is a **number the asker supplies**.
///
/// Depth rides as a trailing byte on the WANT payload, and `on_want` reads it
/// verbatim. `DEFAULT_WANT_DEPTH` is only the fallback for a WANT that carries no
/// byte at all — so a node that simply writes `255` there buys 255 hops of
/// recursion instead of 8.
///
/// The topology is a line longer than the honest depth, so reach is the thing
/// being measured: with an honest budget the interest stops partway along it,
/// and with a forged one it should not.
///
/// The ids asked for are **legitimate** — named by a manifest every node holds —
/// because that is the interesting case. An id nobody has a manifest for is
/// already refused (`a_want_for_an_id_no_manifest_names_starts_no_hunt`); this
/// measures what the gate still lets through.
fn malicious_want(depth: u8) -> Report {
    const N: usize = 24;
    let links = (0..N - 1).map(|i| Link { a: i, b: i + 1, mtu: 1400, loss_pct: 0, latency_ms: 5 }).collect();
    let mut sim = Sim::new(World::new(N, links), 0xBADDA7A);
    let now = sim.now_secs();

    // The publisher is outside the mesh: every node learns the file exists, and
    // none of them holds a byte of it. That isolates the measurement — nobody
    // can answer, so every interest adopted along the way stays adopted.
    let mut publisher = Node::new("publisher", &[]);
    let (magnet, published) = publisher.publish_file("bait.bin", &vec![0x11; 40_000], ZERO_DEST, now);
    let root = match &published[0] {
        Forward::Flood { bytes, .. } | Forward::Directed { bytes, .. } => bytes.clone(),
    };
    for i in 0..N {
        sim.world.nodes[i].on_rx(&root, 0, None, now);
    }

    // One legitimate id, asked for once, from one end of the line.
    let target = sim.world.nodes[N - 1].missing(&magnet, 1);
    assert!(!target.is_empty(), "the manifest names something to ask for");
    let mut payload: Vec<u8> = target[0].to_vec();
    payload.push(depth);
    let want = Envelope::new(ty::WANT, ZERO_DEST, 0, payload).wire();

    sim.start_measuring();
    sim.emit(N - 1, vec![Forward::Flood { except: NO_IFACE, bytes: want }]);
    sim.run(sim.now_ms + 120_000, None);

    // How far did one frame reach, and how many nodes took on an obligation?
    let holding = (0..N).filter(|i| sim.world.nodes[*i].open_interests() > 0).count();
    Report {
        name: format!("malicious-want-depth{depth}"),
        note: format!(
            "one WANT claiming depth {depth}: {holding} of {N} nodes now hold an interest nobody can serve"
        ),
        reached: holding,
        of: N,
        m: sim.m,
    }
}

/// The fetcher walks away mid-transfer. How long does the mesh keep hunting?
///
/// This is the cost M11-K exists to remove, and it is worth measuring rather
/// than asserting: an adopted interest is a *standing* obligation, so a chain of
/// relays that has adopted one keeps asking every seeder in range until the
/// lease expires. With a fifteen-minute wall-clock lease — right for sneakernet,
/// where a courier really may be fifteen minutes away — a popular magnet plus a
/// flaky requester is a permanent load nobody asked for.
///
/// Three ways a fetch can end, measured side by side:
///   * **vanishes** — nothing is said; only the lease stops it. The backstop.
///   * **cancels** — the fetcher says so, and the cancel unwinds the chain.
///   * **link drops** — the fetcher cannot say so, but its neighbour can tell.
fn fetch_abandoned() -> Report {
    // How many interests are still open across the relays, after each ending.
    fn open_after(ending: Ending) -> usize {
        let links = (0..3).map(|i| Link { a: i, b: i + 1, mtu: 1400, loss_pct: 0, latency_ms: 10 }).collect();
        let mut sim = Sim::new(World::new(4, links), 0xCA7CE1).with_link_fragmentation();
        let now = sim.now_secs();
        // Non-repeating: since M11-M chunks are content-addressed, so 60 kB of one
        // byte is two distinct objects, not fifty. This scenario is about a
        // *standing pull* with many outstanding parts, so it needs a file that
        // actually has many parts.
        let bytes: Vec<u8> = (0..15_000u32).flat_map(|i| i.to_be_bytes()).collect();
        let (magnet, fwds) = sim.world.nodes[0].publish_file("gone.bin", &bytes, ZERO_DEST, now);
        sim.emit(0, fwds);
        sim.run(sim.now_ms + 60_000, None);

        // Let the pull get going properly first. A tree resolves top-down, so the
        // relays have to actually carry some of it before they hold the
        // sub-manifests that make a deeper id legal to adopt — `may_adopt` is a
        // capability check, not a guess, and starving the chain from the start
        // would just make every node correctly refuse.
        for _ in 0..4 {
            let want = sim.world.nodes[3].fetch_n(&magnet, 4);
            if want.is_empty() {
                break;
            }
            sim.emit(3, want);
            sim.run(sim.now_ms + 30_000, None);
        }

        // Now the seeder goes away — out of range, powered down. This is what
        // makes an interest *stand open* rather than resolve in a millisecond:
        // the relays know the file exists and what it names, they have adopted
        // the obligation to find it, and the only node holding the remaining
        // bytes is unreachable. An interest that gets served retires itself; the
        // expensive case is the one that never can.
        sim.world.links[0].loss_pct = 100;

        let mut adopted = 0;
        for _ in 0..8 {
            let want = sim.world.nodes[3].fetch_n(&magnet, 4);
            if want.is_empty() {
                break;
            }
            sim.emit(3, want);
            sim.run(sim.now_ms + 30_000, None);
            adopted = (1..3).map(|i| sim.world.nodes[i].open_interests()).sum();
            if adopted > 0 {
                break;
            }
        }
        assert!(adopted > 0, "the relays must have adopted something for the ending to matter");

        match ending {
            Ending::Vanishes => {}
            // The fetcher says it is leaving. `abandon_all` rather than
            // `abandon(&magnet)`: a departing node cannot reliably enumerate
            // what it asked for — see `on_cancel` — and this scenario is about
            // leaving, not about dropping one file.
            Ending::Cancels => {
                let bye = sim.world.nodes[3].abandon_all();
                sim.emit(3, bye);
                sim.run(sim.now_ms + 30_000, None);
            }
            Ending::LinkDrops => {
                // Node 2 notices the link to 3 is gone. Node 2 sits on links
                // (1,2) and (2,3), so the fetcher is behind its second interface.
                let unwind = sim.world.nodes[2].forget_interests_on(1);
                sim.emit(2, unwind);
                sim.run(sim.now_ms + 30_000, None);
            }
        }
        (1..3).map(|i| sim.world.nodes[i].open_interests()).sum()
    }

    let vanished = open_after(Ending::Vanishes);
    let cancelled = open_after(Ending::Cancels);
    let dropped = open_after(Ending::LinkDrops);

    // The measurement that matters: saying so must beat saying nothing.
    assert!(
        cancelled < vanished,
        "a cancel should retire interests a silent departure leaves standing ({cancelled} vs {vanished})"
    );
    assert!(dropped < vanished, "a dropped link should too ({dropped} vs {vanished})");
    let reached = usize::from(cancelled == 0 && dropped == 0);
    Report {
        name: "fetch-abandoned".into(),
        note: format!(
            "interests still open across the relays — vanished: {vanished}, cancelled: {cancelled}, link dropped: {dropped}"
        ),
        reached,
        of: 1,
        m: Metrics::default(),
    }
}

enum Ending {
    Vanishes,
    Cancels,
    LinkDrops,
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
    let f = sim.world.nodes[0].send(dest, b"across the join".to_vec(), now).expect("well under the ceiling");
    sim.emit(0, f);
    sim.run(sim.now_ms + 180_000, Some(10));
    let reached = usize::from(!sim.seen_delivered[10].is_empty());
    Report {
        name: "partition".into(),
        note: "two 5-node clusters joined by one bridge, 5% loss".to_string(),
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
    let f = sim.world.nodes[0].send(dest, b"how far".to_vec(), now).expect("well under the ceiling");
    sim.emit(0, f);
    sim.run(sim.now_ms + 120_000, Some(len - 1));
    let reached = usize::from(!sim.seen_delivered[len - 1].is_empty());
    Report {
        name: format!("hop-limit-{len}"),
        note: if reached == 1 { "reached the far end" } else { "hop budget exhausted before the far end" }
            .to_string(),
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
        "linkfrag-loss" => vec![
            linkfrag_loss(0, 200),
            linkfrag_loss(5, 200),
            linkfrag_loss(10, 200),
            linkfrag_loss(20, 200),
            linkfrag_repair(5, 1, 200),
            linkfrag_repair(10, 1, 200),
            linkfrag_repair(20, 1, 200),
            linkfrag_repair(10, 2, 200),
            linkfrag_repair(10, 4, 200),
            linkfrag_repair(10, 7, 200),
            linkfrag_repair(20, 7, 200),
            linkfrag_repair(20, 14, 200),
        ],
        "partition" => vec![partition()],
        "files" => vec![file_multihop(), fetch_abandoned()],
        "malicious" => vec![malicious_want(DEFAULT_WANT_DEPTH), malicious_want(255)],
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
            fetch_abandoned(),
            // The fragment-loss cliff, kept in the smoke suite as a standing
            // record: only the clean case is asserted, because the rest are
            // probabilities and a threshold on a coin flip is a flaky build.
            linkfrag_loss(0, 200),
            linkfrag_loss(10, 200),
            linkfrag_repair(5, 1, 200),
            linkfrag_repair(10, 1, 200),
            linkfrag_repair(20, 1, 200),
            linkfrag_repair(10, 2, 200),
            hop_limit(17),
            hop_limit(19),
            // M11-L: one forged frame against a 24-node line. Cheap, and the
            // only scenario whose failure mode is reaching *too far*.
            malicious_want(DEFAULT_WANT_DEPTH),
            malicious_want(255),
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
            "line" | "partition" | "hop-limit-17" | "mixed-mtu-linkfrag" | "file-multihop"
            | "fetch-abandoned" | "linkfrag-loss-0pct" => {
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

    // M11-L, and the one threshold phrased as "no *more* than". Every other
    // scenario here fails by delivering too little; this one fails by a stranger
    // reaching too far. A forged depth must buy exactly what an honest one does,
    // so the two reports are compared against each other rather than against a
    // fixed number — the honest reach is allowed to change, the gap is not.
    let reach = |name: &str| reports.iter().find(|r| r.name == name).map(|r| r.reached);
    if let (Some(honest), Some(forged)) =
        (reach(&format!("malicious-want-depth{DEFAULT_WANT_DEPTH}")), reach("malicious-want-depth255"))
    {
        if forged > honest {
            bad.push(format!(
                "a forged WANT depth reaches {forged} nodes against an honest {honest} — the depth byte is attacker-supplied and must be clamped (M11-L)"
            ));
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
