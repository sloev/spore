//! Browser (wasm32) node ABI.
//!
//! Exposes a real SPORE `Node` — create, subscribe, send, receive — to
//! JavaScript, without `wasm-bindgen`. Bytes cross the boundary through
//! `spore_alloc`/`spore_free`, and byte results come back **packed into an i64**
//! as `(ptr << 32) | len` (wasm pointers are 32-bit), which JS unpacks and frees.
//!
//! `send`/`recv` return a small self-describing blob of `{forwards, delivered}`:
//!
//!     u32 n_forwards, [u32 len, bytes]*n_forwards,
//!     u32 n_delivered,[u32 len, bytes]*n_delivered
//!
//! forwards are envelope wires to transmit on every transport; delivered are
//! envelopes addressed to us. See `web/spore.mjs` for the JS side.

use crate::*;

// -- randomness: routed to a JS import so the wasm needs no wasm-bindgen ------
//
// The host must supply `env.spore_fill_random(ptr, len)` (JS fills it from
// crypto.getRandomValues). Every OsRng call in the crate flows through here.
// The `wasm_import_module` attribute names the import module ("env") so newer
// wasm-lld emits it as an import instead of erroring on an undefined symbol.
#[link(wasm_import_module = "env")]
extern "C" {
    fn spore_fill_random(ptr: *mut u8, len: usize);
}
fn wasm_getrandom(buf: &mut [u8]) -> Result<(), getrandom::Error> {
    unsafe { spore_fill_random(buf.as_mut_ptr(), buf.len()) };
    Ok(())
}
getrandom::register_custom_getrandom!(wasm_getrandom);

// -- raw memory the JS side allocates/frees ---------------------------------

/// Allocate `len` zeroed bytes in wasm memory; returns a pointer JS writes into.
#[no_mangle]
pub extern "C" fn spore_alloc(len: usize) -> *mut u8 {
    let bs = vec![0u8; len].into_boxed_slice();
    Box::into_raw(bs).cast::<u8>()
}

/// Free a buffer from `spore_alloc` or a packed result (JS passes the exact len).
///
/// # Safety
/// `ptr`/`len` must describe a live allocation from this module.
#[no_mangle]
pub unsafe extern "C" fn spore_free(ptr: *mut u8, len: usize) {
    if !ptr.is_null() {
        let s = std::slice::from_raw_parts_mut(ptr, len);
        drop(Box::from_raw(s as *mut [u8]));
    }
}

fn pack(v: Vec<u8>) -> i64 {
    let bs = v.into_boxed_slice();
    let len = bs.len() as i64;
    let ptr = Box::into_raw(bs).cast::<u8>() as i64;
    (ptr << 32) | len
}

/// How a forward should travel, carried across the ABI so the caller does not
/// have to guess. See [`forward_wires`].
const FWD_FLOOD: u8 = 0;
const FWD_DIRECTED: u8 = 1;

fn blob(forwards: Vec<(u8, Iface, Vec<u8>)>, delivered: Vec<Vec<u8>>) -> Vec<u8> {
    let mut o = Vec::new();
    o.extend_from_slice(&(forwards.len() as u32).to_le_bytes());
    for (kind, iface, item) in &forwards {
        o.push(*kind);
        o.extend_from_slice(&iface.to_le_bytes());
        o.extend_from_slice(&(item.len() as u32).to_le_bytes());
        o.extend_from_slice(item);
    }
    o.extend_from_slice(&(delivered.len() as u32).to_le_bytes());
    for item in &delivered {
        o.extend_from_slice(&(item.len() as u32).to_le_bytes());
        o.extend_from_slice(item);
    }
    o
}

/// Wires plus **how each one should travel**.
///
/// The router has already decided this — `Flood` names the interface to skip,
/// `Directed` names the one to use — and flattening both variants to bare bytes
/// threw that decision away. A caller with no routing information then has to
/// guess, and the only available guess (never send back where it came from) is
/// correct for a flood and wrong for a directed reply: on a link with a single
/// interface it silently drops every delivery receipt, because the receipt's only
/// route home is the one the guess excludes.
///
/// So the kind **and the interface** travel with the bytes: a consumer applies
/// split-horizon only to a `FWD_FLOOD` whose interface is not `NO_IFACE`.
fn forward_wires(fs: Vec<Forward>) -> Vec<(u8, Iface, Vec<u8>)> {
    fs.into_iter()
        .map(|f| match f {
            // For a flood the interface is the one to SKIP; NO_IFACE means skip
            // nothing, which is what a locally-originated envelope says.
            Forward::Flood { except, bytes } => (FWD_FLOOD, except, bytes),
            // For a directed forward it is the one to USE.
            Forward::Directed { iface, bytes, .. } => (FWD_DIRECTED, iface, bytes),
        })
        .collect()
}

