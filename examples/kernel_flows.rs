//! Emit `docs/KERNEL_FLOWS.md` — what the kernel actually does, in a few
//! scenarios, drawn from the calls rather than from anyone's memory of them.
//!
//!   cargo run --release --example kernel_flows > docs/KERNEL_FLOWS.md
//!
//! **Why generated.** The spec says what the rules are and the threat model says
//! what they defend, but neither shows a message *moving*. A hand-drawn sequence
//! diagram would be a claim about the code, and this project has already been
//! bitten twice by exactly that: the forwarding table in
//! `reference/versioned_vectors.json` was prose until it was observed, and the
//! prose had `hops` counting the wrong way and claimed a bad signature stops a
//! relay. Every arrow below was recorded from a real `Node` call.
//!
//! **What this is not.** It is not the simulator (`spore_sim`), which measures a
//! network at scale under loss and adversaries. These are small, exact, readable
//! traces: three or four nodes, every frame accounted for, so a reader can
//! follow one message end to end.

use spore::*;
use std::collections::HashMap;

/// One recorded step: who did what to whom.
enum Step {
    /// `from -> to: label`
    Msg { from: String, to: String, label: String },
    /// Something a node did to itself — a decision, a drop, a delivery.
    Note { at: String, label: String },
}

/// A scenario being recorded.
struct Trace {
    title: &'static str,
    /// Why this scenario is worth a diagram at all.
    why: &'static str,
    steps: Vec<Step>,
    /// Facts counted while running, printed under the diagram.
    facts: Vec<String>,
}

impl Trace {
    fn new(title: &'static str, why: &'static str) -> Trace {
        Trace { title, why, steps: Vec::new(), facts: Vec::new() }
    }

    fn msg(&mut self, from: &str, to: &str, label: &str) {
        self.steps.push(Step::Msg { from: from.into(), to: to.into(), label: sanitise(label) });
    }

    fn note(&mut self, at: &str, label: &str) {
        self.steps.push(Step::Note { at: at.into(), label: sanitise(label) });
    }

    fn fact(&mut self, f: String) {
        self.facts.push(f);
    }

    fn mermaid(&self) -> String {
        let mut o = String::from("```mermaid\nsequenceDiagram\n");
        // Participants are declared in first-seen order so the columns read
        // left to right in the order the story happens.
        let mut seen: Vec<String> = Vec::new();
        for s in &self.steps {
            match s {
                Step::Msg { from, to, .. } => {
                    for p in [from, to] {
                        if !seen.contains(p) {
                            seen.push(p.clone());
                        }
                    }
                }
                Step::Note { at, .. } => {
                    if !seen.contains(at) {
                        seen.push(at.clone());
                    }
                }
            }
        }
        for p in &seen {
            o.push_str(&format!("  participant {p}\n"));
        }
        for s in &self.steps {
            match s {
                Step::Msg { from, to, label } => o.push_str(&format!("  {from}->>{to}: {label}\n")),
                Step::Note { at, label } => o.push_str(&format!("  Note over {at}: {label}\n")),
            }
        }
        o.push_str("```\n");
        o
    }
}

/// Mermaid labels end at a newline and take `:` as a separator, so a path like
/// `Node::on_rx` splits a label in two. Same fix as `state_machines.rs`.
fn sanitise(label: &str) -> String {
    label.replace("::", " ").replace(':', " —").replace(';', ",")
}

/// A tiny mesh: named nodes, and a delivery function that moves forwards between
/// them exactly as a transport would.
struct Mesh {
    names: Vec<String>,
    nodes: Vec<Node>,
    /// `node -> [(peer, this node's interface id for that link)]`.
    ///
    /// Each link gets its **own** interface id, which is what makes the trace
    /// faithful: `Forward::Flood` carries an `except`, and a mesh that gave every
    /// link the same id could not honour it. The first version of this file did
    /// exactly that, and drew a frame going straight back to the sender — an
    /// arrow no real transport would ever emit.
    links: HashMap<usize, Vec<(usize, Iface)>>,
    frames: usize,
    bytes: usize,
}

