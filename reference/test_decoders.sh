#!/usr/bin/env bash
# Conformance for the C and shell Tier-0 decoders: both must reproduce the Rust
# test vectors (address, message IDs) and verify signatures (accept the genuine
# envelope, reject a tampered one). Run: bash reference/test_decoders.sh
set -eu
cd "$(dirname "$0")/.."
V=reference/vectors.json
val() { python3 -c "import json;print(json.load(open('$V'))['$1'])"; }

ADDR=$(val addr); SID=$(val signed_id); UNSID=$(val unsigned_id)
SIGNED=$(val signed_wire); TAMPERED=$(val tampered_wire); ARMOR=$(val armor); UNSIGNED=$(val unsigned_wire)

cc -O2 -o /tmp/spore_t0 reference/spore_t0.c
C() { /tmp/spore_t0 "$1"; }
S() { bash reference/spore_t0.sh "$1"; }

check_signed() { # $1 label, $2 output
  printf %s "$2" | grep -q "$ADDR"           || { echo "[$1] address mismatch";  exit 1; }
  printf %s "$2" | grep -q "$SID"            || { echo "[$1] signed id mismatch"; exit 1; }
  printf %s "$2" | grep -qi "verifies: True" || { echo "[$1] signature not verified"; exit 1; }
}
check_signed "C signed"   "$(C "$SIGNED")"
check_signed "sh signed"  "$(S "$SIGNED")"
check_signed "C armor"    "$(C "$ARMOR")"
check_signed "sh armor"   "$(S "$ARMOR")"

C "$TAMPERED" | grep -qi "verifies: False" || { echo "C: tampered not rejected"; exit 1; }
S "$TAMPERED" | grep -qi "verifies: False" || { echo "sh: tampered not rejected"; exit 1; }

C "$UNSIGNED" | grep -q "$UNSID" || { echo "C: unsigned id mismatch"; exit 1; }
S "$UNSIGNED" | grep -q "$UNSID" || { echo "sh: unsigned id mismatch"; exit 1; }

echo "DECODERS OK — C and shell reproduce the Rust vectors and verify signatures"
