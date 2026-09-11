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

/// A file big enough, and varied enough, to still have parts left to ask for
/// after the publisher's push budget.
///
/// Both halves matter since M11-M. Chunks average `CHUNK_AVG_BYTES` rather than
/// an MTU-sized 1336, so a 40 kB file is ten chunks and `DEFAULT_PUSH_CHUNKS`
/// nearly covers it; and chunks are content-addressed, so 40 kB of one repeated
/// byte is a *single* distinct object however long it is. A test that wants a
/// fetch to actually happen needs both size and variety.
fn pullable_file() -> Vec<u8> {
    (0..50_000u32).flat_map(|i| i.to_be_bytes()).collect()
}

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
    let (magnet, fwds) = publisher.publish_file("f.bin", &pullable_file(), ZERO_DEST, NOW);
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
    let (magnet, fwds) = publisher.publish_file("f.bin", &pullable_file(), ZERO_DEST, NOW);

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
    let (magnet, fwds) = publisher.publish_file("f.bin", &pullable_file(), ZERO_DEST, NOW);
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
    let (magnet, fwds) = publisher.publish_file("f.bin", &pullable_file(), ZERO_DEST, NOW);
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

    // Past the *old* fixed lease it is still held, because M11-P scopes an
    // interest to the object rather than to a timer: these chunks have not
    // expired, so the hunt for them still makes sense.
    let old_lease = NOW + INTEREST_LEASE_SECS + 1;
    relay.on_rx(&Envelope::new(ty::WANT, ZERO_DEST, 0, vec![0u8; 16]).wire(), 0, None, old_lease);
    assert!(relay.open_interests() > 0, "an interest now outlives the meeting that created it");

    // Past the object, it is gone. Bounded is bounded — what changed is *what*
    // bounds it, not whether anything does.
    let later = NOW + MAX_INTEREST_LEASE_SECS + 1;
    relay.on_rx(&Envelope::new(ty::WANT, ZERO_DEST, 0, vec![0u8; 16]).wire(), 0, None, later);
    assert_eq!(relay.open_interests(), 0, "an interest nobody renewed is forgotten");
}

/// Set a relay hunting for a chunk on a neighbour's behalf, and hand back the
/// magnet plus the ids it is now looking for.
fn relay_with_an_adopted_interest(relay: &mut Node, iface: Iface) -> Id {
    let mut publisher = Node::new("publisher", &[]);
    let (magnet, fwds) = publisher.publish_file("f.bin", &pullable_file(), ZERO_DEST, NOW);
    for f in &fwds {
        let wire = match f {
            Forward::Flood { bytes, .. } => bytes,
            Forward::Directed { bytes, .. } => bytes,
        };
        relay.on_rx(wire, iface, None, NOW);
    }
    let missing = relay.missing(&magnet, 4);
    assert!(!missing.is_empty());
    for id in &missing {
        relay.on_rx(&Envelope::new(ty::WANT, ZERO_DEST, 0, id.to_vec()).wire(), iface, None, NOW);
    }
    assert!(relay.open_interests() > 0, "the relay adopted the interest");
    magnet
}

#[test]
fn a_cancel_retires_an_adopted_interest_at_once() {
    // The lease is fifteen minutes. A fetcher that can say it has stopped should
    // not cost the mesh fifteen minutes of hunting.
    let mut relay = Node::new("relay", &[]);
    let magnet = relay_with_an_adopted_interest(&mut relay, 0);

    let ids = relay.missing(&magnet, 4);
    let mut payload: Vec<u8> = ids.iter().flatten().copied().collect();
    let mut e = Envelope::new(ty::WANT, ZERO_DEST, 0, std::mem::take(&mut payload));
    e.flags |= fl::CANCEL;
    let rx = relay.on_rx(&e.wire(), 0, None, NOW);

    assert_eq!(relay.open_interests(), 0, "the cancel retired it now, not at the lease");
    // And the cancel unwinds: the relay tells whoever it had asked.
    assert!(!rx.forwards.is_empty(), "the last waiter left, so the chain unwinds");
    let (out, _) = match &rx.forwards[0] {
        Forward::Flood { bytes, .. } => Envelope::decode(bytes).expect("a frame"),
        Forward::Directed { bytes, .. } => Envelope::decode(bytes).expect("a frame"),
    };
    assert_eq!(out.typ, ty::WANT);
    assert!(out.flags & fl::CANCEL != 0, "and it is itself a cancel");
}

#[test]
fn a_cancel_only_removes_the_senders_own_waiter() {
    // Otherwise a cancel is a denial-of-service primitive: one hostile neighbour
    // silences a fetch that somebody else is waiting on.
    let mut relay = Node::new("relay", &[]);
    let magnet = relay_with_an_adopted_interest(&mut relay, 0);
    let before = relay.open_interests();

    // A second neighbour, on a different link, asks for the same ids.
    let ids = relay.missing(&magnet, 4);
    for id in &ids {
        relay.on_rx(&Envelope::new(ty::WANT, ZERO_DEST, 0, id.to_vec()).wire(), 1, None, NOW);
    }

    // Now the second neighbour cancels. The first is still waiting.
    let payload: Vec<u8> = ids.iter().flatten().copied().collect();
    let mut e = Envelope::new(ty::WANT, ZERO_DEST, 0, payload);
    e.flags |= fl::CANCEL;
    let rx = relay.on_rx(&e.wire(), 1, None, NOW);

    assert_eq!(relay.open_interests(), before, "the other waiter's fetch survives");
    assert!(rx.forwards.is_empty(), "and nothing unwinds while someone still waits");
}