impl Mesh {
    fn new(spec: &[(&str, &[&str])]) -> Mesh {
        let mut names = Vec::new();
        let mut nodes = Vec::new();
        for (i, (name, topics)) in spec.iter().enumerate() {
            names.push(name.to_string());
            let mut seed = [0u8; 32];
            seed[0] = (i + 1) as u8;
            nodes.push(Node::from_seed(name, topics, &seed));
        }
        Mesh { names, nodes, links: HashMap::new(), frames: 0, bytes: 0 }
    }

    fn link(&mut self, a: usize, b: usize) {
        let ia = self.links.entry(a).or_default().len() as Iface;
        self.links.entry(a).or_default().push((b, ia));
        let ib = self.links.entry(b).or_default().len() as Iface;
        self.links.entry(b).or_default().push((a, ib));
    }

    /// Which of `node`'s interfaces faces `peer`.
    fn iface_to(&self, node: usize, peer: usize) -> Iface {
        self.links.get(&node).and_then(|l| l.iter().find(|(p, _)| *p == peer)).map(|(_, i)| *i).unwrap_or(0)
    }

    fn idx(&self, name: &str) -> usize {
        self.names.iter().position(|n| n == name).expect("unknown node")
    }

    fn addr(&self, name: &str) -> Addr {
        self.nodes[self.idx(name)].addr
    }

    /// Hand `forwards` to the mesh and keep delivering until nothing is left to
    /// deliver, recording every hop. Returns how many nodes delivered to an app.
    fn run(&mut self, t: &mut Trace, origin: &str, forwards: Vec<Forward>, now: u32) -> usize {
        let mut queue: Vec<(usize, Iface, Vec<u8>)> = Vec::new();
        let from = self.idx(origin);
        for f in forwards {
            self.fan_out(t, from, &f, &mut queue);
        }

        let mut delivered = 0usize;
        // Bounded so a routing bug in the kernel shows up as a diagram that
        // stops rather than as a generator that never returns.
        for _ in 0..64 {
            if queue.is_empty() {
                break;
            }
            let batch = std::mem::take(&mut queue);
            for (to, on_iface, wire) in batch {
                // Held before and after, because `Rx` says what came *out* and
                // the interesting case here is what stayed *in*. Without this the
                // diagram calls a node that took custody of a message "dropped",
                // which is the opposite of what happened — and custody is the one
                // behaviour this protocol is named for.
                let before = self.nodes[to].store_len();
                let rx = self.nodes[to].on_rx(&wire, on_iface, None, now);
                let stored = self.nodes[to].store_len() > before;

                if !rx.delivered.is_empty() {
                    delivered += rx.delivered.len();
                    t.note(&self.names[to].clone(), "delivered to the app");
                }
                if rx.delivered.is_empty() && rx.forwards.is_empty() {
                    let why = if stored {
                        "kept for later — nowhere onward to send it"
                    } else {
                        "already seen — dropped"
                    };
                    t.note(&self.names[to].clone(), why);
                }
                for f in &rx.forwards {
                    self.fan_out(t, to, f, &mut queue);
                }
            }
        }
        delivered
    }

    fn fan_out(&mut self, t: &mut Trace, from: usize, f: &Forward, queue: &mut Vec<(usize, Iface, Vec<u8>)>) {
        let mine = self.links.get(&from).cloned().unwrap_or_default();
        let (bytes, targets): (&Vec<u8>, Vec<(usize, Iface)>) = match f {
            // Every interface except the one it arrived on. Honouring `except` is
            // what keeps this a trace rather than an illustration.
            Forward::Flood { except, bytes } => {
                (bytes, mine.into_iter().filter(|(_, i)| i != except).collect())
            }
            // Back down one link. The kernel names the interface; this looks up
            // which peer is on the other end of it.
            Forward::Directed { iface, bytes, .. } => {
                (bytes, mine.into_iter().filter(|(_, i)| i == iface).collect())
            }
        };
        let label = describe(bytes);
        for (to, _) in targets {
            self.frames += 1;
            self.bytes += bytes.len();
            t.msg(&self.names[from].clone(), &self.names[to].clone(), &label);
            let on = self.iface_to(to, from);
            queue.push((to, on, bytes.clone()));
        }
    }
}

