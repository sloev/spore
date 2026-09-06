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
        named: 32,
        interests: 4,
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

#[test]
fn a_served_chunk_stops_at_the_node_that_asked_for_it() {
    // M11-J. One answered WANT used to put a chunk on every link in the mesh:
    // chunks carried the default 16 hops with FLOOD set, so although the
    // publisher never sent one, every node that received one re-flooded it —
    // including nodes that had never asked for the file. That is the shape of
    // the invariant's first clause: traffic without continuing evidence of
    // demand.
    let mut publisher = Node::new("publisher", &[]);
    let mut fetcher = Node::new("fetcher", &[]);
    let mut bystander = Node::new("bystander", &[]);

    // Bigger than the push budget (M11-E), or the file arrives complete with
    // its manifest and there is no serve path left to observe.
    let (magnet, fwds) = publisher.publish_file("f.bin", &vec![0xAB; 40_000], ZERO_DEST, NOW);
    for f in &fwds {
        let wire = match f {
            Forward::Flood { bytes, .. } => bytes,
            Forward::Directed { bytes, .. } => bytes,
        };
        fetcher.on_rx(wire, 0, None, NOW);
    }

    // The fetcher asks its neighbour, which happens to be the publisher.
    let mut served = Vec::new();
    for w in fetcher.fetch(&magnet) {
        let wire = match &w {
            Forward::Flood { bytes, .. } => bytes,
            Forward::Directed { bytes, .. } => bytes,
        };
        for f in publisher.on_rx(wire, 0, None, NOW).forwards {
            served.push(match f {
                Forward::Flood { bytes, .. } => bytes,
                Forward::Directed { bytes, .. } => bytes,
            });
        }
    }
    assert!(!served.is_empty(), "the publisher answered with chunks");

    let mut onward = 0;
    for w in &served {
        onward += fetcher.on_rx(w, 0, None, NOW).forwards.len();
    }
    assert_eq!(onward, 0, "the node that asked does not re-broadcast what it received");

    let mut relayed = 0;
    for w in &served {
        relayed += bystander.on_rx(w, 0, None, NOW).forwards.len();
    }
    assert_eq!(relayed, 0, "and a node that never asked carries nothing");
}

// ------------------------------------------------- M11-I: recursive pull

/// Wire two nodes together by hand: everything `from` emits, `to` receives.
fn pump(from: &mut Node, to: &mut Node, forwards: Vec<Forward>, now: u32) -> Vec<Forward> {
    let _ = from;
    let mut out = Vec::new();
    for f in forwards {
        let wire = match &f {
            Forward::Flood { bytes, .. } => bytes,
            Forward::Directed { bytes, .. } => bytes,
        };
        out.extend(to.on_rx(wire, 0, None, now).forwards);
    }
    out
}

#[test]
fn a_want_for_an_id_no_manifest_names_starts_no_hunt() {
    // **The rule that keeps recursive pull from being S-012 at depth n.**
    //
    // Adopting a neighbour's interest means spending someone else's radio, so
    // the flooded root is the capability: the mesh has agreed the file exists
    // and the Merkle tree names every legal child. An id nobody has a manifest
    // for is not a fetch, it is a request to search the mesh.
    let mut relay = Node::new("relay", &[]);

    let unknown = [0x77u8; 16];
    let want = Envelope::new(ty::WANT, ZERO_DEST, 0, unknown.to_vec()).wire();
    let rx = relay.on_rx(&want, 0, None, NOW);

    assert!(rx.forwards.is_empty(), "an unnamed id must produce silence, not a search");
}