#[test]
fn a_cancel_for_an_id_nobody_asked_for_does_nothing() {
    // A cancel must not be a way to make a node emit traffic. n ids in, at most
    // n out, and only where an interest actually died.
    let mut relay = Node::new("relay", &[]);
    let mut e = Envelope::new(ty::WANT, ZERO_DEST, 0, vec![0xAB; 64]);
    e.flags |= fl::CANCEL;
    let rx = relay.on_rx(&e.wire(), 0, None, NOW);
    assert!(rx.forwards.is_empty(), "nothing was retired, so nothing is said");
    assert_eq!(relay.open_interests(), 0);
}

#[test]
fn a_cancel_cannot_be_replayed_into_an_echo() {
    // The second cancel finds no interest, so the unwind happens once.
    let mut relay = Node::new("relay", &[]);
    let magnet = relay_with_an_adopted_interest(&mut relay, 0);
    let ids = relay.missing(&magnet, 4);
    let payload: Vec<u8> = ids.iter().flatten().copied().collect();
    let mut e = Envelope::new(ty::WANT, ZERO_DEST, 0, payload);
    e.flags |= fl::CANCEL;
    let wire = e.wire();

    let first = relay.on_rx(&wire, 0, None, NOW);
    let second = relay.on_rx(&wire, 0, None, NOW);
    assert!(!first.forwards.is_empty());
    assert!(second.forwards.is_empty(), "a replayed cancel is silence");
}

#[test]
fn a_dropped_link_retires_the_interests_it_was_waiting_on() {
    // The half that covers a fetcher which cannot say goodbye — out of range, or
    // crashed. A stateful bridge knows the link went; this is what it does about it.
    let mut relay = Node::new("relay", &[]);
    relay_with_an_adopted_interest(&mut relay, 3);
    assert!(relay.open_interests() > 0);

    let unwind = relay.forget_interests_on(3);
    assert_eq!(relay.open_interests(), 0, "its only waiter is gone");
    assert!(!unwind.is_empty(), "and the chain unwinds behind it");

    // A link that nobody was waiting on costs nothing.
    let mut quiet = Node::new("quiet", &[]);
    assert!(quiet.forget_interests_on(3).is_empty());
}

#[test]
fn abandoning_a_fetch_tells_the_neighbours() {
    // The fetcher's own side of the cancel.
    let mut publisher = Node::new("publisher", &[]);
    let mut fetcher = Node::new("fetcher", &[]);
    let (magnet, fwds) = publisher.publish_file("f.bin", &pullable_file(), ZERO_DEST, NOW);
    for f in &fwds {
        let wire = match f {
            Forward::Flood { bytes, .. } => bytes,
            Forward::Directed { bytes, .. } => bytes,
        };
        fetcher.on_rx(wire, 0, None, NOW);
    }
    assert!(!fetcher.fetch(&magnet).is_empty(), "there is something to ask for");

    let frames = fetcher.abandon(&magnet);
    assert!(!frames.is_empty(), "abandoning a fetch in progress says so");
    for f in &frames {
        let bytes = match f {
            Forward::Flood { bytes, .. } => bytes,
            Forward::Directed { bytes, .. } => bytes,
        };
        let (e, _) = Envelope::decode(bytes).expect("a frame");
        assert_eq!(e.typ, ty::WANT);
        assert!(e.flags & fl::CANCEL != 0);
        assert_eq!(e.payload.len() % 16, 0, "a cancel carries no depth byte");
    }
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
    let Some((magnet, fwds)) =
        publisher.publish_file_sealed("secret.txt", &vec![0xEE; 2000], recipient.addr, NOW)
    else {
        return; // no prekey on this build
    };
    // The root and its header, and nothing else. The header is one small object
    // the recipient cannot use the file without, so it rides along (M11-F); the
    // *chunks* do not, because a sealed file is addressed to one person and
    // spraying ciphertext at every neighbour still advertises that it exists.
    assert_eq!(fwds.len(), 2, "the root and its header, and no chunks");
    let root = publisher.manifests.get(&magnet).expect("we just published it");
    assert!(root.sealed(), "and the root names the header rather than carrying it");
}

// ------------------------------------------- M11-F: the sealed-file floor

#[test]
fn a_sealed_file_can_be_published_on_a_lora_link() {
    // **The measurement M11-F exists for.** The sealed header — ephemeral key,
    // AEAD tag, file key, real name, ~82 bytes — used to sit *inside* the signed
    // root, on top of the root's own 114 bytes of source key and signature. That
    // put a sealed root past 256 bytes before it could name a single chunk, so
    // `publish_file_sealed` was impossible on every LoRa profile: it missed raw
    // LoRa's ~255-byte frame by one byte and Meshtastic's 237 by nineteen.
    //
    // Naming the header instead of carrying it costs 16 bytes and drops the
    // floor to 188.
    let floor = (16..600).find(|m| file::root_fanout(*m, 6, 0, true) >= 1);
    assert_eq!(floor, Some(188), "the sealed root must fit a small link");

    for (name, mtu) in [("LoRaWAN DR5", 222usize), ("Meshtastic", 237), ("raw LoRa P2P", 255)] {
        assert!(
            file::root_fanout(mtu, 6, 0, true) >= 1,
            "{name} at mtu {mtu} must be able to carry a sealed root"
        );
    }
}

