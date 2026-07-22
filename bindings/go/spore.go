// Auto-generated SPORE bindings (cgo) — do not edit; run bindings/generate.py.
//
// Build/run needs libspore on the linker + loader path, e.g.:
//   LD_LIBRARY_PATH=../../target/release go test ./...
package spore

/*
#cgo CFLAGS: -I${SRCDIR}/..
#cgo LDFLAGS: -L${SRCDIR}/../../target/release -lspore
#include <stdlib.h>
#include "spore.h"
*/
import "C"
import "unsafe"

func take(b C.SporeBytes) []byte {
	if b.data == nil {
		return nil
	}
	out := C.GoBytes(unsafe.Pointer(b.data), C.int(b.len))
	C.spore_bytes_free(b)
	return out
}

func ptr(b []byte) *C.uint8_t {
	if len(b) == 0 {
		return nil
	}
	return (*C.uint8_t)(unsafe.Pointer(&b[0]))
}


// Keypair — Generate an Ed25519 signing keypair (secret, public).
func Keypair() ([]byte, []byte) {
	var out_sk [32]byte
	var out_pk [32]byte
	C.spore_keypair((*C.uint8_t)(unsafe.Pointer(&out_sk[0])), (*C.uint8_t)(unsafe.Pointer(&out_pk[0])))
	return append([]byte(nil), out_sk[:]...), append([]byte(nil), out_pk[:]...)
}

// Prekey — Generate an X25519 encryption prekey pair (secret, public).
func Prekey() ([]byte, []byte) {
	var out_sec [32]byte
	var out_pub [32]byte
	C.spore_prekey((*C.uint8_t)(unsafe.Pointer(&out_sec[0])), (*C.uint8_t)(unsafe.Pointer(&out_pub[0])))
	return append([]byte(nil), out_sec[:]...), append([]byte(nil), out_pub[:]...)
}

// AddrOf — SPORE address = SHA-256(pubkey)[..8].
func AddrOf(pk []byte) []byte {
	var out [8]byte
	C.spore_addr_of(ptr(pk), (*C.uint8_t)(unsafe.Pointer(&out[0])))
	return append([]byte(nil), out[:]...)
}

// TopicOf — Topic address = SHA-256(utf8)[..8].
func TopicOf(s []byte) []byte {
	var out [8]byte
	C.spore_topic_of(ptr(s), C.size_t(len(s)), (*C.uint8_t)(unsafe.Pointer(&out[0])))
	return append([]byte(nil), out[:]...)
}

// Seal — Anonymous sealed box to a recipient prekey.
func Seal(msg []byte, recip_prekey []byte) []byte {
	return take(C.spore_seal(ptr(msg), C.size_t(len(msg)), ptr(recip_prekey)))
}

// Open — Open a sealed box with a prekey secret (None on failure).
func Open(sealed []byte, prekey_sec []byte) []byte {
	return take(C.spore_open(ptr(sealed), C.size_t(len(sealed)), ptr(prekey_sec)))
}

// TopicSeal — Encrypt for a topic under a 32-byte pre-shared key.
func TopicSeal(msg []byte, psk []byte) []byte {
	return take(C.spore_topic_seal(ptr(msg), C.size_t(len(msg)), ptr(psk)))
}

// TopicOpen — Decrypt a topic payload (None on failure).
func TopicOpen(ct []byte, psk []byte) []byte {
	return take(C.spore_topic_open(ptr(ct), C.size_t(len(ct)), ptr(psk)))
}

// ArmorWrap — Armor envelope bytes into ~S1.…~ text.
func ArmorWrap(env []byte) []byte {
	return take(C.spore_armor_wrap(ptr(env), C.size_t(len(env))))
}

// ArmorUnwrap — Recover envelope bytes from armor (None if absent).
func ArmorUnwrap(text []byte) []byte {
	return take(C.spore_armor_unwrap(ptr(text), C.size_t(len(text))))
}

// MessageNew — Build and sign a DATA envelope; returns wire bytes.
func MessageNew(sk []byte, dest []byte, expiry uint32, payload []byte) []byte {
	return take(C.spore_message_new(ptr(sk), ptr(dest), C.uint32_t(expiry), ptr(payload), C.size_t(len(payload))))
}

// MessageVerify — Verify a signed envelope's signature.
func MessageVerify(wire []byte) bool {
	return C.spore_message_verify(ptr(wire), C.size_t(len(wire))) != 0
}

// MessageId — Envelope 16-byte content ID (None if it doesn't decode).
func MessageId(wire []byte) []byte {
	var out [16]byte
	ok := C.spore_message_id(ptr(wire), C.size_t(len(wire)), (*C.uint8_t)(unsafe.Pointer(&out[0]))) != 0
	if !ok {
		return nil
	}
	return append([]byte(nil), out[:]...)
}

// MessagePayload — Extract an envelope's payload (None if it doesn't decode).
func MessagePayload(wire []byte) []byte {
	return take(C.spore_message_payload(ptr(wire), C.size_t(len(wire))))
}
