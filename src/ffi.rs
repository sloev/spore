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
    if !b.data.is_null() {
        let s = std::slice::from_raw_parts_mut(b.data, b.len);
        drop(Box::from_raw(s as *mut [u8]));
    }
}

// -- small helpers -----------------------------------------------------------

unsafe fn slice<'a>(ptr: *const u8, len: usize) -> &'a [u8] {
    if ptr.is_null() {
        &[]
    } else {
        std::slice::from_raw_parts(ptr, len)
    }
}
unsafe fn arr32(ptr: *const u8) -> [u8; 32] {
    let mut a = [0u8; 32];
    a.copy_from_slice(slice(ptr, 32));
    a
}
unsafe fn arr8(ptr: *const u8) -> Addr {
    let mut a = [0u8; 8];
    a.copy_from_slice(slice(ptr, 8));
    a
}

// -- identity ----------------------------------------------------------------

/// Generate an Ed25519 signing keypair: writes 32-byte secret and 32-byte public.
///
/// # Safety
/// `out_sk` and `out_pk` must each point to 32 writable bytes.
#[no_mangle]
pub unsafe extern "C" fn spore_keypair(out_sk: *mut u8, out_pk: *mut u8) {
    let mut seed = [0u8; 32];
    OsRng.fill_bytes(&mut seed);
    let sk = SigningKey::from_bytes(&seed);
    let pk = sk.verifying_key().to_bytes();
    std::ptr::copy_nonoverlapping(seed.as_ptr(), out_sk, 32);
    std::ptr::copy_nonoverlapping(pk.as_ptr(), out_pk, 32);
}

/// Generate an X25519 encryption prekey pair: writes 32-byte secret and public.
///
/// # Safety
/// `out_sec` and `out_pub` must each point to 32 writable bytes.
#[no_mangle]
pub unsafe extern "C" fn spore_prekey(out_sec: *mut u8, out_pub: *mut u8) {
    let (sec, pubk) = prekey_keypair();
    std::ptr::copy_nonoverlapping(sec.as_ptr(), out_sec, 32);
    std::ptr::copy_nonoverlapping(pubk.as_ptr(), out_pub, 32);
}

/// SPORE address = SHA-256(pubkey)[..8]. Writes 8 bytes.
///
/// # Safety
/// `pk` points to 32 bytes, `out` to 8 writable bytes.
#[no_mangle]
pub unsafe extern "C" fn spore_addr_of(pk: *const u8, out: *mut u8) {
    let a = addr_of(&arr32(pk));
    std::ptr::copy_nonoverlapping(a.as_ptr(), out, 8);
}

/// Topic address = SHA-256(utf8)[..8]. Writes 8 bytes.
///
/// # Safety
/// `s`/`s_len` describe a byte string; `out` points to 8 writable bytes.
#[no_mangle]
pub unsafe extern "C" fn spore_topic_of(s: *const u8, s_len: usize, out: *mut u8) {
    let name = String::from_utf8_lossy(slice(s, s_len));
    let a = topic_of(&name);
    std::ptr::copy_nonoverlapping(a.as_ptr(), out, 8);
}

// -- sealing / encrypted topics ----------------------------------------------

/// Anonymous sealed box to a recipient prekey (public, 32 B). Returns
/// `ephemeral_pubkey(32) ‖ ciphertext`.
///
/// # Safety
/// `msg`/`msg_len` describe the plaintext; `recip_prekey` points to 32 bytes.
#[no_mangle]
pub unsafe extern "C" fn spore_seal(msg: *const u8, msg_len: usize, recip_prekey: *const u8) -> SporeBytes {
    SporeBytes::from_vec(seal(slice(msg, msg_len), &arr32(recip_prekey)))
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
    SporeBytes::or_null(open_sealed(slice(sealed, sealed_len), &arr32(prekey_sec)))
}

/// Encrypt for a topic under a 32-byte pre-shared key (XChaCha20-Poly1305).
///
/// # Safety
/// `msg`/`msg_len` describe the plaintext; `psk` points to 32 bytes.
#[no_mangle]
pub unsafe extern "C" fn spore_topic_seal(msg: *const u8, msg_len: usize, psk: *const u8) -> SporeBytes {
    SporeBytes::from_vec(topic_seal(slice(msg, msg_len), &arr32(psk)))
}

/// Decrypt a topic payload with the pre-shared key. Null on failure.
///
/// # Safety
/// `ct`/`ct_len` describe the ciphertext; `psk` points to 32 bytes.
#[no_mangle]
pub unsafe extern "C" fn spore_topic_open(ct: *const u8, ct_len: usize, psk: *const u8) -> SporeBytes {
    SporeBytes::or_null(topic_open(slice(ct, ct_len), &arr32(psk)))
}

// -- text armor --------------------------------------------------------------

/// Armor envelope bytes into `~S1.…~` text (UTF-8).
///
/// # Safety
/// `env`/`env_len` describe the envelope bytes.
#[no_mangle]
pub unsafe extern "C" fn spore_armor_wrap(env: *const u8, env_len: usize) -> SporeBytes {
    SporeBytes::from_vec(armor::wrap(slice(env, env_len)).into_bytes())
}

/// Recover envelope bytes from armor found anywhere in `text`. Null if none.
///
/// # Safety
/// `text`/`text_len` describe a UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn spore_armor_unwrap(text: *const u8, text_len: usize) -> SporeBytes {
    let s = String::from_utf8_lossy(slice(text, text_len));
    SporeBytes::or_null(armor::unwrap(&s))
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
    let sk = SigningKey::from_bytes(&arr32(sk));
    let mut e = Envelope::new(ty::DATA, arr8(dest), expiry, slice(payload, payload_len).to_vec());
    e.sign(&sk);
    SporeBytes::from_vec(e.wire())
}

/// Verify a signed envelope's signature. Returns 1 if valid, else 0.
///
/// # Safety
/// `wire`/`wire_len` describe the envelope bytes.
#[no_mangle]
pub unsafe extern "C" fn spore_message_verify(wire: *const u8, wire_len: usize) -> u8 {
    match Envelope::decode(slice(wire, wire_len)) {
        Ok((e, _)) => e.verify() as u8,
        Err(_) => 0,
    }
}

/// Write an envelope's 16-byte content ID to `out`. Returns 1 on success, 0 if
/// the bytes don't decode.
///
/// # Safety
/// `wire`/`wire_len` describe the envelope; `out` points to 16 writable bytes.
#[no_mangle]
pub unsafe extern "C" fn spore_message_id(wire: *const u8, wire_len: usize, out: *mut u8) -> u8 {
    match Envelope::decode(slice(wire, wire_len)) {
        Ok((e, _)) => {
            let id = e.id();
            std::ptr::copy_nonoverlapping(id.as_ptr(), out, 16);
            1
        }
        Err(_) => 0,
    }
}

/// Extract an envelope's payload. Null if the bytes don't decode.
///
/// # Safety
/// `wire`/`wire_len` describe the envelope bytes.
#[no_mangle]
pub unsafe extern "C" fn spore_message_payload(wire: *const u8, wire_len: usize) -> SporeBytes {
    match Envelope::decode(slice(wire, wire_len)) {
        Ok((e, _)) => SporeBytes::from_vec(e.payload),
        Err(_) => SporeBytes::null(),
    }
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
}
