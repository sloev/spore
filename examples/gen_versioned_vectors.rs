//! Emit vectors for the **normative but versioned** tier — the part of the
//! protocol two implementations must also agree on, and which `vectors.json`
//! deliberately does not cover.
//!
//!   cargo run --example gen_versioned_vectors > reference/versioned_vectors.json
//!
//! **Why a second file rather than more keys in the first.** The spec's tier
//! table (SPEC.md, Part I preamble) puts §1, §2 and the file layer's content id
//! in one row — frozen, changeable only by a `ver` bump, pinned by
//! `reference/vectors.json`, which CI refuses to edit. Link framing, the INV/WANT
//! payload shapes and the file-layer tags are in the *next* row down: normative,
//! but changeable with a minor release so long as both ends of one link agree.
//!
//! Those are different promises, and putting them in one file would make them
//! look like one promise. It would also mean every legitimate minor-release
//! change to link framing had to carry the `allow-frozen-change` label, which is
//! how a guard stops meaning anything. Two files, two tiers, and the tier is
//! legible from the filename.
//!
//! What this buys: the roadmap's complaint was that two implementations "could
//! agree perfectly on an envelope and still fail to talk". An envelope is where
//! interop *starts*. A node that cannot parse an INV, cannot reassemble a
//! fragmented frame, and cannot read a manifest is wire-compatible and useless.

use ed25519_dalek::SigningKey;
use spore::*;

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn hexes(v: &[Vec<u8>]) -> String {
    let items: Vec<String> = v.iter().map(|x| format!("\"{}\"", hex(x))).collect();
    format!("[{}]", items.join(", "))
}

fn ids(v: &[Id]) -> String {
    let items: Vec<String> = v.iter().map(|x| format!("\"{}\"", hex(x))).collect();
    format!("[{}]", items.join(", "))
}

/// A deterministic 16-byte id, so the vectors never depend on a hash of
/// something the reader cannot reconstruct.
fn id_n(n: u8) -> Id {
    [n; 16]
}

/// One manifest, with the fields a decoder must recover and the bytes it must
/// recover them from.
fn manifest_entry(label: &str, man: &file::Manifest, note: &str, last: bool) -> String {
    let enc = man.encode();
    let mut o = format!("    \"{label}\": {{\n");
    o.push_str(&format!(
        "      \"depth\": {}, \"count\": {}, \"total_len\": {},\n",
        man.depth, man.count, man.total_len
    ));
    o.push_str(&format!("      \"chunk_size\": {}, \"name\": \"{}\",\n", man.chunk_size, man.name));
    o.push_str(&format!("      \"chunk_ids\": {},\n", ids(&man.chunk_ids)));
    o.push_str(&format!("      \"encoded\": \"{}\",\n", hex(&enc)));
    o.push_str(&format!("      \"content_id\": \"{}\",\n", hex(&file::content_id(&enc))));
    o.push_str(&format!("      \"note\": \"{note}\"\n"));
    o.push_str(if last { "    }\n" } else { "    },\n" });
    o
}

/// The cases worth pinning: the ordinary path, and then every way a frame stops.
fn forwarding_cases() -> Vec<(String, String, String)> {
    let c = |i: &str, s: &str, w: &str| (i.to_string(), s.to_string(), w.to_string());
    vec![
        c("DATA, FLOOD, hops=4, unseen", "topic followed",
          "the ordinary path: deliver locally and relay on every interface but the one it arrived on"),
        c("the same DATA a second time", "id already seen",
          "dedup is by envelope id with hops zeroed, so one message arriving by two paths is one message"),
        c("DATA, FLOOD, hops=0", "topic followed",
          "hops counts down and forwarding stops at zero; the frame is still for us, it just stops here"),
        c("DATA addressed elsewhere, hops=4", "dest is neither ours nor a topic we follow",
          "custody: unicast we cannot read is carried anyway, which is what makes the mesh a courier"),
        c("INV listing an id we lack", "empty store",
          "INV and WANT are per-link, hops=0, consumed on receipt: never stored, never deduped, never relayed"),
        c("WANT for an id we hold", "that envelope in the store",
          "answered on the link it arrived on rather than flooded, because exactly one peer asked"),
        c("WANT for an id we lack", "empty store",
          "nothing to serve; the node may adopt an interest and ask onward, which is not a reply to this link"),
        c("DATA with created_at far in the future", "beyond MAX_CLOCK_SKEW_SECS",
          "post-dated frames are refused at admission: one would otherwise outrank every honest envelope at eviction"),
        c("DATA whose payload was altered after signing", "signature no longer verifies",
          "relayed and delivered anyway, and that is the rule rather than a gap: SPEC section 5 says verify before binding trust state, do not verify to forward. A relay moves bytes it cannot read. What the bad signature costs the sender is attribution - the envelope binds no path, names no neighbour, and reaches the app flagged for the app to check - so a second implementation that drops it here is not stricter, it is broken, because it would also drop every envelope it merely lacks the key for"),
    ]
}