#[test]
fn a_sealed_file_reaches_its_recipient_over_the_wire() {
    // End to end, without copying a store: the recipient gets what was actually
    // broadcast, asks for the rest, and opens the file.
    let mut publisher = Node::new("publisher", &[]);
    let mut recipient = Node::new("recipient", &[]);
    for f in &recipient.build_announce(NOW) {
        let wire = match f {
            Forward::Flood { bytes, .. } => bytes,
            Forward::Directed { bytes, .. } => bytes,
        };
        publisher.on_rx(wire, 0, None, NOW);
    }

    let body: Vec<u8> = (0..30_000u32).map(|i| (i.wrapping_mul(7)) as u8).collect();
    let Some((magnet, fwds)) = publisher.publish_file_sealed("plans.pdf", &body, recipient.addr, NOW) else {
        return; // no prekey on this build
    };
    for f in &fwds {
        let wire = match f {
            Forward::Flood { bytes, .. } => bytes,
            Forward::Directed { bytes, .. } => bytes,
        };
        recipient.on_rx(wire, 0, None, NOW);
    }

    // Pull the rest. A tree resolves top-down, so this takes several rounds.
    for _ in 0..24 {
        let want = recipient.fetch_n(&magnet, 4);
        if want.is_empty() {
            break;
        }
        for w in &want {
            let wire = match w {
                Forward::Flood { bytes, .. } => bytes,
                Forward::Directed { bytes, .. } => bytes,
            };
            for cf in publisher.on_rx(wire, 0, None, NOW).forwards {
                let cw = match &cf {
                    Forward::Flood { bytes, .. } => bytes,
                    Forward::Directed { bytes, .. } => bytes,
                };
                recipient.on_rx(cw, 0, None, NOW);
            }
        }
    }

    let mut out = Vec::new();
    let (name, n) = recipient.open_file_to(&magnet, &mut out).expect("the recipient can open it");
    assert_eq!(name, "plans.pdf", "the real name comes out of the header object");
    assert_eq!(n, body.len() as u64);
    assert_eq!(out, body);
}

/// A meeting: hand every frame `a` wants to send to `b`, and every frame that
/// provokes back to `a`, until the exchange goes quiet. Two nodes in a room.
fn meet(a: &mut Node, b: &mut Node, opening: Vec<Forward>, now: u32) {
    let mut in_flight = opening;
    for _ in 0..64 {
        if in_flight.is_empty() {
            return;
        }
        let back = pump(a, b, in_flight, now);
        if back.is_empty() {
            return;
        }
        in_flight = pump(b, a, back, now);
    }
}

#[test]
fn a_want_crosses_an_ocean_on_a_courier_who_never_met_the_publisher() {
    // The sneakernet pull. A WANT itself cannot travel — it is hops=0, unsigned
    // and consumed on receipt, so there is nothing to put on a USB key. What
    // travels is the **manifest**: an ordinary stored envelope, and the thing
    // that makes a file's chunks legal to ask for. Carrying the index *is*
    // carrying the demand.
    let mut publisher = Node::new("publisher", &[]);
    let (magnet, published) = publisher.publish_file("atlas.bin", &pullable_file(), ZERO_DEST, NOW);

    // The root floods, so it reaches people the chunks never will. Alice hears
    // only that — she knows the file exists and cannot get a byte of it.
    let root = match &published[0] {
        Forward::Flood { bytes, .. } | Forward::Directed { bytes, .. } => bytes.clone(),
    };
    let mut alice = Node::new("alice", &[]);
    let mut courier = Node::new("courier", &[]);
    alice.on_rx(&root, 0, None, NOW);
    courier.on_rx(&root, 0, None, NOW);

    assert!(alice.file_name(&magnet).is_some(), "alice knows the file exists");
    assert!(!alice.has_file(&magnet), "and holds none of it");
    assert!(!alice.missing(&magnet, 8).is_empty(), "she has something to want");

    // Alice asks the room. Nobody here has it, so the want dies at the edge of
    // the local mesh — this is the case sneakernet exists for.
    let asked = alice.fetch(&magnet);
    assert!(!asked.is_empty(), "she asks");

    // --- the courier flies. No link, no session, no shared time base. ---------
    // A week later, in another country, and long past INTEREST_LEASE_SECS.
    // Two days out. The journey has a *deadline*: a file's chunks carry the
    // publisher's expiry (`DEFAULT_MESSAGE_EXPIRY_SECS`, 7 days), so the whole
    // round trip has to finish inside it. Sneakernet range is measured in time,
    // not distance, and this is the constant that sets it.
    let abroad = NOW + 2 * 86_400;

    // Bob has the bytes. He never met alice and never heard her ask.
    let mut bob = Node::new("bob", &[]);
    for f in &published {
        let wire = match f {
            Forward::Flood { bytes, .. } | Forward::Directed { bytes, .. } => bytes,
        };
        bob.on_rx(wire, 0, None, abroad);
    }
    for round in 0..24 {
        // The clock advances: a meeting takes time, and the per-link gossip
        // budget refills with it. Holding `now` still would model a link that
        // spends its whole allowance in one instant and never gets it back.
        let t = abroad + round * 60;
        let want = bob.fetch_n(&magnet, 8);
        if want.is_empty() {
            break;
        }
        meet(&mut bob, &mut publisher, want, t);
    }
    assert!(bob.has_file(&magnet), "bob holds the whole file");

    // The courier announces what it wants, because it still holds the manifest.
    for round in 0..24 {
        let t = abroad + round * 60;
        let want = courier.fetch_n(&magnet, 8);
        if want.is_empty() {
            break;
        }
        meet(&mut courier, &mut bob, want, t);
    }
    assert!(courier.has_file(&magnet), "the courier carries it home");

    // --- and flies back. Another week; the publisher is still never involved. -
    let home = abroad + 2 * 86_400; // four days total, inside the expiry
    for round in 0..24 {
        let t = home + round * 60;
        let want = alice.fetch_n(&magnet, 8);
        if want.is_empty() {
            break;
        }
        meet(&mut alice, &mut courier, want, t);
    }

    assert!(alice.has_file(&magnet), "alice has the file she asked for two weeks ago");
    assert_eq!(
        alice.file_bytes(&magnet).expect("assembled"),
        pullable_file(),
        "byte for byte, having never met anyone who had it when she asked"
    );
}

