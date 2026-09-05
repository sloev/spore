//! **The resource invariant**, and one test per path that consumes a resource.
//!
//! > No remote node can cause another to transmit, store, or process an
//! > unbounded amount without continuing evidence of demand, or an explicit
//! > bounded local allowance.
//!
//! Every mechanism this file exercises already existed. What did not exist was
//! the sentence: the bounds read as a pile of incident-driven patches — S-012's
//! 32× WANT amplifier, S-013's half-finished fragment sets, audit #189's
//! unbounded partial bytes — rather than as one idea applied everywhere. A
//! defence you cannot name is one the next feature forgets to apply.
//!
//! So this module is deliberately boring and deliberately complete. One test per
//! path a *stranger* can push on, each phrased as the attacker's goal rather than
//! the mechanism's name, because the question is never "does `trim_map` work" but
//! "can someone I have never met make this node grow without limit".
//!
//! Two paths are covered elsewhere and are not repeated here: WANT amplification
//! (`a_want_cannot_be_used_as_an_amplifier`) and per-interface fragment
//! reassembly (`a_loud_interface_cannot_evict_a_quiet_one_from_the_partial_budget`).
//!
//! Test-only, and in `src/` rather than `tests/` for the same reason
//! `robustness.rs` is: the freeze guard treats all of `tests/` as frozen contract.

use crate::*;

const NOW: u32 = 1_700_000_000;

/// Ceilings small enough to cross quickly, so a test proves the *rule* rather
/// than spending a minute proving arithmetic.
fn tight() -> Limits {
    Limits {
        seen: 32,
        partial_objects: 4,
        partial_bytes: 16 * 1024,
        peers: 8,
        manifests: 4,
        acked: 8,
        inbox: 8,
    }
}

/// A public envelope with a distinct id, as any stranger could mint.
fn junk(seq: u32) -> Vec<u8> {
    let mut e = Envelope::new(ty::DATA, ZERO_DEST, NOW + 3600, seq.to_be_bytes().to_vec());
    e.flags |= fl::FLOOD;
    e.wire()
}

#[test]
fn a_stranger_cannot_grow_the_dedup_table_without_limit() {
    // Dedup is the one table every single received envelope touches, so it is
    // the cheapest thing in the node to attack: no key, no session, no quota
    // relationship — just distinct bytes, as fast as the link allows.
    let mut n = Node::new("victim", &[]);
    n.set_limits(tight());
    for i in 0..500 {
        n.on_rx(&junk(i), 0, None, NOW);
    }
    assert!(n.seen.len() <= n.limits.seen, "dedup grew to {}", n.seen.len());
}

#[test]
fn a_stranger_cannot_grow_the_store_past_its_byte_budget() {
    // The store is the one resource measured in bytes rather than entries,
    // because that is what actually runs out. Custody is a *local allowance*
    // under the invariant: the node chose the budget, and a peer may fill it but
    // never exceed it.
    let mut n = Node::new("victim", &[]);
    n.set_store_budget(64 * 1024);
    for i in 0..2000 {
        n.on_rx(&junk(i), 0, None, NOW);
    }
    assert!(n.store.bytes() <= 64 * 1024, "store holds {} bytes against a 64 KiB budget", n.store.bytes());
}

#[test]
fn a_crowd_of_strangers_cannot_grow_the_peer_tables_without_limit() {
    // Every ANNOUNCE teaches this node a prekey, a busy byte, a claimed name and
    // possibly a ratchet session — four maps keyed by address. Addresses are
    // free to mint, so without a ceiling "announce from a fresh keypair" is a
    // memory-exhaustion primitive that costs the attacker one signature.
    let mut n = Node::new("victim", &[]);
    n.set_limits(tight());
    for i in 0..80 {
        let mut peer = Node::from_seed("stranger", &[], &[(i % 251) as u8; 32]);
        let ann = peer.build_announce(NOW);
        for f in &ann {
            let wire = match f {
                Forward::Flood { bytes, .. } => bytes,
                Forward::Directed { bytes, .. } => bytes,
            };
            n.on_rx(wire, 0, None, NOW);
        }
    }
    let cap = n.limits.peers;
    assert!(n.peer_prekeys.len() <= cap, "prekeys {}", n.peer_prekeys.len());
    assert!(n.peer_busy.len() <= cap, "busy {}", n.peer_busy.len());
    assert!(n.peer_names.len() <= cap, "names {}", n.peer_names.len());
    assert!(n.sessions.len() <= cap, "sessions {}", n.sessions.len());
}

