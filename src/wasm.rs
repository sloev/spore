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

fn blob(forwards: Vec<Vec<u8>>, delivered: Vec<Vec<u8>>) -> Vec<u8> {
    let mut o = Vec::new();
    let mut put = |list: &[Vec<u8>]| {
        o.extend_from_slice(&(list.len() as u32).to_le_bytes());
        for item in list {
            o.extend_from_slice(&(item.len() as u32).to_le_bytes());
            o.extend_from_slice(item);
        }
    };
    put(&forwards);
    put(&delivered);
    o
}

fn forward_wires(fs: Vec<Forward>) -> Vec<Vec<u8>> {
    fs.into_iter()
        .map(|f| match f {
            Forward::Flood { bytes, .. } => bytes,
            Forward::Directed { bytes, .. } => bytes,
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

/// Follow a feed/topic so its traffic is delivered to us.
///
/// # Safety
/// `n` is valid; `topic`/`len` describe a UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn spore_node_subscribe(n: *mut Node, topic: *const u8, len: usize) {
    let s = String::from_utf8_lossy(std::slice::from_raw_parts(topic, len));
    (*n).subscribe(&s);
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
    let forwards = node.send(d, pl, now);
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