#[test]
fn a_sneakernet_journey_has_a_deadline_and_it_is_the_publisher_s_expiry() {
    // The limit on carrying a file by hand is **time, not distance**. Chunks are
    // ordinary envelopes and carry the publisher's expiry, so a courier who takes
    // longer than that arrives holding bytes the far end will no longer serve.
    // Worth pinning: the failure is silent — the courier still holds the
    // manifest, still knows the file exists, and simply never completes.
    let mut publisher = Node::new("publisher", &[]);
    let (magnet, published) = publisher.publish_file("atlas.bin", &pullable_file(), ZERO_DEST, NOW);

    let mut holder = Node::new("holder", &[]);
    for f in &published {
        let wire = match f {
            Forward::Flood { bytes, .. } | Forward::Directed { bytes, .. } => bytes,
        };
        holder.on_rx(wire, 0, None, NOW);
    }
    for round in 0..24 {
        let want = holder.fetch_n(&magnet, 8);
        if want.is_empty() {
            break;
        }
        meet(&mut holder, &mut publisher, want, NOW + round * 60);
    }
    assert!(holder.has_file(&magnet), "the holder has it while it is fresh");

    // A courier who set out with the manifest and came back too late.
    let mut courier = Node::new("courier", &[]);
    let root = match &published[0] {
        Forward::Flood { bytes, .. } | Forward::Directed { bytes, .. } => bytes.clone(),
    };
    courier.on_rx(&root, 0, None, NOW);
    let late = NOW + DEFAULT_MESSAGE_EXPIRY_SECS + 3600;
    for round in 0..24 {
        let want = courier.fetch_n(&magnet, 8);
        if want.is_empty() {
            break;
        }
        meet(&mut courier, &mut holder, want, late + round * 60);
    }
    assert!(!courier.has_file(&magnet), "past the expiry there is nothing left to fetch");
}

/// Set a courier hunting on Alice's behalf, and return what it was asked for.
fn courier_adopts_alices_want(courier: &mut Node, magnet: &Id, published: &[Forward], now: u32) -> Vec<Id> {
    let mut alice = Node::new("alice", &[]);
    let root = match &published[0] {
        Forward::Flood { bytes, .. } | Forward::Directed { bytes, .. } => bytes.clone(),
    };
    alice.on_rx(&root, 0, None, now);
    courier.on_rx(&root, 0, None, now);

    let wanted = alice.missing(magnet, 4);
    assert!(!wanted.is_empty(), "alice has something to want");
    for id in &wanted {
        courier.on_rx(&Envelope::new(ty::WANT, ZERO_DEST, 0, id.to_vec()).wire(), 0, None, now);
    }
    assert!(courier.open_interests() > 0, "the courier took the job");
    wanted
}

#[test]
fn an_adopted_interest_outlives_the_meeting_that_created_it() {
    // M11-P. Fifteen minutes was shorter than any journey worth making, which
    // made adopted interest an online-only mechanism inside a delay-tolerant
    // protocol. The lease now comes from the object: as long as the chunks could
    // still turn up, and not one second longer.
    let mut publisher = Node::new("publisher", &[]);
    let mut courier = Node::new("courier", &[]);
    let (magnet, published) = publisher.publish_file("atlas.bin", &pullable_file(), ZERO_DEST, NOW);
    courier_adopts_alices_want(&mut courier, &magnet, &published, NOW);

    // A day later — far past the old fixed lease — the courier still remembers.
    let a_day = NOW + 86_400;
    courier.on_rx(&Envelope::new(ty::WANT, ZERO_DEST, 0, vec![0u8; 16]).wire(), 0, None, a_day);
    assert!(
        courier.open_interests() > 0,
        "a {INTEREST_LEASE_SECS}s lease would have forgotten this before the courier reached the airport"
    );

    // But not past the object it is hunting for. Nothing will serve these chunks
    // once the publisher's expiry has passed, so wanting them is pure cost.
    let too_late = NOW + MAX_INTEREST_LEASE_SECS + 3600;
    courier.on_rx(&Envelope::new(ty::WANT, ZERO_DEST, 0, vec![0u8; 16]).wire(), 0, None, too_late);
    assert_eq!(courier.open_interests(), 0, "an interest must not outlive the object");
}