/// What a frame is, from its own bytes — the same thing a receiver sees.
fn describe(wire: &[u8]) -> String {
    let Ok((e, _)) = Envelope::decode(wire) else {
        return format!("{} bytes, unparseable", wire.len());
    };
    let kind = match e.typ {
        ty::DATA => "DATA",
        ty::INV => "INV",
        ty::WANT => "WANT",
        ty::ANNOUNCE => "ANNOUNCE",
        _ => "other",
    };
    let mut bits = vec![format!("{kind} hops={}", e.hops)];
    if e.flags & fl::FLOOD != 0 {
        bits.push("FLOOD".into());
    }
    if e.flags & fl::SIGNED != 0 {
        bits.push("signed".into());
    }
    if e.flags & fl::ENCRYPTED != 0 {
        bits.push("sealed".into());
    }
    format!("{} ({} B)", bits.join(" "), wire.len())
}

// ---------------------------------------------------------------------------
// the scenarios
// ---------------------------------------------------------------------------

/// A public message crossing a line of nodes.
fn flood() -> Trace {
    let mut t = Trace::new(
        "A public message floods",
        "The ordinary path, and the one every other scenario is a variation of. \
         Worth drawing because the interesting part is not that it arrives — it is \
         that it stops.",
    );
    // A triangle rather than a line, because a line never produces a duplicate
    // and a duplicate is the interesting part. Everyone can hear everyone.
    let mut m = Mesh::new(&[("Ana", &["news"][..]), ("Ben", &["news"][..]), ("Cai", &["news"][..])]);
    m.link(0, 1);
    m.link(1, 2);
    m.link(0, 2);

    let now = 1_700_000_000;
    let forwards = m.nodes[0].publish("news", b"the dam holds".to_vec(), now);
    t.note("Ana", "publish to \"news\"");
    let delivered = m.run(&mut t, "Ana", forwards, now);

    t.fact(format!("{} frames, {} bytes on the wire", m.frames, m.bytes));
    t.fact(format!("{delivered} app deliveries, from one publish"));
    t.fact(
        "**`hops` counts down**, 16 to 15, and a relayed frame is a *different* frame with \
         one fewer hop. It is still the same message, because the id that dedup uses is \
         computed with `hops` zeroed — without that, every hop count would be a new id and \
         the flood would never terminate."
            .into(),
    );
    t.fact(
        "**No frame goes back the way it came.** `Forward::Flood` names the interface it \
         arrived on and the transport skips it. The first version of this generator gave \
         every link the same interface id, could not honour that, and drew an arrow straight \
         back to the sender — an arrow no real transport would emit. The diagram was wrong in \
         precisely the way a hand-drawn one would have been."
            .into(),
    );
    t.fact(
        "The duplicate that *does* arrive is dropped as already seen. On a shared radio most \
         of these never leave the antenna at all: the CSMA layer jitters and cancels when it \
         overhears the same id."
            .into(),
    );
    t
}

/// The same message a second time.
fn dedup() -> Trace {
    let mut t = Trace::new(
        "The same message, arriving twice",
        "Dedup is by envelope id with `hops` zeroed, so two copies that travelled \
         different distances are still one message. Without the zeroing, every hop \
         count would be a different id and a flood would never terminate.",
    );
    let mut m = Mesh::new(&[("Ana", &["news"][..]), ("Ben", &["news"][..])]);
    m.link(0, 1);

    let now = 1_700_000_000;
    let forwards = m.nodes[0].publish("news", b"once".to_vec(), now);
    let wire = match &forwards[0] {
        Forward::Flood { bytes, .. } | Forward::Directed { bytes, .. } => bytes.clone(),
    };

    let rx = m.nodes[1].on_rx(&wire, 0, None, now);
    t.msg("Ana", "Ben", &describe(&wire));
    t.note("Ben", &format!("delivered x{}, forwards x{}", rx.delivered.len(), rx.forwards.len()));

    let rx2 = m.nodes[1].on_rx(&wire, 0, None, now);
    t.msg("Ana", "Ben", &format!("{} (again)", describe(&wire)));
    t.note("Ben", &format!("delivered x{}, forwards x{}", rx2.delivered.len(), rx2.forwards.len()));

    t.fact(format!(
        "First arrival: {} delivered, {} forwarded. Second: {} delivered, {} forwarded.",
        rx.delivered.len(),
        rx.forwards.len(),
        rx2.delivered.len(),
        rx2.forwards.len()
    ));
    t
}