// -- node lifecycle ----------------------------------------------------------

/// Create a node. Returns an opaque handle to free with `spore_node_free`.
#[no_mangle]
pub extern "C" fn spore_node_new() -> *mut Node {
    Box::into_raw(Box::new(Node::new("web", &[])))
}

/// # Safety
/// `n` must be a handle from `spore_node_new`, not yet freed.
#[no_mangle]
pub unsafe extern "C" fn spore_node_free(n: *mut Node) {
    if !n.is_null() {
        drop(Box::from_raw(n));
    }
}

/// Create a node from a fixed 32-byte signing seed, restoring a persisted
/// identity (same address and keys). `seed` points to 32 readable bytes.
///
/// # Safety
/// `seed` points to 32 readable bytes.
#[no_mangle]
pub unsafe extern "C" fn spore_node_new_seeded(seed: *const u8) -> *mut Node {
    let mut s = [0u8; 32];
    std::ptr::copy_nonoverlapping(seed, s.as_mut_ptr(), 32);
    Box::into_raw(Box::new(Node::from_seed("web", &[], &s)))
}

/// Write the node's 8-byte address to `out`.
///
/// # Safety
/// `n` is a valid handle; `out` points to 8 writable bytes.
#[no_mangle]
pub unsafe extern "C" fn spore_node_addr(n: *mut Node, out: *mut u8) {
    std::ptr::copy_nonoverlapping((*n).addr.as_ptr(), out, 8);
}

/// Write the node's 32-byte signing seed (its whole identity) to `out`, so a
/// caller can persist it and later reconstruct the node with
/// [`spore_node_new_seeded`].
///
/// # Safety
/// `n` is a valid handle; `out` points to 32 writable bytes.
#[no_mangle]
pub unsafe extern "C" fn spore_node_seed(n: *mut Node, out: *mut u8) {
    let s = (*n).seed();
    std::ptr::copy_nonoverlapping(s.as_ptr(), out, 32);
}

/// Bytes needed for [`spore_node_prekey_ring`], so a caller can size its buffer.
///
/// # Safety
/// `n` is a valid handle.
#[no_mangle]
pub unsafe extern "C" fn spore_node_prekey_ring_len(n: *mut Node) -> usize {
    (*n).prekey_ring().len()
}

/// Write the node's prekey ring (§7) so a caller can persist it beside the seed.
///
/// **The seed is not enough.** Prekey secrets are random, not derived from the
/// seed — that is what makes deleting them mean something (S-022). A page that
/// stores only the seed comes back unable to open mail sealed to any prekey the
/// node had rotated to. Persist both.
///
/// This is secret material: every byte opens mail. Treat it like the seed.
///
/// # Safety
/// `n` is a valid handle; `out` points to at least
/// [`spore_node_prekey_ring_len`] writable bytes.
#[no_mangle]
pub unsafe extern "C" fn spore_node_prekey_ring(n: *mut Node, out: *mut u8) -> usize {
    let r = (*n).prekey_ring();
    std::ptr::copy_nonoverlapping(r.as_ptr(), out, r.len());
    r.len()
}

/// Restore a ring written by [`spore_node_prekey_ring`]. Returns 1 on success, 0
/// if the blob is malformed — in which case the node is left untouched.
///
/// # Safety
/// `n` is valid; `blob`/`len` describe readable bytes.
#[no_mangle]
pub unsafe extern "C" fn spore_node_restore_prekey_ring(n: *mut Node, blob: *const u8, len: usize) -> u32 {
    if blob.is_null() {
        return 0;
    }
    let s = std::slice::from_raw_parts(blob, len);
    u32::from((*n).restore_prekey_ring(s))
}

/// Follow a feed/topic so its traffic is delivered to us.
///
/// # Safety
/// `n` is valid; `topic`/`len` describe a UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn spore_node_subscribe(n: *mut Node, topic: *const u8, len: usize) {
    let s = String::from_utf8_lossy(std::slice::from_raw_parts(topic, len));
    (*n).subscribe(&s);
}