#[test]
fn alices_want_rides_in_a_pocket_and_comes_home_answered() {
    // The whole of M11-P in one story: Alice's *specific* request survives a
    // restart, crosses to a mesh that never heard her ask, is answered there by a
    // stranger, and comes back. The courier never wanted the file itself.
    let mut publisher = Node::new("publisher", &[]);
    let (magnet, published) = publisher.publish_file("atlas.bin", &pullable_file(), ZERO_DEST, NOW);

    let mut courier = Node::new("courier", &[]);
    let wanted = courier_adopts_alices_want(&mut courier, &magnet, &published, NOW);

    // Onto the USB key.
    let blob = courier.pending_interests();
    assert!(blob.len() > 3, "there is something to carry");

    // --- the courier's node is powered off and brought up again elsewhere -----
    let abroad = NOW + 2 * 86_400;
    let mut courier = Node::new("courier", &[]);
    assert_eq!(courier.open_interests(), 0, "a fresh node remembers nothing by itself");
    assert!(courier.restore_pending_interests(&blob, abroad), "the blob is well formed");
    assert!(courier.open_interests() > 0, "and the want came back with it");

    // Bob, abroad, has the file. He never met Alice.
    let mut bob = Node::new("bob", &[]);
    for f in &published {
        let wire = match f {
            Forward::Flood { bytes, .. } | Forward::Directed { bytes, .. } => bytes,
        };
        bob.on_rx(wire, 0, None, abroad);
    }
    for round in 0..24 {
        let want = bob.fetch_n(&magnet, 8);
        if want.is_empty() {
            break;
        }
        meet(&mut bob, &mut publisher, want, abroad + round * 60);
    }
    assert!(bob.has_file(&magnet), "bob holds it");

    // The courier arrives and says what it is carrying. Nobody asked it to.
    let asking = courier.resume_interests(abroad);
    assert!(!asking.is_empty(), "an interest that survived is spoken somewhere new");
    meet(&mut courier, &mut bob, asking, abroad + 120);
    for id in &wanted {
        // `holds_named`, not `has`: a manifest names content, and `has` asks
        // about envelope ids (M11-M).
        assert!(courier.holds_named(id), "the courier got what Alice asked for, from a stranger");
    }

    // --- and home again, where Alice asks a second time ----------------------
    let home = abroad + 2 * 86_400;
    let mut served = 0usize;
    for (k, id) in wanted.iter().enumerate() {
        let rx = courier.on_rx(
            &Envelope::new(ty::WANT, ZERO_DEST, 0, id.to_vec()).wire(),
            0,
            None,
            home + k as u32,
        );
        served += usize::from(!rx.forwards.is_empty());
    }
    assert_eq!(served, wanted.len(), "every id Alice wanted is answered from the pocket");
}

#[test]
fn a_restored_interest_cannot_buy_a_longer_promise_than_the_node_would_make() {
    // Otherwise "restore" is a way to hand a node an obligation it never adopted
    // and could not have — a blob is not a signed manifest, and the resource
    // invariant does not care that the bytes came from disk.
    let mut n = Node::new("victim", &[]);
    let forged = {
        let mut b = vec![1u8, 0, 1];
        b.extend_from_slice(&[0xAB; 16]);
        b.extend_from_slice(&u32::MAX.to_be_bytes()); // "wanted until the heat death"
        b
    };
    assert!(n.restore_pending_interests(&forged, NOW));
    assert_eq!(n.open_interests(), 1);

    // Past the ceiling the node sets for itself, it is gone regardless.
    let past = NOW + MAX_INTEREST_LEASE_SECS + 3600;
    n.on_rx(&Envelope::new(ty::WANT, ZERO_DEST, 0, vec![0u8; 16]).wire(), 0, None, past);
    assert_eq!(n.open_interests(), 0, "a blob cannot mint an immortal interest");
}

#[test]
fn a_malformed_interest_blob_changes_nothing() {
    let mut n = Node::new("victim", &[]);
    assert!(!n.restore_pending_interests(&[], NOW));
    assert!(!n.restore_pending_interests(&[2, 0, 1], NOW), "wrong version byte");
    assert!(!n.restore_pending_interests(&[1, 0, 5], NOW), "count does not match length");
    assert_eq!(n.open_interests(), 0);
}

#[test]
fn restoring_interests_is_still_capped_by_the_local_limit() {
    // The blob is a local allowance, not an exemption from one.
    let mut n = Node::new("victim", &[]);
    n.set_limits(tight());
    let mut b = vec![1u8];
    b.extend_from_slice(&100u16.to_be_bytes());
    for i in 0..100u8 {
        b.extend_from_slice(&[i; 16]);
        b.extend_from_slice(&(NOW + 3600).to_be_bytes());
    }
    assert!(n.restore_pending_interests(&b, NOW));
    assert!(n.open_interests() <= n.limits().interests, "restored {}", n.open_interests());
}

#[test]
fn a_carried_interest_is_restated_on_a_cadence_not_every_tick() {
    // The cadence is what turns a remembered want into an asked one after a
    // journey — but a node that re-asked every tick would be a beacon of other
    // people's requests, which is the amplifier the whole invariant exists to
    // prevent.
    let mut publisher = Node::new("publisher", &[]);
    let mut courier = Node::new("courier", &[]);
    let (magnet, published) = publisher.publish_file("atlas.bin", &pullable_file(), ZERO_DEST, NOW);
    courier_adopts_alices_want(&mut courier, &magnet, &published, NOW);

    // First tick after adopting: it speaks, because it may have just arrived
    // somewhere new and nothing else would make it ask.
    let spoke = courier.tick(NOW + 1);
    assert!(spoke.iter().any(|f| matches!(f, Forward::Flood { .. })), "a node holding an interest says so");

    // Immediately again: silence. The question is standing, not a retry.
    assert!(courier.tick(NOW + 2).is_empty(), "it does not repeat itself every tick");

    // And once the cadence has elapsed, it asks again — this is the arrival path.
    let later = NOW + INTEREST_RESUME_SECS + 3;
    assert!(!courier.tick(later).is_empty(), "a standing question is restated");
}