/// Carrying mail for someone who is not here.
fn custody() -> Trace {
    let mut t = Trace::new(
        "Custody: mail for a node that is not on the network",
        "The scenario that makes this a *store-and-forward* protocol rather than a \
         router. Ben cannot read this message and is not its destination, and he \
         carries it anyway.",
    );
    let mut m = Mesh::new(&[("Ana", &[][..]), ("Ben", &[][..]), ("Cai", &[][..])]);
    m.link(0, 1); // Cai is not linked yet — she is not here

    let now = 1_700_000_000;
    let cai = m.addr("Cai");

    // Cai was heard from once, before she went away. That ANNOUNCE is what
    // carries her prekey, and without it `send_direct` would fall back to
    // plaintext — which would make the whole scenario a lie, since the point is
    // that Ben carries something he cannot read. Worth doing properly: the first
    // draft of this page skipped it and then claimed Ben could not read a
    // message that was in the clear.
    let ann = m.nodes[2].build_announce(now - 60);
    for f in &ann {
        let wire = match f {
            Forward::Flood { bytes, .. } | Forward::Directed { bytes, .. } => bytes.clone(),
        };
        m.nodes[0].on_rx(&wire, 0, None, now - 60);
    }
    t.note("Ana", "heard Cai's ANNOUNCE earlier — holds her prekey");

    let (_id, forwards, sealed) = m.nodes[0].send_direct(cai, b"meet at the ridge", now);
    assert!(sealed, "the scenario is about carrying what you cannot read");
    t.note("Ana", "send_direct to Cai, sealed to her prekey");
    m.run(&mut t, "Ana", forwards, now);

    let held = m.nodes[1].store_len();
    t.fact(format!("Ben is holding {held} sealed envelope he cannot open, for a node he has never met"));

    // Later, Cai arrives. Ben offers what he has.
    m.link(1, 2);
    let later = now + 3_600;
    let inv = m.nodes[1].build_inv(&std::collections::HashSet::new());
    t.msg("Ben", "Cai", &describe(&inv));
    let rx = m.nodes[2].on_rx(&inv, 0, None, later);
    for f in &rx.forwards {
        let want = match f {
            Forward::Flood { bytes, .. } | Forward::Directed { bytes, .. } => bytes.clone(),
        };
        t.msg("Cai", "Ben", &describe(&want));
        let served = m.nodes[1].on_rx(&want, 0, None, later);
        for sf in &served.forwards {
            let wire = match sf {
                Forward::Flood { bytes, .. } | Forward::Directed { bytes, .. } => bytes.clone(),
            };
            t.msg("Ben", "Cai", &describe(&wire));
            let got = m.nodes[2].on_rx(&wire, 0, None, later);
            if !got.delivered.is_empty() {
                t.note("Cai", "delivered — an hour after it was sent");
            }
        }
    }

    t.fact(
        "Nothing here is a route. Ana never knew where Cai was, Ben never learned, and the \
         message crossed an hour of Cai being switched off."
            .into(),
    );
    t
}

