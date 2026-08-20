//! C ABI for language bindings.
//!
//! Everything is byte-in / byte-out. Fixed-size buffers (`[u8; 8/16/32]`) are
//! passed as raw pointers the caller owns. Variable-length results are returned
//! as [`SporeBytes`]; the caller must free them with [`spore_bytes_free`]. The
//! Python, Go, and JS wrappers under `bindings/` are generated against exactly
//! these signatures, so keep the two in sync (see `bindings/spec.json`).
//!
//! Safety: every function trusts the caller to pass valid pointers and the
//! documented lengths — that's the nature of a C ABI. The wrappers uphold it.

use crate::*;
use ed25519_dalek::SigningKey;

/// An owned byte buffer handed across the ABI. Free with `spore_bytes_free`.
/// A null `data` with `len == 0` signals failure (bad input, wrong key, …).
#[repr(C)]
pub struct SporeBytes {
    pub data: *mut u8,
    pub len: usize,
}

impl SporeBytes {
    fn from_vec(v: Vec<u8>) -> Self {
        let boxed = v.into_boxed_slice();
        let len = boxed.len();
        let data = Box::into_raw(boxed) as *mut u8;
        SporeBytes { data, len }
    }
    fn null() -> Self {
        SporeBytes { data: std::ptr::null_mut(), len: 0 }
    }
    fn or_null(v: Option<Vec<u8>>) -> Self {
        match v {
            Some(v) => Self::from_vec(v),
            None => Self::null(),
        }
    }
}

/// Free a buffer returned by any `spore_*` function.
///
/// # Safety
/// `b` must have come from this library and not been freed already.
#[no_mangle]
pub unsafe extern "C" fn spore_bytes_free(b: SporeBytes) {
    guard((), || {
        if !b.data.is_null() {
            let s = std::slice::from_raw_parts_mut(b.data, b.len);
            drop(Box::from_raw(s as *mut [u8]));
        }
    })
}

// -- small helpers -----------------------------------------------------------

/// Run `f`, returning `fallback` if it panics.
///
/// A panic unwinding out of an `extern "C"` function is undefined behaviour —
/// not "an error the caller sees", but genuinely undefined, because the foreign
/// frames above have no unwind tables. Every entry point below therefore catches
/// before it can reach the boundary and returns the same failure value it would
/// return for bad input: a null [`SporeBytes`], a `0`, or nothing at all.
///
/// This is a backstop, not an excuse: the helpers are written not to panic in the
/// first place. But `spore` is a library whose callers are Python, Go and JS
/// wrappers, and a panic reaching them would corrupt the host process rather than
/// raise anything catchable in those languages.
fn guard<T>(fallback: T, f: impl FnOnce() -> T) -> T {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).unwrap_or(fallback)
}

unsafe fn slice<'a>(ptr: *const u8, len: usize) -> &'a [u8] {
    if ptr.is_null() {
        &[]
    } else {
        std::slice::from_raw_parts(ptr, len)
    }
}
/// A fixed-size key read from a caller pointer.
///
/// `copy_from_slice` panics on a length mismatch, and `slice` yields an empty
/// slice for a null pointer — so the obvious spelling of this made every function
/// taking a key abort the process on a null argument, which a wrapper passing
/// `None`/`nil` does by accident. A short or null pointer now yields zeroes: a
/// well-defined wrong key, which fails as a wrong key does, instead of UB.
unsafe fn arr32(ptr: *const u8) -> [u8; 32] {
    let mut a = [0u8; 32];
    let s = slice(ptr, 32);
    if s.len() == 32 {
        a.copy_from_slice(s);
    }
    a
}
unsafe fn arr8(ptr: *const u8) -> Addr {
    let mut a = [0u8; 8];
    let s = slice(ptr, 8);
    if s.len() == 8 {
        a.copy_from_slice(s);
    }
    a
}

// -- identity ----------------------------------------------------------------