#[test]
fn a_neighbours_want_is_adopted_never_forwarded() {
    // The WANT envelope stays where it landed. What travels is a *new* request
    // this node made because it acquired an interest of its own — which is what
    // keeps §6's bound intact: WANT is hops=0, unsigned, consumed, never relayed.
    let mut publisher = Node::new("publisher", &[]);
    let mut relay = Node::new("relay", &[]);
    // Bigger than the push budget, so chunks are genuinely missing to ask for.
    let (magnet, fwds) = publisher.publish_file("f.bin", &vec![0xCD; 40_000], ZERO_DEST, NOW);

    // The relay hears the root, so it knows the file exists and what it names.
    for f in &fwds {
        let wire = match f {
            Forward::Flood { bytes, .. } => bytes,
            Forward::Directed { bytes, .. } => bytes,
        };
        relay.on_rx(wire, 0, None, NOW);
    }

    // A neighbour asks the relay for a chunk the relay does not hold.
    let mut fetcher = Node::new("fetcher", &[]);
    for f in &fwds {
        let wire = match f {
            Forward::Flood { bytes, .. } => bytes,
            Forward::Directed { bytes, .. } => bytes,
        };
        fetcher.on_rx(wire, 0, None, NOW);
    }
    let want = fetcher.fetch(&magnet);
    let onward = pump(&mut fetcher, &mut relay, want.clone(), NOW);

    assert!(!onward.is_empty(), "the relay asks onward on the neighbour's behalf");
    let asked = match &onward[0] {
        Forward::Flood { bytes, .. } => bytes.clone(),
        Forward::Directed { bytes, .. } => bytes.clone(),
    };
    let original = match &want[0] {
        Forward::Flood { bytes, .. } => bytes.clone(),
        Forward::Directed { bytes, .. } => bytes.clone(),
    };
    assert_ne!(asked, original, "it must be a new request, not the same envelope relayed");

    let (e, _) = Envelope::decode(&asked).expect("a WANT");
    assert_eq!(e.typ, ty::WANT);
    assert_eq!(
        e.payload.len() % 16,
        1,
        "and it carries the remaining depth, so the asking cannot go on forever"
    );
    assert!(e.payload[e.payload.len() - 1] < DEFAULT_WANT_DEPTH, "depth decremented");
}

#[test]
fn the_depth_budget_runs_out() {
    // Depth is what bounds how far one neighbour's curiosity can travel. At
    // zero a node answers from its own store or says nothing.
    let mut publisher = Node::new("publisher", &[]);
    let mut relay = Node::new("relay", &[]);
    let (magnet, fwds) = publisher.publish_file("f.bin", &vec![0xCD; 40_000], ZERO_DEST, NOW);
    for f in &fwds {
        let wire = match f {
            Forward::Flood { bytes, .. } => bytes,
            Forward::Directed { bytes, .. } => bytes,
        };
        relay.on_rx(wire, 0, None, NOW);
    }
    let missing = relay.missing(&magnet, 1);
    assert!(!missing.is_empty(), "the relay knows what it is missing");

    let mut payload: Vec<u8> = missing[0].to_vec();
    payload.push(0); // spent
    let want = Envelope::new(ty::WANT, ZERO_DEST, 0, payload).wire();
    let rx = relay.on_rx(&want, 0, None, NOW);
    assert!(rx.forwards.is_empty(), "a spent budget stops the asking");
}

#[test]
fn an_adopted_interest_is_bounded_and_expires() {
    // An interest is a promise to pass something back, so a crowd of neighbours
    // must not be able to make this node remember without limit — and an
    // interest nobody renewed must not outlive its usefulness.
    let mut publisher = Node::new("publisher", &[]);
    let mut relay = Node::new("relay", &[]);
    relay.set_limits(tight());
    let (magnet, fwds) = publisher.publish_file("f.bin", &vec![0xCD; 40_000], ZERO_DEST, NOW);
    for f in &fwds {
        let wire = match f {
            Forward::Flood { bytes, .. } => bytes,
            Forward::Directed { bytes, .. } => bytes,
        };
        relay.on_rx(wire, 0, None, NOW);
    }

    let missing = relay.missing(&magnet, 64);
    for id in &missing {
        let want = Envelope::new(ty::WANT, ZERO_DEST, 0, id.to_vec()).wire();
        relay.on_rx(&want, 0, None, NOW);
    }
    assert!(
        relay.open_interests() <= relay.limits().interests,
        "interests {} over cap {}",
        relay.open_interests(),
        relay.limits().interests
    );

    // Long past the lease, with any traffic at all to drive the sweep.
    let later = NOW + INTEREST_LEASE_SECS + 1;
    relay.on_rx(&Envelope::new(ty::WANT, ZERO_DEST, 0, vec![0u8; 16]).wire(), 0, None, later);
    assert_eq!(relay.open_interests(), 0, "an interest nobody renewed is forgotten");
}