#[test]
fn a_stranger_cannot_grow_the_manifest_table_without_limit() {
    // A manifest is the index for a file, and holding one is what entitles a
    // node to fetch its chunks. That makes the table *useful* to flood: every
    // manifest is an open invitation to spend bandwidth later.
    let mut n = Node::new("victim", &[]);
    n.set_limits(tight());
    for i in 0..40u32 {
        let mut publisher = Node::from_seed("publisher", &[], &[(i % 251) as u8; 32]);
        let (_, fwds) = publisher.publish_file("f.bin", &i.to_be_bytes(), ZERO_DEST, NOW);
        for f in &fwds {
            let wire = match f {
                Forward::Flood { bytes, .. } => bytes,
                Forward::Directed { bytes, .. } => bytes,
            };
            n.on_rx(wire, 0, None, NOW);
        }
    }
    assert!(n.manifests.len() <= n.limits.manifests, "manifests {}", n.manifests.len());
}

#[test]
fn an_undrained_feed_inbox_is_bounded_by_the_node_not_the_publisher() {
    // The inbox is the one queue whose growth is the *application's* fault as
    // much as the sender's: an app that never polls is not a reason for the node
    // to hold everything a topic ever carried. Oldest goes first, because a feed
    // event nobody drained for a thousand posts is stale.
    let mut n = Node::new("victim", &["weather"]);
    n.set_limits(tight());
    for i in 0..200u32 {
        let mut poster = Node::from_seed("poster", &[], &[7u8; 32]);
        let fwds = poster.publish("weather", i.to_be_bytes().to_vec(), NOW);
        for f in &fwds {
            let wire = match f {
                Forward::Flood { bytes, .. } => bytes,
                Forward::Directed { bytes, .. } => bytes,
            };
            n.on_rx(wire, 0, None, NOW);
        }
    }
    assert!(n.feed_inbox.len() <= n.limits.inbox, "feed inbox {}", n.feed_inbox.len());
}

#[test]
fn the_receipt_set_is_bounded_even_if_receipts_keep_arriving() {
    // Receipts are ids we remember so a resend can stop. Remembering is the
    // point, so the bound is what keeps "remember it" from meaning "forever".
    let mut n = Node::new("victim", &[]);
    n.set_limits(tight());
    for i in 0..200u32 {
        n.acked.insert([(i % 251) as u8; 16]);
        n.enforce_bounds(NOW);
    }
    assert!(n.acked.len() <= n.limits.acked, "acked {}", n.acked.len());
}

#[test]
fn every_ceiling_is_reachable_from_one_call_so_a_small_runtime_can_set_them_all() {
    // The invariant's second clause is "an explicit bounded local allowance".
    // That is only true if a runtime can *state* its allowance in one place — an
    // MCU that has to remember seven setters will get six of them right.
    let mut n = Node::new("mcu", &[]);
    n.set_limits(Limits::for_budget(64 * 1024));
    let lim = n.limits();
    assert!(lim.seen > 0 && lim.peers > 0 && lim.manifests > 0 && lim.inbox > 0);
    assert!(lim.partial_objects > 0 && lim.partial_bytes >= 16 * 1024);
    // And it must not silently grow a node past the desktop defaults.
    assert!(lim.partial_objects <= MAX_PARTIAL_OBJECTS);
}