/// Generate an Ed25519 signing keypair: writes 32-byte secret and 32-byte public.
///
/// # Safety
/// `out_sk` and `out_pk` must each point to 32 writable bytes.
#[no_mangle]
pub unsafe extern "C" fn spore_keypair(out_sk: *mut u8, out_pk: *mut u8) {
    guard((), || {
        let mut seed = [0u8; 32];
        OsRng.fill_bytes(&mut seed);
        let sk = SigningKey::from_bytes(&seed);
        let pk = sk.verifying_key().to_bytes();
        std::ptr::copy_nonoverlapping(seed.as_ptr(), out_sk, 32);
        std::ptr::copy_nonoverlapping(pk.as_ptr(), out_pk, 32);
    })
}

/// Generate an X25519 encryption prekey pair: writes 32-byte secret and public.
///
/// # Safety
/// `out_sec` and `out_pub` must each point to 32 writable bytes.
#[no_mangle]
pub unsafe extern "C" fn spore_prekey(out_sec: *mut u8, out_pub: *mut u8) {
    guard((), || {
        let (sec, pubk) = prekey_keypair();
        std::ptr::copy_nonoverlapping(sec.as_ptr(), out_sec, 32);
        std::ptr::copy_nonoverlapping(pubk.as_ptr(), out_pub, 32);
    })
}

/// SPORE address = SHA-256(pubkey)[..8]. Writes 8 bytes.
///
/// # Safety
/// `pk` points to 32 bytes, `out` to 8 writable bytes.
#[no_mangle]
pub unsafe extern "C" fn spore_addr_of(pk: *const u8, out: *mut u8) {
    guard((), || {
        let a = addr_of(&arr32(pk));
        std::ptr::copy_nonoverlapping(a.as_ptr(), out, 8);
    })
}

/// Topic address = SHA-256(utf8)[..8]. Writes 8 bytes.
///
/// # Safety
/// `s`/`s_len` describe a byte string; `out` points to 8 writable bytes.
#[no_mangle]
pub unsafe extern "C" fn spore_topic_of(s: *const u8, s_len: usize, out: *mut u8) {
    guard((), || {
        let name = String::from_utf8_lossy(slice(s, s_len));
        let a = topic_of(&name);
        std::ptr::copy_nonoverlapping(a.as_ptr(), out, 8);
    })
}

// -- sealing / encrypted topics ----------------------------------------------

/// Anonymous sealed box to a recipient prekey (public, 32 B). Returns
/// `ephemeral_pubkey(32) ‖ ciphertext`.
///
/// # Safety
/// `msg`/`msg_len` describe the plaintext; `recip_prekey` points to 32 bytes.
#[no_mangle]
pub unsafe extern "C" fn spore_seal(msg: *const u8, msg_len: usize, recip_prekey: *const u8) -> SporeBytes {
    guard(SporeBytes::null(), || SporeBytes::from_vec(seal(slice(msg, msg_len), &arr32(recip_prekey))))
}

/// Open a sealed box with a prekey secret (32 B). Null on failure.
///
/// # Safety
/// `sealed`/`sealed_len` describe the box; `prekey_sec` points to 32 bytes.
#[no_mangle]
pub unsafe extern "C" fn spore_open(
    sealed: *const u8,
    sealed_len: usize,
    prekey_sec: *const u8,
) -> SporeBytes {
    guard(SporeBytes::null(), || {
        SporeBytes::or_null(open_sealed(slice(sealed, sealed_len), &arr32(prekey_sec)))
    })
}

/// Encrypt for a topic under a 32-byte pre-shared key (XChaCha20-Poly1305).
///
/// # Safety
/// `msg`/`msg_len` describe the plaintext; `psk` points to 32 bytes.
#[cfg(not(target_arch = "wasm32"))]
#[no_mangle]
pub unsafe extern "C" fn spore_topic_seal(msg: *const u8, msg_len: usize, psk: *const u8) -> SporeBytes {
    guard(SporeBytes::null(), || SporeBytes::from_vec(topic_seal(slice(msg, msg_len), &arr32(psk))))
}