/// What the kernel refuses.
fn refusals() -> Trace {
    let mut t = Trace::new(
        "What the kernel refuses, and what it carries anyway",
        "The decisions that are easiest to get backwards. Two of these were wrong \
         in my own prose before they were observed.",
    );
    let mut m = Mesh::new(&[("Ana", &["news"][..]), ("Ben", &["news"][..])]);
    m.link(0, 1);
    let now = 1_700_000_000;

    // hops exhausted
    let mut e = Envelope::new(ty::DATA, topic_of("news"), now, b"stops here".to_vec());
    e.flags |= fl::FLOOD;
    e.hops = 0;
    let wire = e.wire();
    let rx = m.nodes[1].on_rx(&wire, 0, None, now);
    t.msg("Ana", "Ben", &describe(&wire));
    t.note("Ben", &format!("hops=0 — delivered x{}, forwarded x{}", rx.delivered.len(), rx.forwards.len()));

    // post-dated
    let mut e = Envelope::new(ty::DATA, topic_of("news"), now + 86_400, b"from the future".to_vec());
    e.flags |= fl::FLOOD;
    e.hops = 4;
    let wire = e.wire();
    let rx = m.nodes[1].on_rx(&wire, 0, None, now);
    t.msg("Ana", "Ben", &format!("{} created_at +1 day", describe(&wire)));
    t.note(
        "Ben",
        &format!("post-dated — delivered x{}, forwarded x{}", rx.delivered.len(), rx.forwards.len()),
    );

    // bad signature
    let sk = ed25519_dalek::SigningKey::from_bytes(&[42u8; 32]);
    let mut e = Envelope::new(ty::DATA, topic_of("news"), now, b"tampered after signing".to_vec());
    e.flags |= fl::FLOOD;
    e.hops = 4;
    e.sign(&sk);
    let mut wire = e.wire();
    let p = wire.len() - 64 - 22;
    wire[p] ^= 0x01;
    let rx = m.nodes[1].on_rx(&wire, 0, None, now);
    t.msg("Ana", "Ben", &format!("{} altered after signing", describe(&wire)));
    t.note(
        "Ben",
        &format!("bad signature — delivered x{}, forwarded x{}", rx.delivered.len(), rx.forwards.len()),
    );

    t.fact(
        "`hops` counts **down** and forwarding stops at zero — the frame is still for us, it \
         just stops here."
            .into(),
    );
    t.fact(
        "A post-dated envelope is refused at admission. Accepting one would let a sender \
         outrank every honest envelope at eviction, which is a cheap way to evict a node's \
         whole store."
            .into(),
    );
    t.fact(
        "**A bad signature does not stop a relay**, and that is the rule rather than a gap \
         (SPEC section 5: verify before binding trust state, do not verify to forward). What \
         it costs the sender is attribution — the envelope binds no path and names no \
         neighbour. An implementation that \"hardened\" by dropping it would also drop every \
         envelope it merely lacks the key for, which is most of them."
            .into(),
    );
    t
}

fn main() {
    let traces = [flood(), dedup(), custody(), refusals()];

    let lines = [
        "<!-- Generated by `cargo run --release --example kernel_flows`. Do not edit by hand. -->",
        "",
        "# Kernel flows",
        "",
        "A message moving, in four scenarios. **Every arrow was recorded from a real",
        "`Node` call** — the frames, the byte counts and the decisions are what the",
        "kernel did when this page was generated, not a description of what it should",
        "do.",
        "",
        "That distinction has already earned itself here. The forwarding table in",
        "`reference/versioned_vectors.json` was prose until it was observed, and the",
        "prose had `hops` counting the wrong way and claimed a failed signature stops a",
        "relay. Both errors survived review and died on first contact with the code.",
        "",
        "These are small and exact — three or four nodes, every frame accounted for.",
        "For behaviour at scale, under loss and with adversaries, see",
        "[Simulations](SIMULATIONS.md); for the crypto and concurrency state machines,",
        "[State machines](STATE_MACHINES.md).",
        "",
    ];
    for l in lines {
        println!("{l}");
    }

    for t in &traces {
        println!("## {}\n", t.title);
        println!("{}\n", t.why);
        print!("{}", t.mermaid());
        println!();
        for f in &t.facts {
            println!("- {f}\n");
        }
    }
}
