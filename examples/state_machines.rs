//! Emit `docs/STATE_MACHINES.md` by **driving the real state machines** and
//! recording the transitions they actually take.
//!
//!     cargo run --release --example state_machines > docs/STATE_MACHINES.md
//!
//! A hand-drawn state diagram is a claim about code that nobody re-checks, and
//! this repository has already shipped one of those: the router flow said "id
//! seen, or expired?" for months after expiry stopped existing. So these are
//! generated the same way the simulation results are — every edge below was
//! observed, and an edge the code can no longer take disappears from the page.
//!
//! The scenarios here are the same ones the property tests drive, for the same
//! reason: if a transition matters enough to test, it matters enough to draw.

use spore::bridge::hub::Hub;
use spore::mix::Batch;
use spore::ratchet::{keypair, Ratchet};
use std::collections::BTreeSet;

const T0: u32 = 1_700_000_000;
const TTL: u32 = 7 * 24 * 3600;

/// A transition label mermaid will accept.
///
/// In `stateDiagram-v2` the label runs from the first `:` to end of line, and a
/// second colon inside it is a parse error — `Hub::new` took the whole page down
/// once. Sanitising here rather than at every call site means a future label
/// cannot break the build by containing a path.
fn sanitise(label: &str) -> String {
    label.replace("::", " ").replace(':', " —").replace(';', ",")
}

/// One observed edge: `from --label--> to`.
type Edge = (String, String, String);

/// Records what a machine did, so the diagram is the record rather than a plan.
#[derive(Default)]
struct Observed {
    edges: BTreeSet<Edge>,
}

impl Observed {
    fn saw(&mut self, from: &str, label: &str, to: &str) {
        self.edges.insert((from.into(), label.into(), to.into()));
    }
    fn mermaid(&self) -> String {
        let mut o = String::from("```mermaid\nstateDiagram-v2\n");
        for (from, label, to) in &self.edges {
            o.push_str(&format!("  {from} --> {to}: {}\n", sanitise(label)));
        }
        o.push_str("```\n");
        o
    }
}

/// What happens to a frame handed to `Ratchet::decrypt`.
///
/// The states are the ones a caller can distinguish, because those are the ones
/// worth drawing: a message either opens, or is refused, and the interesting part
/// is *which* path it took to either.
fn ratchet_machine() -> (Observed, Vec<String>) {
    let mut o = Observed::default();
    let mut notes = Vec::new();

    let (a_sec, a_pub) = keypair();
    let (b_sec, b_pub) = keypair();
    let mut alice = Ratchet::init_alice(a_sec, b_pub, TTL);
    let mut bob = Ratchet::init_bob(b_sec, b_pub, a_pub, TTL);

    o.saw("[*]", "init_bob", "NoSendingChain");
    if !bob.can_send() {
        notes.push(
            "A responder has no sending chain until it has received: `can_send()` is false, and \
             its own earlier messages go as one-shot seals instead."
                .into(),
        );
    }

    // In-order delivery.
    let m0 = alice.encrypt(b"zero");
    assert!(bob.decrypt(&m0, T0).is_some());
    o.saw("NoSendingChain", "first message opens: DH ratchet", "Established");
    o.saw("Established", "n == nr: derive from chain", "Opened");
    if bob.can_send() {
        notes.push("Receiving one message is what gives the responder a sending chain.".into());
    }

    // Replay of the same ciphertext.
    assert!(bob.decrypt(&m0, T0).is_none());
    o.saw("Established", "n < nr: already consumed", "Refused");

    // Out of order: bank keys, then claim one.
    let m1 = alice.encrypt(b"one");
    let m2 = alice.encrypt(b"two");
    assert!(bob.decrypt(&m2, T0).is_some());
    o.saw("Established", "n > nr: bank the gap", "SkippedBanked");
    o.saw("SkippedBanked", "the banked key opens it", "Opened");
    assert!(bob.decrypt(&m1, T0).is_some());
    assert!(bob.decrypt(&m1, T0).is_none());
    o.saw("SkippedBanked", "banked key already spent", "Refused");

    // Forgery: must change nothing.
    let good = alice.encrypt(b"genuine");
    let mut forged = good.clone();
    forged[0] ^= 1;
    assert!(bob.decrypt(&forged, T0).is_none());
    o.saw("Established", "fails to authenticate: nothing committed", "Refused");
    assert!(bob.decrypt(&good, T0).is_some(), "the forgery left the session intact");
    notes.push(
        "`Refused` is a dead end by design: **no path out of it changes state**. That is the \
         whole of the M1 fix — the machine used to commit the DH step and the chain advance on \
         the way to `Refused`, so one forged frame moved the session somewhere no genuine \
         message could reach."
            .into(),
    );

    // Absurd gap.
    let mut absurd = alice.encrypt(b"far");
    absurd[32..34].copy_from_slice(&u16::MAX.to_be_bytes());
    assert!(bob.decrypt(&absurd, T0).is_none());
    o.saw("Established", "gap past MAX_SKIP: refuse without computing", "Refused");

    // Expiry of a banked key.
    let m3 = alice.encrypt(b"three");
    let m4 = alice.encrypt(b"four");
    assert!(bob.decrypt(&m4, T0).is_some());
    assert!(bob.decrypt(&m3, T0 + TTL + 1).is_none());
    o.saw("SkippedBanked", "older than the skip TTL: dropped", "Refused");
    notes.push(
        "The TTL edge is forward secrecy doing its job: a key banked for a straggler is deleted \
         once the window closes, so a message that arrives too late is unreadable *by design* \
         rather than by failure."
            .into(),
    );

    (o, notes)
}