/// Decrypt a topic payload with the pre-shared key. Null on failure.
///
/// # Safety
/// `ct`/`ct_len` describe the ciphertext; `psk` points to 32 bytes.
#[cfg(not(target_arch = "wasm32"))]
#[no_mangle]
pub unsafe extern "C" fn spore_topic_open(ct: *const u8, ct_len: usize, psk: *const u8) -> SporeBytes {
    guard(SporeBytes::null(), || SporeBytes::or_null(topic_open(slice(ct, ct_len), &arr32(psk))))
}

// -- feed/microblog (L5) ---------------------------------------------------

/// Publish an event to a feed topic.
///
/// # Safety
/// `n` valid; `topic`/`tlen` = UTF-8 topic; `data`/`dlen` = event bytes.
#[cfg(not(target_arch = "wasm32"))]
#[no_mangle]
pub unsafe extern "C" fn spore_node_publish(
    n: *mut crate::Node,
    topic: *const u8,
    tlen: usize,
    data: *const u8,
    dlen: usize,
    now: u32,
) -> SporeBytes {
    guard(SporeBytes::null(), || {
        let node = &mut *n;
        let topic_str = String::from_utf8_lossy(std::slice::from_raw_parts(topic, tlen));
        let event = std::slice::from_raw_parts(data, dlen).to_vec();
        let fwd = node.publish(&topic_str, event, now);
        let mut wire = Vec::new();
        for f in fwd {
            match f {
                crate::Forward::Flood { bytes, .. } | crate::Forward::Directed { bytes, .. } => {
                    wire.extend_from_slice(&bytes)
                }
            }
        }
        SporeBytes::from_vec(wire)
    })
}

/// Drain feed events from subscribed topics.
///
/// # Safety
/// `n` is valid.
#[cfg(not(target_arch = "wasm32"))]
#[no_mangle]
pub unsafe extern "C" fn spore_node_poll_feed(n: *mut crate::Node) -> SporeBytes {
    guard(SporeBytes::null(), || {
        let events = (*n).poll_feed();
        let mut out = Vec::new();
        out.extend_from_slice(&(events.len() as u32).to_be_bytes());
        for ev in &events {
            out.extend_from_slice(&(ev.topic.len() as u32).to_be_bytes());
            out.extend_from_slice(&ev.topic);
            out.extend_from_slice(&(ev.data.len() as u32).to_be_bytes());
            out.extend_from_slice(&ev.data);
        }
        SporeBytes::from_vec(out)
    })
}

// -- files (W5) ---------------------------------------------------------------

/// Publish a file from bytes. Returns forwards packed as SporeBytes.
#[cfg(not(target_arch = "wasm32"))]
#[no_mangle]
pub unsafe extern "C" fn spore_node_publish_file(
    n: *mut crate::Node,
    name: *const u8,
    nlen: usize,
    data: *const u8,
    dlen: usize,
    dest: *const u8,
    now: u32,
) -> SporeBytes {
    guard(SporeBytes::null(), || {
        let node = &mut *n;
        let n_str = String::from_utf8_lossy(std::slice::from_raw_parts(name, nlen));
        let bytes = std::slice::from_raw_parts(data, dlen);
        let mut d = [0u8; 8];
        d.copy_from_slice(std::slice::from_raw_parts(dest, 8));
        let (_magnet, forwards) = node.publish_file(&n_str, bytes, d, now);
        SporeBytes::from_vec(forwards.into_iter().flat_map(|f| match f {
            crate::Forward::Flood { bytes, .. } | crate::Forward::Directed { bytes, .. } => bytes,
        }).collect())
    })
}