/// Publish an event to a feed topic (any subscriber can read it).
///
/// # Safety
/// `n` valid; `topic`/`tlen` = UTF-8 topic; `data`/`dlen` = event bytes.
#[no_mangle]
pub unsafe extern "C" fn spore_node_publish(
    n: *mut Node,
    topic: *const u8,
    tlen: usize,
    data: *const u8,
    dlen: usize,
    now: u32,
) -> i64 {
    let node = &mut *n;
    let topic_str = String::from_utf8_lossy(std::slice::from_raw_parts(topic, tlen));
    let event = std::slice::from_raw_parts(data, dlen).to_vec();
    let forwards = node.publish(&topic_str, event, now);
    pack(blob(forward_wires(forwards), Vec::new()))
}

/// Drain feed events received on subscribed topics.
///
/// Returns packed feed events. Each event is:
/// `[from_len:1][from bytes (8 or 0)][topic_len:4 BE][topic_bytes][data_len:4 BE][data_bytes]`,
/// preceded by the total count `[n:4 BE]`. `from` is the authenticated sender
/// address, or empty when the envelope carried no full source.
///
/// # Safety
/// `n` is valid.
#[no_mangle]
pub unsafe extern "C" fn spore_node_poll_feed(n: *mut Node) -> i64 {
    let events = (*n).poll_feed();
    let mut out = Vec::new();
    out.extend_from_slice(&(events.len() as u32).to_be_bytes());
    for ev in &events {
        // Sender: 1 length byte then 0 or 8 address bytes.
        match ev.from {
            Some(addr) => {
                out.push(8);
                out.extend_from_slice(&addr);
            }
            None => out.push(0),
        }
        out.extend_from_slice(&(ev.topic.len() as u32).to_be_bytes());
        out.extend_from_slice(&ev.topic);
        out.extend_from_slice(&(ev.data.len() as u32).to_be_bytes());
        out.extend_from_slice(&ev.data);
    }
    pack(out)
}

// -- the hot path ------------------------------------------------------------

/// Originate a signed message to `dest` (8 bytes; all-zero = public). Returns a
/// `{forwards, delivered}` blob (delivered is empty) packed into an i64.
///
/// # Safety
/// `n` valid; `dest` points to 8 bytes; `payload`/`plen` to the body.
#[no_mangle]
pub unsafe extern "C" fn spore_node_send(
    n: *mut Node,
    dest: *const u8,
    payload: *const u8,
    plen: usize,
    now: u32,
) -> i64 {
    let node = &mut *n;
    let mut d = [0u8; 8];
    d.copy_from_slice(std::slice::from_raw_parts(dest, 8));
    let pl = std::slice::from_raw_parts(payload, plen).to_vec();
    // An object past one fountain set returns 0, the same "nothing to send"
    // signal the rest of this ABI uses; the JS side already treats 0 as empty.
    let Ok(forwards) = node.send(d, pl, now) else { return 0 };
    pack(blob(forward_wires(forwards), Vec::new()))
}

/// Feed a received frame to the router. Returns `{forwards, delivered}` packed
/// into an i64: forwards to relay on every other transport, delivered for the app.
///
/// # Safety
/// `n` valid; `bytes`/`len` describe the frame.
#[no_mangle]
pub unsafe extern "C" fn spore_node_recv(n: *mut Node, bytes: *const u8, len: usize, now: u32) -> i64 {
    let node = &mut *n;
    let rx = node.on_rx(std::slice::from_raw_parts(bytes, len), 0, None, now);
    let delivered = rx.delivered.iter().map(|e| e.wire()).collect();
    pack(blob(forward_wires(rx.forwards), delivered))
}

// -- files (W5) ---------------------------------------------------------------