#[test]
fn a_node_carrying_nothing_stays_quiet() {
    // The cadence must cost nothing when there is nothing to carry, or every
    // node in the mesh pays for a feature almost none of them are using.
    let mut n = Node::new("quiet", &[]);
    assert!(n.tick(NOW + INTEREST_RESUME_SECS + 1).is_empty());
    assert!(n.tick(NOW + 2 * INTEREST_RESUME_SECS + 2).is_empty());
}

#[test]
fn a_stranger_cannot_claim_a_deeper_want_than_local_policy_allows() {
    // M11-L. Depth is what bounds how far one neighbour's curiosity travels, and
    // it arrives as a byte on an *unsigned* frame from anyone in range. It was
    // the only one of recursive pull's three bounds taken on trust: the manifest
    // gate and the interest table are both checked locally, but reach was
    // whatever the asker claimed. `spore-sim`'s `malicious-want` measures the
    // difference across a mesh; this pins the rule at one node.
    let mut publisher = Node::new("publisher", &[]);
    let (magnet, fwds) = publisher.publish_file("bait.bin", &pullable_file(), ZERO_DEST, NOW);

    let ask_with = |depth: u8| -> u8 {
        let mut relay = Node::new("relay", &[]);
        for f in &fwds {
            let wire = match f {
                Forward::Flood { bytes, .. } | Forward::Directed { bytes, .. } => bytes,
            };
            relay.on_rx(wire, 0, None, NOW);
        }
        let missing = relay.missing(&magnet, 1);
        let mut payload: Vec<u8> = missing[0].to_vec();
        payload.push(depth);
        let rx = relay.on_rx(&Envelope::new(ty::WANT, ZERO_DEST, 0, payload).wire(), 0, None, NOW);
        // What it asks onward carries the depth it decided to grant, minus one.
        let out = match &rx.forwards[0] {
            Forward::Flood { bytes, .. } | Forward::Directed { bytes, .. } => bytes.clone(),
        };
        let (e, _) = Envelope::decode(&out).expect("a WANT");
        e.payload[e.payload.len() - 1]
    };

    let honest = ask_with(DEFAULT_WANT_DEPTH);
    assert_eq!(honest, DEFAULT_WANT_DEPTH - 1, "an honest budget is passed on decremented");
    assert_eq!(ask_with(255), honest, "a forged budget buys exactly what an honest one does");

    // Asking for *less* must keep working — a relayed WANT always carries a
    // decremented depth, so a clamp that rejected anything but the default would
    // break recursion at the second hop.
    assert_eq!(ask_with(3), 2, "a smaller budget is honoured, not raised");
}
#[test]
fn a_file_published_twice_shares_every_chunk_with_itself() {
    // M11-M. This test used to be `..._shares_nothing_with_itself`, and it
    // passed: a byte-identical file published twice had **0 of 16 envelopes** in
    // common, because a chunk's name was the hash of its whole envelope and that
    // envelope carried a random per-publish `file_id`, a topic derived from it,
    // and a wall-clock expiry.
    //
    // Chunks are now named by their **content** — the hash of `[CHUNK_TAG][bytes]`
    // and nothing else — so the same bytes get the same name from any publisher
    // at any time. The envelopes still differ, and should: an envelope is a
    // message, with an expiry and a destination. What changed is that the file
    // layer stopped confusing the two.
    let body = pullable_file();

    let mut a = Node::new("a", &[]);
    let mut b = Node::new("b", &[]);
    let (m1, f1) = a.publish_file("v1.bin", &body, ZERO_DEST, NOW);
    let (m2, _) = b.publish_file("v1.bin", &body, ZERO_DEST, NOW + 3600);

    // A third node learns the file from A, then meets B — who published the same
    // bytes an hour later, having never met A.
    let mut c = Node::new("c", &[]);
    for f in &f1 {
        let wire = match f {
            Forward::Flood { bytes, .. } | Forward::Directed { bytes, .. } => bytes,
        };
        c.on_rx(wire, 0, None, NOW);
    }
    let wanted = c.missing(&m1, 9999);
    assert!(!wanted.is_empty(), "c still needs most of the file");

    // Every id A's manifest names, B can serve — without the two publishers ever
    // having exchanged a byte, and without sharing a magnet.
    assert_ne!(m1, m2, "different publishers, different signed roots");
    let servable = wanted.iter().filter(|id| b.holds_named(id)).count();
    assert_eq!(servable, wanted.len(), "B can serve all {} of them", wanted.len());
}