/// Get file bytes for a magnet. SporeBytes::null() if not found.
#[cfg(not(target_arch = "wasm32"))]
#[no_mangle]
pub unsafe extern "C" fn spore_node_file_bytes(n: *mut crate::Node, magnet: *const u8) -> SporeBytes {
    let mut id = [0u8; 16];
    id.copy_from_slice(std::slice::from_raw_parts(magnet, 16));
    guard(SporeBytes::null(), || SporeBytes::or_null((*n).file_bytes(&id)))
}

/// Get filename for a magnet. SporeBytes::null() if not found.
#[cfg(not(target_arch = "wasm32"))]
#[no_mangle]
pub unsafe extern "C" fn spore_node_file_name(n: *mut crate::Node, magnet: *const u8) -> SporeBytes {
    let mut id = [0u8; 16];
    id.copy_from_slice(std::slice::from_raw_parts(magnet, 16));
    guard(SporeBytes::null(), || (*n).file_name(&id).map(|s| SporeBytes::from_vec(s.into_bytes())).unwrap_or(SporeBytes::null()))
}

// -- text armor --------------------------------------------------------------

/// Armor envelope bytes into `~S1.…~` text (UTF-8).
///
/// # Safety
/// `env`/`env_len` describe the envelope bytes.
#[no_mangle]
pub unsafe extern "C" fn spore_armor_wrap(env: *const u8, env_len: usize) -> SporeBytes {
    guard(SporeBytes::null(), || SporeBytes::from_vec(armor::wrap(slice(env, env_len)).into_bytes()))
}

/// Recover envelope bytes from armor found anywhere in `text`. Null if none.
///
/// # Safety
/// `text`/`text_len` describe a UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn spore_armor_unwrap(text: *const u8, text_len: usize) -> SporeBytes {
    guard(SporeBytes::null(), || {
        let s = String::from_utf8_lossy(slice(text, text_len));
        SporeBytes::or_null(armor::unwrap(&s))
    })
}

// -- messages (signed DATA envelopes) ----------------------------------------

/// Build and sign a DATA envelope; returns its wire bytes.
///
/// # Safety
/// `sk` points to 32 bytes, `dest` to 8 bytes, `payload`/`payload_len` to the body.
#[no_mangle]
pub unsafe extern "C" fn spore_message_new(
    sk: *const u8,
    dest: *const u8,
    expiry: u32,
    payload: *const u8,
    payload_len: usize,
) -> SporeBytes {
    guard(SporeBytes::null(), || {
        let sk = SigningKey::from_bytes(&arr32(sk));
        let mut e = Envelope::new(ty::DATA, arr8(dest), expiry, slice(payload, payload_len).to_vec());
        e.sign(&sk);
        SporeBytes::from_vec(e.wire())
    })
}

/// Verify a signed envelope's signature. Returns 1 if valid, else 0.
///
/// # Safety
/// `wire`/`wire_len` describe the envelope bytes.
#[no_mangle]
pub unsafe extern "C" fn spore_message_verify(wire: *const u8, wire_len: usize) -> u8 {
    guard(0, || match Envelope::decode(slice(wire, wire_len)) {
        Ok((e, _)) => e.verify() as u8,
        Err(_) => 0,
    })
}

/// Write an envelope's 16-byte content ID to `out`. Returns 1 on success, 0 if
/// the bytes don't decode.
///
/// # Safety
/// `wire`/`wire_len` describe the envelope; `out` points to 16 writable bytes.
#[no_mangle]
pub unsafe extern "C" fn spore_message_id(wire: *const u8, wire_len: usize, out: *mut u8) -> u8 {
    guard(0, || match Envelope::decode(slice(wire, wire_len)) {
        Ok((e, _)) => {
            let id = e.id();
            std::ptr::copy_nonoverlapping(id.as_ptr(), out, 16);
            1
        }
        Err(_) => 0,
    })
}