/// Publish a file from a byte slice. Returns the 16-byte magnet ID packed into an
/// i64 along with forwards.
///
/// # Safety
/// `n` valid; `name`/`nlen` = UTF-8 filename; `data`/`dlen` = file bytes.
#[no_mangle]
pub unsafe extern "C" fn spore_node_publish_file(
    n: *mut Node,
    name: *const u8,
    nlen: usize,
    data: *const u8,
    dlen: usize,
    dest: *const u8,
    now: u32,
) -> i64 {
    let node = &mut *n;
    let n_str = String::from_utf8_lossy(std::slice::from_raw_parts(name, nlen));
    let bytes = std::slice::from_raw_parts(data, dlen).to_vec();
    let mut d = [0u8; 8];
    d.copy_from_slice(std::slice::from_raw_parts(dest, 8));
    let (magnet, forwards) = node.publish_file(&n_str, bytes.as_slice(), d, now);
    let mut out = Vec::with_capacity(16 + 4);
    out.extend_from_slice(&magnet);
    // This payload has its own (big-endian) shape and JS reads only the magnet
    // from it, so it keeps bare wires rather than growing a kind byte nothing
    // would read.
    let fw: Vec<Vec<u8>> = forward_wires(forwards).into_iter().map(|(_, _, b)| b).collect();
    out.extend_from_slice(&(fw.len() as u32).to_be_bytes());
    for w in &fw {
        out.extend_from_slice(&(w.len() as u32).to_be_bytes());
        out.extend_from_slice(w);
    }
    pack(out)
}

/// Fetch a file by its 16-byte magnet ID. Returns forwards to relay.
///
/// # Safety
/// `n` valid; `magnet` points to 16 bytes.
#[no_mangle]
pub unsafe extern "C" fn spore_node_fetch_file(n: *mut Node, magnet: *const u8, _now: u32) -> i64 {
    let node = &mut *n;
    let mut id = [0u8; 16];
    id.copy_from_slice(std::slice::from_raw_parts(magnet, 16));
    let forwards = node.fetch(&id);
    pack(blob(forward_wires(forwards), Vec::new()))
}

/// Get the 16-byte magnet ID for a file we've published. Returns zeroed ID
/// (all 16 bytes zero) if the file is not found.
///
/// # Safety
/// `n` valid; `magnet` points to 16 bytes (returned from publish_file).
#[no_mangle]
pub unsafe extern "C" fn spore_node_file_bytes(n: *mut Node, magnet: *const u8) -> i64 {
    let mut id = [0u8; 16];
    id.copy_from_slice(std::slice::from_raw_parts(magnet, 16));
    match (*n).file_bytes(&id) {
        Some(bytes) => pack(bytes),
        None => 0,
    }
}

/// Get the filename for a magnet. Returns empty on not found.
///
/// # Safety
/// `n` valid; `magnet` points to 16 bytes.
#[no_mangle]
pub unsafe extern "C" fn spore_node_file_name(n: *mut Node, magnet: *const u8) -> i64 {
    let mut id = [0u8; 16];
    id.copy_from_slice(std::slice::from_raw_parts(magnet, 16));
    match (*n).file_name(&id) {
        Some(name) => pack(name.into_bytes()),
        None => 0,
    }
}

/// Peers we have heard from, freshest first. Returns packed
/// `[n:4 BE] ([addr:8] [age_secs:4 BE] [has_prekey:1] [name_len:4 BE] [name])...`
///
/// The browser had no way to enumerate peers at all, while Android's JNI has had
/// `nativePeers` since M8 — one of the concrete divergences Milestone 10 exists
/// to close. The capability itself is already in the portable core
/// (`Node::peers`, `Node::peer_name`); only this export was missing.
///
/// `name` is what the peer **claims** to be called in its ANNOUNCE. Anyone may
/// announce any name, so it is a display hint and never identity — offer it as
/// the default when the user assigns a local petname, and show it as a claim.
/// `has_prekey` is what decides whether a message to them can actually be
/// sealed, which is why it travels with the address rather than being guessed.
///
/// # Safety
/// `n` is valid.
#[no_mangle]
pub unsafe extern "C" fn spore_node_peers(n: *mut Node, now: u32) -> i64 {
    let node = &*n;
    let peers = node.peers(now);
    let mut out = Vec::new();
    out.extend_from_slice(&(peers.len() as u32).to_be_bytes());
    for (addr, age, has_prekey) in &peers {
        out.extend_from_slice(addr);
        out.extend_from_slice(&age.to_be_bytes());
        out.push(u8::from(*has_prekey));
        let name = node.peer_name(addr).unwrap_or("");
        out.extend_from_slice(&(name.len() as u32).to_be_bytes());
        out.extend_from_slice(name.as_bytes());
    }
    pack(out)
}