#[test]
fn identical_content_inside_one_file_is_stored_once() {
    // The same property seen from the other side, and the reason `missing` has
    // to return *distinct* ids: 40 kB of one repeated byte is thirty chunks and
    // two distinct objects. Before content addressing it was thirty objects.
    let mut a = Node::new("a", &[]);
    // Big enough to be many chunks at `CHUNK_AVG_BYTES`, and entirely uniform,
    // so every one of them is the same object.
    let (magnet, _) = a.publish_file("flat.bin", &vec![0xAA; 400_000], ZERO_DEST, NOW);
    let m = a.files().into_iter().find(|(id, ..)| *id == magnet).expect("published");
    assert!(m.2 >= 400_000, "the file is still 400 kB");

    // Distinct leaves, counted through the manifest.
    let mut leaves: HashSet<Id> = HashSet::new();
    let mut visits = 0usize;
    let held = a.manifests.get(&magnet).unwrap().clone();
    a.walk_tree(&held, &mut |id, depth, _| {
        if depth == 0 {
            visits += 1;
            leaves.insert(*id);
        }
        true
    });
    assert!(visits >= 20, "the file is still many chunks long, got {visits}");
    assert!(leaves.len() <= 2, "but only {} distinct objects", leaves.len());

    // And it still reassembles byte for byte — dedup must not lose repeats.
    assert_eq!(a.file_bytes(&magnet).as_deref(), Some(&vec![0xAA; 400_000][..]));
}

#[test]
fn a_departing_node_can_cancel_what_it_cannot_enumerate() {
    // A fetcher names ids by walking a manifest tree, and can only descend into
    // the parts it holds — so ids it asked for while a parent was still
    // resolving may no longer be nameable. `spore-sim` measured a fetcher able
    // to list 37 of the 45 interests its neighbour was holding for it.
    //
    // So "cancel these ids" cannot be complete, and a wildcard is not a
    // convenience but the only correct form for a node that is leaving.
    let mut publisher = Node::new("publisher", &[]);
    let mut relay = Node::new("relay", &[]);
    let (magnet, fwds) = publisher.publish_file("f.bin", &pullable_file(), ZERO_DEST, NOW);
    for f in &fwds {
        let wire = match f {
            Forward::Flood { bytes, .. } | Forward::Directed { bytes, .. } => bytes,
        };
        relay.on_rx(wire, 0, None, NOW);
    }
    for id in relay.missing(&magnet, 8) {
        relay.on_rx(&Envelope::new(ty::WANT, ZERO_DEST, 0, id.to_vec()).wire(), 0, None, NOW);
    }
    assert!(relay.open_interests() > 0, "the relay is hunting on someone's behalf");

    // One frame, no ids, and the relay is holding nothing for us.
    let mut e = Envelope::new(ty::WANT, ZERO_DEST, 0, Vec::new());
    e.flags |= fl::CANCEL;
    let rx = relay.on_rx(&e.wire(), 0, None, NOW);
    assert_eq!(relay.open_interests(), 0, "a departing neighbour releases all of it");
    assert!(!rx.forwards.is_empty(), "and the unwind carries on upstream");
}

#[test]
fn a_wildcard_cancel_still_only_speaks_for_the_sender() {
    // The wildcard is the widest cancel there is, so it is the one worth
    // checking cannot speak for anyone else: a hostile neighbour saying "I want
    // nothing" must not silence a fetch somebody else is waiting on.
    let mut publisher = Node::new("publisher", &[]);
    let mut relay = Node::new("relay", &[]);
    let (magnet, fwds) = publisher.publish_file("f.bin", &pullable_file(), ZERO_DEST, NOW);
    for f in &fwds {
        let wire = match f {
            Forward::Flood { bytes, .. } | Forward::Directed { bytes, .. } => bytes,
        };
        relay.on_rx(wire, 0, None, NOW);
    }
    for id in relay.missing(&magnet, 8) {
        relay.on_rx(&Envelope::new(ty::WANT, ZERO_DEST, 0, id.to_vec()).wire(), 0, None, NOW);
    }
    let wanted_by_zero = relay.open_interests();
    assert!(wanted_by_zero > 0);

    // A stranger on another link says it wants nothing. It never asked for
    // anything, so it is releasing nothing.
    let mut e = Envelope::new(ty::WANT, ZERO_DEST, 0, Vec::new());
    e.flags |= fl::CANCEL;
    let rx = relay.on_rx(&e.wire(), 1, None, NOW);
    assert_eq!(relay.open_interests(), wanted_by_zero, "the real waiter is untouched");
    assert!(rx.forwards.is_empty(), "and nothing unwinds");
}

#[test]
fn two_publishers_on_different_links_cut_a_file_the_same_way() {
    // The other half of M11-M, and the reason the chunk parameters are protocol
    // constants rather than `mtu - 64`. Content addressing names identical bytes
    // identically — but only if both publishers *produce* identical bytes, and a
    // Wi-Fi node cutting 1336-byte chunks and a LoRa node cutting 173-byte ones
    // never do. Link fragmentation (M11-D) is what made the MTU irrelevant here:
    // a hop that cannot carry a chunk splits it, so the publisher no longer has
    // to guess at the narrowest link on a path it cannot see.
    let body = pullable_file();

    let mut wifi = Node::new("wifi", &[]);
    let mut lora = Node::new("lora", &[]);
    lora.mtu = 237;
    let (wm, wf) = wifi.publish_file("atlas.bin", &body, ZERO_DEST, NOW);
    lora.publish_file("atlas.bin", &body, ZERO_DEST, NOW + 3600);

    // A node that learned the file from the Wi-Fi publisher can be served every
    // part of it by the LoRa publisher, which has never met either of them.
    let mut c = Node::new("c", &[]);
    for f in &wf {
        let wire = match f {
            Forward::Flood { bytes, .. } | Forward::Directed { bytes, .. } => bytes,
        };
        c.on_rx(wire, 0, None, NOW);
    }
    let wanted = c.missing(&wm, 9999);
    assert!(!wanted.is_empty(), "there is something left to want");
    let servable = wanted.iter().filter(|id| lora.holds_named(id)).count();
    assert_eq!(servable, wanted.len(), "the LoRa publisher can serve all {} parts", wanted.len());
}