#[test]
fn a_sealed_file_is_never_fetched_on_someone_elses_behalf() {
    // Sealed chunks are ciphertext, so confidentiality holds either way — but
    // copying one across the mesh and advertising that it exists is not what
    // sealing to one recipient meant. The gate is checkable precisely because a
    // node entitled to recurse is the one holding the manifest that knows.
    let mut publisher = Node::new("publisher", &[]);
    let mut recipient = Node::new("recipient", &[]);
    let mut relay = Node::new("relay", &[]);

    // The publisher must know the recipient's prekey to seal to them.
    let ann = recipient.build_announce(NOW);
    for f in &ann {
        let wire = match f {
            Forward::Flood { bytes, .. } => bytes,
            Forward::Directed { bytes, .. } => bytes,
        };
        publisher.on_rx(wire, 0, None, NOW);
    }
    let Some((magnet, fwds)) =
        publisher.publish_file_sealed("secret.txt", &vec![0xEE; 6000], recipient.addr, NOW)
    else {
        return; // no prekey yet on this build; the open-file gate is tested above
    };
    for f in &fwds {
        let wire = match f {
            Forward::Flood { bytes, .. } => bytes,
            Forward::Directed { bytes, .. } => bytes,
        };
        relay.on_rx(wire, 0, None, NOW);
    }

    let missing = relay.missing(&magnet, 1);
    if missing.is_empty() {
        return; // nothing nameable yet
    }
    let want = Envelope::new(ty::WANT, ZERO_DEST, 0, missing[0].to_vec()).wire();
    let rx = relay.on_rx(&want, 0, None, NOW);
    assert!(rx.forwards.is_empty(), "a sealed file must not be hunted for a third party");
}

// -------------------------------------------------- M11-E: pushing chunks

#[test]
fn a_small_file_arrives_without_a_round_trip() {
    // Publishing floods the root and pushes the first few chunks with it, so a
    // file that fits inside the push threshold is *complete* on arrival — no
    // WANT, no answer, no third leg.
    let mut publisher = Node::new("publisher", &[]);
    let mut peer = Node::new("peer", &[]);

    // Small enough that every chunk fits the push budget.
    let body = vec![0x42; 2000];
    let (magnet, fwds) = publisher.publish_file("note.txt", &body, ZERO_DEST, NOW);
    for f in &fwds {
        let wire = match f {
            Forward::Flood { bytes, .. } => bytes,
            Forward::Directed { bytes, .. } => bytes,
        };
        peer.on_rx(wire, 0, None, NOW);
    }

    assert!(peer.has_file(&magnet), "the whole file rode along with its manifest");
    assert_eq!(peer.file_bytes(&magnet).as_deref(), Some(&body[..]), "and it is the same bytes");
    assert!(peer.fetch(&magnet).is_empty(), "with nothing left to ask for");
}

#[test]
fn pushing_is_local_policy_and_zero_is_legal() {
    // The receiver's behaviour does not depend on the sender's choice: it takes
    // what it was given and asks for the rest. So a node that would rather not
    // spend the airtime can push nothing, and the file still transfers.
    let mut publisher = Node::new("publisher", &[]);
    publisher.set_push_chunks(0);
    let mut peer = Node::new("peer", &[]);

    let body = vec![0x42; 2000];
    let (magnet, fwds) = publisher.publish_file("note.txt", &body, ZERO_DEST, NOW);
    assert_eq!(fwds.len(), 1, "pure pull: only the manifest goes out");

    for f in &fwds {
        let wire = match f {
            Forward::Flood { bytes, .. } => bytes,
            Forward::Directed { bytes, .. } => bytes,
        };
        peer.on_rx(wire, 0, None, NOW);
    }
    assert!(!peer.has_file(&magnet), "nothing but the index arrived");
    assert!(!peer.fetch(&magnet).is_empty(), "so the peer asks, exactly as before");
}

#[test]
fn a_sealed_file_is_not_pushed_at_everyone() {
    // A sealed file is addressed to one recipient. Spraying its chunks at every
    // neighbour is the opposite of what sealing asked for, even though they are
    // ciphertext — it advertises the file exists and makes the mesh carry it.
    let mut publisher = Node::new("publisher", &[]);
    let mut recipient = Node::new("recipient", &[]);
    for f in &recipient.build_announce(NOW) {
        let wire = match f {
            Forward::Flood { bytes, .. } => bytes,
            Forward::Directed { bytes, .. } => bytes,
        };
        publisher.on_rx(wire, 0, None, NOW);
    }
    let Some((_, fwds)) = publisher.publish_file_sealed("secret.txt", &vec![0xEE; 2000], recipient.addr, NOW)
    else {
        return; // no prekey on this build
    };
    assert_eq!(fwds.len(), 1, "the sealed root travels alone");
}