/// List all locally stored files. Returns packed `[n:4 BE] [name_len:4 BE] [name] [magnet:16] ...`
///
/// # Safety
/// `n` is valid.
#[no_mangle]
pub unsafe extern "C" fn spore_node_list_files(n: *mut Node) -> i64 {
    let files = (*n).complete_file_names();
    let mut out = Vec::new();
    out.extend_from_slice(&(files.len() as u32).to_be_bytes());
    for (name, id) in &files {
        out.extend_from_slice(&(name.len() as u32).to_be_bytes());
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(id);
    }
    pack(out)
}

// -- encrypted DM (W1) -------------------------------------------------------
//
// `spore_node_send` is the raw unsealed, unsigned path — the call a protocol
// implementer uses to prove a transport carries bytes. It is not what a person
// sends another person, and until now it was the only send the browser had. The
// three exports below are the sealed-and-signed path the core has had since #70,
// finally reachable from a tab.
//
// Additive, like every export here: `wasm.rs` is not in `docs/CONTRIBUTING.md`'s
// frozen list, no wire changes, and a browser that ignores these behaves exactly
// as it did.

/// Build this node's ANNOUNCE (§4) — prekey, busy byte, topics, petname — as a
/// `{forwards, delivered}` blob packed into an i64.
///
/// Required before anyone can seal to this node, and it was missing: the browser
/// could *absorb* an inbound ANNOUNCE through `spore_node_recv` and learn a
/// peer's prekey, but had no way to emit its own. The asymmetry meant a tab could
/// send sealed mail and never receive any — every peer would fall back to
/// cleartext for want of a key. Nothing in the UI would have shown it.
///
/// Also the §5.4b Trickle beacon a runtime is expected to drive on a timer; see
/// `SPEC.md`'s runtime contract, which counts beaconing as the one scheduled duty
/// that lives on the runtime's side of the transport boundary.
///
/// # Safety
/// `n` must be a handle from `spore_node_new`, not yet freed.
#[no_mangle]
pub unsafe extern "C" fn spore_node_announce(n: *mut Node, now: u32) -> i64 {
    let node = &mut *n;
    pack(blob(forward_wires(node.build_announce(now)), Vec::new()))
}

/// Send a sealed, signed DM to `dest`. Returns the `{forwards, delivered}` blob
/// (delivered empty) packed into an i64.
///
/// Sealing is automatic and layered by what the node knows about the peer: a live
/// §7 ratchet session if there is one, else a one-shot seal to their prekey, else
/// **plaintext** — because a node that has never heard the peer's ANNOUNCE has no
/// key to seal to. The third word of the result says which happened, so a UI can
/// tell the truth instead of drawing a padlock unconditionally: see
/// [`spore_node_send_direct_sealed`].
///
/// # Safety
/// `n` valid; `dest` points to 8 bytes; `payload`/`plen` to the body.
#[no_mangle]
pub unsafe extern "C" fn spore_node_send_direct(
    n: *mut Node,
    dest: *const u8,
    payload: *const u8,
    plen: usize,
    now: u32,
) -> i64 {
    let node = &mut *n;
    let mut d = [0u8; 8];
    d.copy_from_slice(std::slice::from_raw_parts(dest, 8));
    let pl = std::slice::from_raw_parts(payload, plen);
    let (id, forwards, _encrypted) = node.send_direct(d, pl, now);
    // The id rides in the `blob`'s second list — one element, 16 bytes — so a
    // UI can later ask `spore_node_acked` about *this* send. Reusing `blob`'s
    // existing two-list shape rather than inventing a new packing: the second
    // list is otherwise unused here (there is nothing "delivered" from a local
    // send), and every JS caller already knows how to unpack it.
    pack(blob(forward_wires(forwards), vec![id.to_vec()]))
}

/// Was the last [`spore_node_send_direct`] actually sealed? `1` yes, `0` no.
///
/// Split into its own call rather than packed into the send's return, because the
/// send already spends its i64 on the forwards blob. A UI **must** consult this:
/// "encrypted" is a property of what the node knew at that moment, not a setting,
/// and showing a padlock over an unsealed send is precisely the fake-UI failure
/// `DEV_GUIDE.md` forbids.
///
/// # Safety
/// `n` valid.
#[no_mangle]
pub unsafe extern "C" fn spore_node_send_direct_sealed(n: *mut Node, dest: *const u8) -> u8 {
    let node = &mut *n;
    let mut d = [0u8; 8];
    d.copy_from_slice(std::slice::from_raw_parts(dest, 8));
    // Asks the same two questions `send_direct` asks, without sending: is there a
    // ratchet session that can send, or a known prekey to seal to?
    node.can_seal_to(&d) as u8
}

