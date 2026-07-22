"""Smoke test / usage example for the generated Python binding.

    cargo build --release           # build ../../target/release/libspore.so
    python3 bindings/python/test.py
"""
import spore


def main():
    # identity + a signed message
    sk, pk = spore.keypair()
    addr = spore.addr_of(pk)
    wire = spore.message_new(sk, addr, 1_700_000_000, b"hello from python")
    assert spore.message_verify(wire) is True
    assert spore.message_payload(wire) == b"hello from python"
    assert len(spore.message_id(wire)) == 16

    bad = bytearray(wire)
    bad[-1] ^= 0xFF
    assert spore.message_verify(bytes(bad)) is False

    # sealed box to a prekey
    sec, pub = spore.prekey()
    sealed = spore.seal(b"for holder only", pub)
    assert spore.open(sealed, sec) == b"for holder only"
    assert spore.open(sealed, bytes(32)) is None

    # encrypted topic
    psk = bytes(range(32))
    ct = spore.topic_seal(b"members only", psk)
    assert spore.topic_open(ct, psk) == b"members only"

    # topic address + text armor
    assert len(spore.topic_of(b"news")) == 8
    text = spore.armor_wrap(wire)
    assert text.startswith(b"~S1.")
    assert spore.armor_unwrap(b"noise " + text + b" noise") == wire

    print("python binding OK")


if __name__ == "__main__":
    main()