#[test]
fn appending_to_a_file_reuses_every_earlier_chunk() {
    // What a **static** chunk size buys. Boundaries are at fixed offsets from the
    // start, so appending does not move any of them: every chunk before the
    // append is byte-identical, has the same content id, and is already held.
    let v1 = pullable_file();
    let mut v2 = v1.clone();
    v2.extend_from_slice(&[0xFF; 30_000]);

    let mut a = Node::new("a", &[]);
    let (m1, _) = a.publish_file("log.bin", &v1, ZERO_DEST, NOW);
    let before = a.store_len();
    let (m2, _) = a.publish_file("log.bin", &v2, ZERO_DEST, NOW);
    let added = a.store_len() - before;

    assert_ne!(m1, m2, "a longer file is a different file");
    let parts = v1.len() / file::CHUNK_BYTES;
    assert!(
        added < parts / 2,
        "appending stored {added} new objects; the {parts} chunks of the original should be reused"
    );
}

#[test]
fn inserting_a_byte_re_chunks_the_file_and_that_is_a_known_cost() {
    // **The trade-off static chunking accepts**, recorded rather than left to be
    // rediscovered. Boundaries are offsets from the start of the file, so
    // inserting a byte at the front shifts every one of them: the bytes are the
    // same, the chunks are not, and nothing is shared.
    //
    // Content-*defined* boundaries (a rolling hash choosing cut points) would fix
    // this — measured at the time as keeping >90% of chunks across a one-byte
    // prepend — at the cost of variable-length chunks, a gear table in the spec,
    // and losing the ability to compute which chunk holds a given byte. The
    // decision was static; this test is where it is written down, and it is the
    // test to invert if that ever changes.
    let v1 = pullable_file();
    let mut v2 = v1.clone();
    v2.insert(0, 0xFF);

    let mut a = Node::new("a", &[]);
    a.publish_file("doc.bin", &v1, ZERO_DEST, NOW);
    let before = a.store_len();
    a.publish_file("doc.bin", &v2, ZERO_DEST, NOW);
    let added = a.store_len() - before;

    let parts = v1.len() / file::CHUNK_BYTES;
    assert!(
        added * 2 > parts,
        "a prepend re-chunks the file: expected most of {parts} parts to be new, got {added}"
    );
}

#[test]
fn a_chunk_is_the_same_size_whatever_link_the_publisher_has() {
    // The regression this guards is the one that started M11-M: chunk size came
    // from `mtu - 64`, so a Wi-Fi publisher and a LoRa publisher cut the same
    // file differently and shared no content ids at all. A chunk is a file-layer
    // object and its size is a protocol constant; the link has no say.
    let body = pullable_file();
    let ids_from = |mtu: usize| -> Vec<Id> {
        let mut n = Node::new("p", &[]);
        n.mtu = mtu;
        let (magnet, _) = n.publish_file("same.bin", &body, ZERO_DEST, NOW);
        let held = n.manifests.get(&magnet).expect("published").clone();
        let mut leaves = Vec::new();
        n.walk_tree(&held, &mut |id, depth, _| {
            if depth == 0 {
                leaves.push(*id);
            }
            true
        });
        leaves
    };
    let wide = ids_from(1400);
    let narrow = ids_from(237);
    let tiny = ids_from(54);

    assert_eq!(wide, narrow, "a 237-byte link must cut the file exactly as a 1400-byte one does");
    assert_eq!(wide, tiny, "and so must a 54-byte one");
    assert_eq!(
        wide.len(),
        body.len().div_ceil(file::CHUNK_BYTES),
        "one chunk per CHUNK_BYTES of file, whatever the link"
    );
}

#[test]
fn a_chunk_larger_than_a_frame_still_crosses_it() {
    // The other half of the separation, and the dependency it creates: a chunk is
    // routinely bigger than a hop's frame, so it only moves because the *link*
    // splits it. Nothing about the chunk changes — the far end reassembles the
    // envelope before the router sees anything, and the content id matches.
    let mut publisher = Node::new("publisher", &[]);
    let (magnet, _) = publisher.publish_file("big.bin", &pullable_file(), ZERO_DEST, NOW);
    let held = publisher.manifests.get(&magnet).expect("published").clone();
    let first = held.chunk_ids[0];
    let wire = publisher.named_wire(&first).expect("a chunk");
    assert!(wire.len() > 237, "a chunk exceeds a LoRa frame, which is the point");

    // Split it for a 237-byte link and put it back, as a bridge pair does.
    let pieces = linkfrag::split_for_link(&wire, 237, 1);
    assert!(pieces.len() > 1, "the link had to cut it into {} pieces", pieces.len());
    let mut rx = linkfrag::Reassembler::default();
    let mut rebuilt = None;
    for p in &pieces {
        if let Some(whole) = rx.accept(0, p, NOW) {
            rebuilt = Some(whole);
        }
    }
    let rebuilt = rebuilt.expect("the far end reassembles it");
    assert_eq!(rebuilt, wire, "byte for byte");

    let (e, _) = Envelope::decode(&rebuilt).expect("a chunk envelope");
    assert_eq!(file::content_id(&e.payload), first, "and it is still the same named content");
}