/// Has a delivery receipt for this envelope id (§8) come back? `1` yes, `0` no.
///
/// The Android JNI binding has had this since the two-state "sent"/"delivered"
/// label shipped; the browser never did, so `sendDirect` had nowhere to send an
/// id and a DM's delivery state was simply unobservable from JS.
///
/// # Safety
/// `n` valid; `id` points to 16 bytes.
#[no_mangle]
pub unsafe extern "C" fn spore_node_acked(n: *mut Node, id: *const u8) -> u8 {
    let node = &*n;
    let mut i = [0u8; 16];
    i.copy_from_slice(std::slice::from_raw_parts(id, 16));
    node.acked(&i) as u8
}

/// The default lifetime (seconds) `Node` gives a locally-originated `DATA`
/// envelope — [`crate::DEFAULT_MESSAGE_EXPIRY_SECS`]. Needs no `Node`: it is a
/// build-time constant, not per-instance state.
///
/// A UI reads this once rather than hardcoding "7 days" a second time, because
/// the core has no "gave up" event for an unacknowledged send — §5.4d's resend
/// backoff exhausts in minutes, long before the envelope itself expires, and
/// silently drops its `Pending` entry either way. The only honest way to tell
/// "still travelling" from "expired, never delivered" outside the core is to
/// compare this constant against the message's own send time, which the UI
/// already has.
#[no_mangle]
pub extern "C" fn spore_default_message_expiry_secs() -> u32 {
    DEFAULT_MESSAGE_EXPIRY_SECS
}

/// Open a delivered DM from `sender`. Returns the plaintext packed into an i64,
/// or an empty blob if it does not open.
///
/// `ratcheted` comes from the envelope's `RATCHET` flag — see
/// [`spore_env_flags`]. An empty result is not an error to surface as a crash: a
/// prekey may simply have expired past the offline window, which is the honest
/// "couldn't decrypt this" case the Android app already shows.
///
/// # Safety
/// `n` valid; `sender` points to 8 bytes; `sealed`/`slen` to the payload.
#[no_mangle]
pub unsafe extern "C" fn spore_node_open_dm(
    n: *mut Node,
    sender: *const u8,
    sealed: *const u8,
    slen: usize,
    ratcheted: u32,
    now: u32,
) -> i64 {
    let node = &mut *n;
    let mut s = [0u8; 8];
    s.copy_from_slice(std::slice::from_raw_parts(sender, 8));
    let body = std::slice::from_raw_parts(sealed, slen);
    pack(node.open_dm(s, body, ratcheted != 0, now).unwrap_or_default())
}

// -- envelope helpers for the app --------------------------------------------

/// Extract an envelope's payload, packed into an i64 (empty if it doesn't decode).
///
/// # Safety
/// `ptr`/`len` describe envelope bytes.
#[no_mangle]
pub unsafe extern "C" fn spore_env_payload(ptr: *const u8, len: usize) -> i64 {
    match Envelope::decode(std::slice::from_raw_parts(ptr, len)) {
        Ok((e, _)) => pack(e.payload),
        Err(_) => pack(Vec::new()),
    }
}

/// Verify an envelope's signature (1 = valid).
///
/// # Safety
/// `ptr`/`len` describe envelope bytes.
#[no_mangle]
pub unsafe extern "C" fn spore_env_verify(ptr: *const u8, len: usize) -> u8 {
    match Envelope::decode(std::slice::from_raw_parts(ptr, len)) {
        Ok((e, _)) => e.verify() as u8,
        Err(_) => 0,
    }
}

/// An envelope's `flags` byte, or 0 if it does not decode.
///
/// A DM thread needs two bits of it: `ENCRYPTED` says the payload is sealed
/// rather than plain, and `RATCHET` says which of the two schemes opens it —
/// the `ratcheted` argument to [`spore_node_open_dm`].
///
/// # Safety
/// `ptr`/`len` describe envelope bytes.
#[no_mangle]
pub unsafe extern "C" fn spore_env_flags(ptr: *const u8, len: usize) -> u8 {
    match Envelope::decode(std::slice::from_raw_parts(ptr, len)) {
        Ok((e, _)) => e.flags,
        Err(_) => 0,
    }
}