/// Run one case against a real node and describe what came back.
fn observe(input: &str, state: &str) -> String {
    let mut n = Node::from_seed("observer", &["news"], &[9u8; 32]);
    let sender = SigningKey::from_bytes(&[8u8; 32]);
    let now: u32 = 1_700_000_000;
    let iface: Iface = 0;

    // Build the frame this case is about.
    let raw: Vec<u8> = if input.starts_with("INV") {
        Envelope::new(ty::INV, ZERO_DEST, 0, id_n(0xf1).to_vec()).wire()
    } else if input.starts_with("WANT") {
        Envelope::new(ty::WANT, ZERO_DEST, 0, id_n(0xf1).to_vec()).wire()
    } else {
        let dest = if state.starts_with("dest is neither") { [0x5au8; 8] } else { topic_of("news") };
        let created = if state.starts_with("beyond") { now + 86_400 } else { now };
        let mut e = Envelope::new(ty::DATA, dest, created, b"the dam holds".to_vec());
        e.flags |= fl::FLOOD;
        e.hops = if input.contains("hops=0") { 0 } else { 4 };
        e.sign(&sender);
        let mut w = e.wire();
        if state.starts_with("signature no longer") {
            let p = w.len() - 64 - 13;
            w[p] ^= 0x01;
        }
        w
    };

    // Put the node into the state this case is about.
    if state.starts_with("that envelope in the store") {
        // Serve a WANT: the node must already hold what is being asked for. Ask
        // for something it really has rather than a made-up id.
        let held = Envelope::new(ty::DATA, topic_of("news"), now, b"held".to_vec()).wire();
        n.on_rx(&held, 1, None, now);
        let (e, _) = Envelope::decode(&held).expect("built it");
        let want = Envelope::new(ty::WANT, ZERO_DEST, 0, e.id().to_vec()).wire();
        return describe(n.on_rx(&want, iface, None, now));
    }
    if state.starts_with("id already seen") {
        n.on_rx(&raw, iface, None, now); // the first arrival
    }

    describe(n.on_rx(&raw, iface, None, now))
}

/// What a node did, in the vocabulary the spec uses.
fn describe(rx: Rx) -> String {
    let mut parts: Vec<String> = Vec::new();
    if !rx.delivered.is_empty() {
        parts.push(format!("deliver x{}", rx.delivered.len()));
    }
    for f in &rx.forwards {
        parts.push(match f {
            Forward::Flood { except, .. } => format!("Flood{{except: {except}}}"),
            Forward::Directed { iface, .. } => format!("Directed{{iface: {iface}}}"),
        });
    }
    if parts.is_empty() {
        "nothing".into()
    } else {
        parts.join(" + ")
    }
}