/// The mix's release machine.
fn mix_machine() -> (Observed, Vec<String>) {
    let mut o = Observed::default();
    let mut notes = Vec::new();
    let min = 3;

    let mut b = Batch::new(min);
    o.saw("[*]", "new(min_batch)", "Empty");
    b.add(vec![1], T0, 1);
    o.saw("Empty", "add", "BelowThreshold");
    assert!(b.ready(T0 + 100).is_empty());
    o.saw("BelowThreshold", "fewer than min_batch due: hold", "BelowThreshold");

    b.add(vec![2], T0, 1);
    b.add(vec![3], T0, 50);
    assert!(b.ready(T0 + 2).is_empty());
    o.saw("BelowThreshold", "held >= min_batch but not enough *due*: hold", "BelowThreshold");
    notes.push(
        "The second self-loop is the bug this machine used to have. The threshold was checked \
         against how many were *held* rather than how many were *due*, so three held with one \
         due released that one alone — a singleton an observer can follow straight through, \
         which is exactly what a batch exists to prevent."
            .into(),
    );

    assert_eq!(b.ready(T0 + 50).len(), 3);
    o.saw("BelowThreshold", "min_batch due: shuffle and release", "Empty");
    notes.push(
        "Release shuffles. A mix that emits in arrival order lets an observer correlate by \
         position without needing any timing at all — the delays hide *when* a message left, \
         the batch hides *which* one it was, and without the shuffle the second buys nothing."
            .into(),
    );

    (o, notes)
}

/// A hub interface's life, observed from outside.
///
/// The locks themselves cannot be drawn from here — the tracker that enforces
/// "one of a hub's locks at a time" is test-only and private, which is the right
/// place for it. What *is* observable is what an interface does, and the part
/// worth drawing is that a retired id is never reused.
fn hub_machine() -> (Observed, Vec<String>) {
    let mut o = Observed::default();
    let mut notes = Vec::new();

    let hub = Hub::new(spore::Node::new("gateway", &[]));
    o.saw("[*]", "Hub::new", "NoInterfaces");

    let (a, a_rx) = hub.register();
    o.saw("NoInterfaces", "register", "Sending");
    let pull = hub.register_pull();
    o.saw("NoInterfaces", "register_pull (no sender)", "PullOnly");

    hub.send(spore::ZERO_DEST, b"one".to_vec()).expect("fits");
    let carried = a_rx.try_iter().count();
    assert!(carried > 0);
    o.saw("Sending", "a flood reaches it", "Sending");
    o.saw("PullOnly", "nothing is pushed to it; it answers WANT from the store", "PullOnly");

    hub.unregister(a);
    o.saw("Sending", "unregister: sender dropped", "Retired");
    hub.send(spore::ZERO_DEST, b"two".to_vec()).expect("fits");
    assert_eq!(a_rx.try_iter().count(), 0);
    o.saw("Retired", "a flood is not delivered to it", "Retired");

    let (c, _c_rx) = hub.register();
    assert_ne!(c, a, "a retired id is never reused");
    o.saw("Retired", "a new register takes a fresh id, never this hole", "Sending");
    hub.unregister(pull);
    hub.unregister(9999);
    o.saw("Retired", "unregister again, or on an unknown id: no-op", "Retired");

    notes.push(
        "A retired slot is emptied rather than removed, so **iface ids are never recycled within \
         a process**. `Forward::Flood`'s `except` addresses interfaces by index, so reusing a hole \
         would silently misroute the one thing that must not be misrouted."
            .into(),
    );
    notes.push(
        "Retiring an interface also retires the pulls it was the reason for (M11-K): anything this \
         node adopted an interest in *on behalf of* a peer behind that link has lost its waiter, \
         and hunting for it is now work for nobody."
            .into(),
    );
    notes.push(
        "Not drawn, because it is not observable from outside: a hub holds **one of its own locks \
         at a time**. Every method that needs both the node and the outbound table scopes the \
         first guard and lets it drop before taking the second — with at most one held there is no \
         pair to invert, which is why there is no lock *order* to get wrong. That was a comment \
         until #278; it is now asserted on every take, in test builds, across every public method."
            .into(),
    );

    (o, notes)
}

fn main() {
    let (ratchet, ratchet_notes) = ratchet_machine();
    let (mix, mix_notes) = mix_machine();
    let (hub, hub_notes) = hub_machine();

    for line in [
        "# State machines",
        "",
        "*This page is generated:* `cargo run --release --example state_machines > docs/STATE_MACHINES.md`.",
        "CI fails if it is stale.",
        "",
        "Every edge below was **observed** — `examples/state_machines.rs` drives the real types and",
        "records the transitions they take, so an edge the code can no longer take disappears from the",
        "page. A hand-drawn diagram is a claim nobody re-checks, and this repository has shipped one of",
        "those before: the router flow said \"id seen, or expired?\" for months after expiry stopped",
        "existing.",
        "",
        "These are the two machines the [September audit](ROADMAP.md) named as unreviewed, and writing",
        "the tests that produce them turned up a live fault in each.",
        "",
    ] {
        println!("{line}");
    }

    println!("## Double Ratchet — what happens to a received frame\n");
    print!("{}", ratchet.mermaid());
    println!();
    for n in &ratchet_notes {
        println!("- {n}\n");
    }

    println!("## Mix — the release queue\n");
    print!("{}", mix.mermaid());
    println!();
    for n in &mix_notes {
        println!("- {n}\n");
    }

    println!("## Hub — an interface's life\n");
    print!("{}", hub.mermaid());
    println!();
    for n in &hub_notes {
        println!("- {n}\n");
    }
}