/// The **authenticated** sender address, packed into an i64 — 8 bytes, or empty.
///
/// Empty for an unsigned envelope, for one whose signature does not verify, and
/// for `SRC8` (which carries an address the envelope cannot prove). Deriving the
/// address from the public key here, behind a `verify`, is what keeps a thread
/// list from being spoofable: the alternative is JS trusting a field, and a
/// signed envelope proving its own sender is the property the whole protocol
/// rests on.
///
/// # Safety
/// `ptr`/`len` describe envelope bytes.
#[no_mangle]
pub unsafe extern "C" fn spore_env_src(ptr: *const u8, len: usize) -> i64 {
    let Ok((e, _)) = Envelope::decode(std::slice::from_raw_parts(ptr, len)) else {
        return pack(Vec::new());
    };
    match e.src {
        crate::Src::Full(pk) if e.verify() => pack(crate::addr_of(&pk).to_vec()),
        _ => pack(Vec::new()),
    }
}

/// Seal a message under a 32-byte topic key (XChaCha20-Poly1305).
#[no_mangle]
pub unsafe extern "C" fn spore_topic_seal(msg: *const u8, msg_len: usize, psk: *const u8) -> i64 {
    let psk_bytes = std::slice::from_raw_parts(psk, 32);
    let psk_arr: &[u8; 32] = match psk_bytes.try_into() {
        Ok(a) => a,
        Err(_) => return pack(Vec::new()),
    };
    let ct = crate::topic_seal(std::slice::from_raw_parts(msg, msg_len), psk_arr);
    pack(ct)
}

/// Open a topic-sealed payload with the 32-byte key. Null on failure.
#[no_mangle]
pub unsafe extern "C" fn spore_topic_open(ct: *const u8, ct_len: usize, psk: *const u8) -> i64 {
    let psk_bytes = std::slice::from_raw_parts(psk, 32);
    let psk_arr: &[u8; 32] = match psk_bytes.try_into() {
        Ok(a) => a,
        Err(_) => return pack(Vec::new()),
    };
    match crate::topic_open(std::slice::from_raw_parts(ct, ct_len), psk_arr) {
        Some(pt) => pack(pt),
        None => pack(Vec::new()),
    }
}

/// Render a private-group invite (W7) — `spore-group:<key hex>?n=…&k=…`.
///
/// Exported rather than formatted in JavaScript so the string the browser hands
/// a person and the string the core will accept are produced by the same code.
/// A second implementation in the UI is a format that drifts, and the failure
/// mode of a drifted invite is a room with one member in it.
///
/// # Safety
/// `name`/`name_len` are UTF-8; `key` points to 32 bytes.
#[no_mangle]
pub unsafe extern "C" fn spore_group_invite_encode(name: *const u8, name_len: usize, key: *const u8) -> i64 {
    let key_arr: &[u8; 32] = match std::slice::from_raw_parts(key, 32).try_into() {
        Ok(a) => a,
        Err(_) => return pack(Vec::new()),
    };
    let Ok(name) = std::str::from_utf8(std::slice::from_raw_parts(name, name_len)) else {
        return pack(Vec::new());
    };
    pack(crate::invite::encode_group(name, key_arr).into_bytes())
}

/// Parse a private-group invite. Returns `[key:32][name utf8]`, or empty if the
/// text is not a group invite, is malformed, or fails its checksum — the
/// caller cannot tell those apart on purpose, because the honest UI response to
/// all three is the same: this invite does not work, ask for it again.
///
/// # Safety
/// `ptr`/`len` describe UTF-8 text.
#[no_mangle]
pub unsafe extern "C" fn spore_group_invite_decode(ptr: *const u8, len: usize) -> i64 {
    let Ok(text) = std::str::from_utf8(std::slice::from_raw_parts(ptr, len)) else {
        return pack(Vec::new());
    };
    match crate::invite::decode_group(text) {
        Some(g) => {
            let mut out = Vec::with_capacity(32 + g.name.len());
            out.extend_from_slice(&g.key);
            out.extend_from_slice(g.name.as_bytes());
            pack(out)
        }
        None => pack(Vec::new()),
    }
}