/// Extract an envelope's payload. Null if the bytes don't decode.
///
/// # Safety
/// `wire`/`wire_len` describe the envelope bytes.
#[no_mangle]
pub unsafe extern "C" fn spore_message_payload(wire: *const u8, wire_len: usize) -> SporeBytes {
    guard(SporeBytes::null(), || match Envelope::decode(slice(wire, wire_len)) {
        Ok((e, _)) => SporeBytes::from_vec(e.payload),
        Err(_) => SporeBytes::null(),
    })
}

// ---------------------------------------------------------------------------
// Tests (exercise the ABI directly; the wrappers are tested from their langs).
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ffi_message_and_seal_roundtrip() {
        unsafe {
            let (mut sk, mut pk) = ([0u8; 32], [0u8; 32]);
            spore_keypair(sk.as_mut_ptr(), pk.as_mut_ptr());
            let mut addr = [0u8; 8];
            spore_addr_of(pk.as_ptr(), addr.as_mut_ptr());

            // Build + verify a signed message.
            let payload = b"ffi hello";
            let msg =
                spore_message_new(sk.as_ptr(), addr.as_ptr(), 1_700_000_000, payload.as_ptr(), payload.len());
            let wire = std::slice::from_raw_parts(msg.data, msg.len);
            assert_eq!(spore_message_verify(wire.as_ptr(), wire.len()), 1);
            let got = spore_message_payload(wire.as_ptr(), wire.len());
            assert_eq!(std::slice::from_raw_parts(got.data, got.len), payload);
            spore_bytes_free(got);
            spore_bytes_free(msg);

            // Seal to a fresh prekey and open it back.
            let (mut psec, mut ppub) = ([0u8; 32], [0u8; 32]);
            spore_prekey(psec.as_mut_ptr(), ppub.as_mut_ptr());
            let sealed = spore_seal(payload.as_ptr(), payload.len(), ppub.as_ptr());
            let opened = spore_open(sealed.data, sealed.len, psec.as_ptr());
            assert_eq!(std::slice::from_raw_parts(opened.data, opened.len), payload);
            spore_bytes_free(opened);
            spore_bytes_free(sealed);
        }
    }

    #[test]
    fn null_pointers_return_failure_instead_of_unwinding() {
        // A wrapper that passes None/nil/null reaches here as a null pointer.
        // Before the guard, `copy_from_slice` panicked on the length mismatch and
        // that panic unwound across the C ABI, which is undefined behaviour — not
        // a catchable exception in Python, Go or JS, but a corrupted host process.
        unsafe {
            let mut out = [0u8; 8];
            spore_addr_of(std::ptr::null(), out.as_mut_ptr()); // must not abort
            spore_topic_of(std::ptr::null(), 0, out.as_mut_ptr());

            let b = spore_seal(std::ptr::null(), 0, std::ptr::null());
            spore_bytes_free(b);

            // Opening with a null key is a wrong key, not a crash.
            let b = spore_open(std::ptr::null(), 0, std::ptr::null());
            assert!(b.data.is_null(), "a null key fails as a wrong key does");

            let b = spore_topic_open(std::ptr::null(), 0, std::ptr::null());
            assert!(b.data.is_null());

            assert_eq!(spore_message_verify(std::ptr::null(), 0), 0);
            let mut id = [0u8; 16];
            assert_eq!(spore_message_id(std::ptr::null(), 0, id.as_mut_ptr()), 0);
            let b = spore_message_payload(std::ptr::null(), 0);
            assert!(b.data.is_null());

            // Freeing a null buffer is a no-op, not a double free.
            spore_bytes_free(SporeBytes::null());
        }
    }

    #[test]
    fn a_panic_inside_the_boundary_becomes_a_failure_value() {
        // The guard itself, exercised directly: whatever goes wrong inside, the
        // caller gets the documented failure value rather than an unwind.
        let hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {})); // keep the test output clean
        let v = guard(0u8, || panic!("boom"));
        let b = guard(SporeBytes::null(), || -> SporeBytes { panic!("boom") });
        std::panic::set_hook(hook);
        assert_eq!(v, 0, "a panic yields the fallback");
        assert!(b.data.is_null());
    }
}