fn main() {
    let mut out = String::from("{\n");

    // ---- link framing (§3 / Part II) --------------------------------------
    //
    // The header is seven bytes, `[0xF6][set:2][idx:2][count:2]`, and the first
    // byte is what tells a receiver it is looking at a piece rather than a whole
    // envelope — VER is 0x01/0x02, never 0xF6. A decoder that gets this wrong
    // does not fail loudly; it feeds a fragment to the envelope parser and
    // reports a malformed frame, which is the least useful error in the protocol.
    // A payload long enough to actually fragment at the MTU below — the first
    // draft of these vectors used a 13-byte payload and "split at 40" returned
    // one piece, which is a passing test of nothing.
    let payload: Vec<u8> = (0..100u16).map(|i| (i % 251) as u8).collect();
    let wire = Envelope::new(ty::DATA, topic_of("news"), 1_700_000_000, payload).wire();
    let pieces = linkfrag::split(&wire, 40, 0x1234);
    let whole = linkfrag::split(&wire, 4096, 0x1234);
    assert!(pieces.len() > 2, "the split vector must actually fragment");
    assert_eq!(whole.len(), 1, "a frame under the MTU is one unwrapped piece");

    out.push_str("  \"link_framing\": {\n");
    out.push_str(&format!("    \"magic\": \"{:02x}\",\n", linkfrag::LINK_FRAG_MAGIC));
    out.push_str(&format!("    \"overhead\": {},\n", linkfrag::LINK_FRAG_OVERHEAD));
    out.push_str(&format!("    \"frame\": \"{}\",\n", hex(&wire)));
    out.push_str("    \"split_mtu_40\": {\n");
    out.push_str("      \"set_id\": 4660,\n");
    out.push_str(&format!("      \"pieces\": {},\n", hexes(&pieces)));
    out.push_str(&format!("      \"count\": {}\n", pieces.len()));
    out.push_str("    },\n");
    // A frame that already fits is returned whole and unwrapped — no header at
    // all. A decoder that always strips seven bytes passes every fragmenting
    // test and corrupts every link wide enough not to fragment.
    out.push_str("    \"split_mtu_4096_fits_whole\": {\n");
    out.push_str(&format!("      \"pieces\": {},\n", hexes(&whole)));
    out.push_str("      \"note\": \"a frame that fits is returned unchanged and unwrapped\"\n");
    out.push_str("    },\n");
    // KISS, including both escapes, because a payload containing 0xC0 is the
    // case every hand-written serial decoder gets wrong once.
    let kissed = kiss::encode(&[0x01, 0xC0, 0xDB, 0x02]);
    out.push_str("    \"kiss\": {\n");
    out.push_str("      \"frame\": \"01c0db02\",\n");
    out.push_str(&format!("      \"encoded\": \"{}\",\n", hex(&kissed)));
    out.push_str("      \"note\": \"FEND C0 -> DB DC, FESC DB -> DB DD; leading C0 00 is FEND + command\"\n");
    out.push_str("    }\n");
    out.push_str("  },\n");

    // ---- gossip: INV and WANT (§6) ----------------------------------------
    //
    // Both are bare concatenations of 16-byte ids in an envelope with
    // `dest = ZERO_DEST` and `created_at = 0`, consumed on receipt and never
    // relayed. The two variations below are the ones a second implementation is
    // most likely to miss, and both are silent failures rather than loud ones.
    let inv_ids = vec![id_n(0xa1), id_n(0xa2)];
    let inv_payload: Vec<u8> = inv_ids.iter().flatten().copied().collect();
    let inv = Envelope::new(ty::INV, ZERO_DEST, 0, inv_payload).wire();

    let want_ids = vec![id_n(0xa2)];
    let want_payload: Vec<u8> = want_ids.iter().flatten().copied().collect();
    let want = Envelope::new(ty::WANT, ZERO_DEST, 0, want_payload.clone()).wire();

    // A WANT may carry one trailing byte of remaining depth. Ids are 16 bytes,
    // so an odd payload length is unambiguous — but a decoder that only does
    // `chunks(16)` drops the byte and silently answers at the default budget.
    let mut with_depth = want_payload.clone();
    with_depth.push(3);
    let want_depth = Envelope::new(ty::WANT, ZERO_DEST, 0, with_depth).wire();

    // A cancel is a WANT with `fl::CANCEL` and *no* depth byte — an even length,
    // so an older build parses it as a plain WANT rather than reading half an id.
    let mut cancel = Envelope::new(ty::WANT, ZERO_DEST, 0, want_payload);
    cancel.flags |= fl::CANCEL;
    let cancel_wire = cancel.wire();

    // An empty cancel payload is the wildcard: "retire every interest I placed
    // with you". It is the shape that looks most like a malformed frame and is
    // the one a reassembler must not discard.
    let mut cancel_all = Envelope::new(ty::WANT, ZERO_DEST, 0, Vec::new());
    cancel_all.flags |= fl::CANCEL;

    out.push_str("  \"gossip\": {\n");
    out.push_str(&format!(
        "    \"ty_inv\": {}, \"ty_want\": {}, \"fl_cancel\": {},\n",
        ty::INV,
        ty::WANT,
        fl::CANCEL
    ));
    out.push_str(&format!("    \"max_ids_per_gossip\": {},\n", MAX_IDS_PER_GOSSIP));
    out.push_str(&format!("    \"inv\": {{ \"ids\": {}, \"wire\": \"{}\" }},\n", ids(&inv_ids), hex(&inv)));
    out.push_str(&format!(
        "    \"want\": {{ \"ids\": {}, \"wire\": \"{}\" }},\n",
        ids(&want_ids),
        hex(&want)
    ));
    out.push_str(&format!(
        "    \"want_with_depth\": {{ \"ids\": {}, \"depth\": 3, \"wire\": \"{}\", \"note\": \"odd payload length: the trailing byte is remaining depth, and the receiver clamps it to its own default\" }},\n",
        ids(&want_ids), hex(&want_depth)));
    out.push_str(&format!(
        "    \"cancel\": {{ \"ids\": {}, \"wire\": \"{}\", \"note\": \"WANT + CANCEL, never a depth byte, so the length stays even\" }},\n",
        ids(&want_ids), hex(&cancel_wire)));
    out.push_str(&format!(
        "    \"cancel_all\": {{ \"ids\": [], \"wire\": \"{}\", \"note\": \"empty payload is the wildcard: retire every interest this peer placed\" }}\n",
        hex(&cancel_all.wire())));
    out.push_str("  },\n");

    // ---- file layer: manifest encodings (§8) ------------------------------
    //
    // Three shapes behind three tags, and the depth byte is present for two of
    // them and absent for the third. A decoder that assumes a fixed header
    // offset reads a leaf's `file_id` shifted by one.
    let leaf = file::Manifest {
        file_id: id_n(0xb0),
        chunk_size: file::CHUNK_BYTES as u32,
        count: 2,
        total_len: 5000,
        name: "dam.txt".into(),
        chunk_ids: vec![id_n(0xc1), id_n(0xc2)],
        depth: 0,
        hdr_id: [0u8; 16],
    };
    let tree = file::Manifest {
        depth: 1,
        name: "big.bin".into(),
        count: 2,
        total_len: 9_000_000,
        chunk_ids: vec![id_n(0xd1), id_n(0xd2)],
        ..leaf.clone()
    };
    let sealed = file::Manifest { hdr_id: id_n(0xe0), name: String::new(), ..leaf.clone() };

    out.push_str("  \"file_layer\": {\n");
    out.push_str(&format!(
        "    \"tags\": {{ \"manifest\": \"{:02x}\", \"chunk\": \"{:02x}\", \"tree\": \"{:02x}\", \"sealed\": \"{:02x}\" }},\n",
        file::MANIFEST_TAG, file::CHUNK_TAG, file::TREE_TAG, file::SEALED_TAG));
    out.push_str(&format!(
        "    \"chunk_bytes\": {}, \"max_depth\": {},\n",
        file::CHUNK_BYTES,
        file::MAX_DEPTH
    ));
    out.push_str(&manifest_entry(
        "leaf",
        &leaf,
        "MANIFEST_TAG, no depth byte: a depth-0 manifest encodes exactly as it did before trees existed",
        false,
    ));
    out.push_str(&manifest_entry(
        "tree",
        &tree,
        "TREE_TAG carries a depth byte; chunk_ids name sub-manifests, not chunks",
        false,
    ));
    out.push_str(&manifest_entry("sealed", &sealed, "SEALED_TAG carries depth and a 16-byte hdr_id naming the header envelope; name is empty and total_len is the plaintext length", true));
    out.push_str("  },\n");

    // ---- forwarding decisions (§6) ----------------------------------------
    //
    // The one section here that is not a byte layout, and the one that cannot be
    // written down honestly. Two implementations can parse every frame above and
    // still not interoperate if they disagree about what to *do* with one — and
    // the disagreements that matter are all about what does **not** get
    // forwarded, which is exactly the part a hand-written table gets wrong.
    //
    // So these are **observed**: each case drives a real `Node` and records what
    // it actually did. The first draft of this section was prose, and it claimed
    // a frame is forwarded while `hops < max`. `hops` counts *down* and stops at
    // zero. That is the whole argument for generating this rather than writing it.
    out.push_str("  \"forwarding\": [\n");
    let mut cases: Vec<String> = Vec::new();
    for (input, state, why) in forwarding_cases() {
        let observed = observe(&input, &state);
        cases.push(format!(
            "    {{ \"input\": \"{input}\", \"state\": \"{state}\", \"observed\": \"{observed}\", \"why\": \"{why}\" }}"
        ));
    }
    out.push_str(&cases.join(",\n"));
    out.push_str("\n  ]\n}\n");

    print!("{out}");
}
