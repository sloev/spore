package spore

import "bytes"
import "testing"

func TestRoundtrip(t *testing.T) {
	sk, pk := Keypair()
	if len(sk) != 32 || len(pk) != 32 {
		t.Fatal("keypair size")
	}
	addr := AddrOf(pk)
	wire := MessageNew(sk, addr, 1700000000, []byte("hello from go"))
	if !MessageVerify(wire) {
		t.Fatal("verify")
	}
	if !bytes.Equal(MessagePayload(wire), []byte("hello from go")) {
		t.Fatal("payload")
	}
	if id := MessageId(wire); id == nil || len(id) != 16 {
		t.Fatal("id")
	}
	bad := append([]byte(nil), wire...)
	bad[len(bad)-1] ^= 0xFF
	if MessageVerify(bad) {
		t.Fatal("tamper should fail")
	}
	sec, pub := Prekey()
	sealed := Seal([]byte("secret"), pub)
	if !bytes.Equal(Open(sealed, sec), []byte("secret")) {
		t.Fatal("seal/open")
	}
	if Open(sealed, make([]byte, 32)) != nil {
		t.Fatal("wrong key should fail")
	}
	psk := make([]byte, 32)
	ct := TopicSeal([]byte("members"), psk)
	if !bytes.Equal(TopicOpen(ct, psk), []byte("members")) {
		t.Fatal("topic seal/open")
	}
	text := ArmorWrap(wire)
	if !bytes.Equal(ArmorUnwrap(text), wire) {
		t.Fatal("armor")
	}
}
