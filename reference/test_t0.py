#!/usr/bin/env python3
"""Cross-language conformance: the pure-Python T0 decoder must reproduce the
reference (Rust) test vectors exactly — same address, same ID, valid signature —
and must reject a tampered envelope. Run: python3 reference/test_t0.py
"""
import json
import os
import sys

sys.path.insert(0, os.path.dirname(__file__))
import spore_t0 as t0  # noqa: E402

here = os.path.dirname(__file__)
vec = json.load(open(os.path.join(here, "vectors.json")))

pubkey = bytes.fromhex(vec["pubkey"])
signed = bytes.fromhex(vec["signed_wire"])
unsigned = bytes.fromhex(vec["unsigned_wire"])
tampered = bytes.fromhex(vec["tampered_wire"])

# 1. Address = SHA-256(pubkey)[..8].
assert t0.addr_of(pubkey).hex() == vec["addr"], "address mismatch"
# 2. Topic derivation.
assert t0.topic_of("news").hex() == vec["topic_news"], "topic mismatch"
# 3. Message IDs (signed and unsigned).
assert t0.envelope_id(unsigned).hex() == vec["unsigned_id"], "unsigned id mismatch"
assert t0.envelope_id(signed).hex() == vec["signed_id"], "signed id mismatch"
# 4. The parsed source key equals the vector's public key.
assert t0.parse(signed)["pubkey"] == pubkey, "parsed pubkey mismatch"
# 5. Signature verifies for the genuine envelope, fails for the tampered one.
assert t0.verify(signed) is True, "genuine signature should verify"
assert t0.verify(tampered) is False, "tampered envelope must not verify"
# 6. Armor round-trips back to the signed wire.
assert t0.armor_unwrap(vec["armor"]) == signed, "armor did not round-trip"

print("T0 OK — pure-Python decoder reproduces the Rust vectors and verifies signatures")
