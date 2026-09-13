#!/usr/bin/env python3
"""Cross-language conformance above the envelope: the pure-Python T1 decoder must
reproduce `versioned_vectors.json` exactly — same fragment headers, same
reassembled frame, same INV/WANT parse, same manifest fields.

Run: python3 reference/test_t1.py

This is the half of interop `test_t0.py` does not cover. Two implementations can
pass every T0 check and still fail to exchange a single file.
"""
import json
import os
import sys

sys.path.insert(0, os.path.dirname(__file__))
import spore_t1 as t1  # noqa: E402

here = os.path.dirname(__file__)
vec = json.load(open(os.path.join(here, "versioned_vectors.json")))

# --- link fragmentation ----------------------------------------------------
lf = vec["link_framing"]
assert t1.LINK_FRAG_MAGIC == int(lf["magic"], 16), "magic mismatch"
assert t1.LINK_FRAG_OVERHEAD == lf["overhead"], "overhead mismatch"

frame = bytes.fromhex(lf["frame"])
pieces = [bytes.fromhex(h) for h in lf["split_mtu_40"]["pieces"]]
assert len(pieces) == lf["split_mtu_40"]["count"], "piece count mismatch"
assert len(pieces) > 2, "the vector must actually fragment, or this checks nothing"

for i, p in enumerate(pieces):
    f = t1.parse_fragment(p)
    assert f["set_id"] == lf["split_mtu_40"]["set_id"], "set id mismatch"
    assert f["idx"] == i, "index mismatch"
    assert f["count"] == len(pieces), "count field mismatch"

assert t1.reassemble(pieces) == frame, "reassembly did not reproduce the frame"
# Out of order, because a link does not promise order and a reassembler that
# relies on arrival order works on loopback and nowhere else.
assert t1.reassemble(list(reversed(pieces))) == frame, "reassembly depends on arrival order"
assert t1.reassemble(pieces[:-1]) is None, "an incomplete set must not reassemble"

# A frame that fits its link is sent unwrapped — no header to strip.
whole = [bytes.fromhex(h) for h in lf["split_mtu_4096_fits_whole"]["pieces"]]
assert len(whole) == 1 and whole[0] == frame, "a fitting frame must be unchanged"
assert not t1.is_fragment(whole[0]), "a whole envelope must not look like a fragment"
assert t1.reassemble(whole) == frame, "an unwrapped frame must pass through"

# --- KISS ------------------------------------------------------------------
k = lf["kiss"]
raw = bytes.fromhex(k["frame"])
assert t1.kiss_encode(raw).hex() == k["encoded"], "kiss encoding mismatch"
assert t1.kiss_decode(bytes.fromhex(k["encoded"])) == [raw], "kiss round-trip failed"

# --- gossip ----------------------------------------------------------------
g = vec["gossip"]
assert (t1.TY_INV, t1.TY_WANT, t1.FL_CANCEL) == (g["ty_inv"], g["ty_want"], g["fl_cancel"])


def payload_of(wire_hex):
    """The payload of an envelope, without a full T0 parse: `plen` is the two
    bytes following the 16-byte header, and the payload follows `plen`."""
    w = bytes.fromhex(wire_hex)
    plen = int.from_bytes(w[16:18], "big")
    return w[18:18 + plen]


assert [i.hex() for i in t1.parse_inv(payload_of(g["inv"]["wire"]))] == g["inv"]["ids"], "inv ids"

w = t1.parse_want(payload_of(g["want"]["wire"]))
assert [i.hex() for i in w["ids"]] == g["want"]["ids"] and w["depth"] is None, "plain want"

wd = t1.parse_want(payload_of(g["want_with_depth"]["wire"]))
assert [i.hex() for i in wd["ids"]] == g["want_with_depth"]["ids"], "want-with-depth ids"
assert wd["depth"] == g["want_with_depth"]["depth"], "the trailing depth byte was dropped"

c = t1.parse_want(payload_of(g["cancel"]["wire"]), flags=t1.FL_CANCEL)
assert c["cancel"] and [i.hex() for i in c["ids"]] == g["cancel"]["ids"], "cancel"
assert c["depth"] is None, "a cancel never carries a depth byte"

ca = t1.parse_want(payload_of(g["cancel_all"]["wire"]), flags=t1.FL_CANCEL)
assert ca["cancel"] and ca["ids"] == [], "the wildcard cancel is an empty payload, not a malformed one"

# --- file layer ------------------------------------------------------------
f = vec["file_layer"]
tags = f["tags"]
assert (t1.MANIFEST_TAG, t1.CHUNK_TAG, t1.TREE_TAG, t1.SEALED_TAG) == tuple(
    int(tags[n], 16) for n in ("manifest", "chunk", "tree", "sealed")
), "file-layer tags"

for label in ("leaf", "tree", "sealed"):
    v = f[label]
    enc = bytes.fromhex(v["encoded"])
    m = t1.parse_manifest(enc)
    assert m is not None, f"{label}: refused to parse"
    assert m["depth"] == v["depth"], f"{label}: depth"
    assert m["count"] == v["count"], f"{label}: count"
    assert m["total_len"] == v["total_len"], f"{label}: total_len"
    assert m["chunk_size"] == v["chunk_size"], f"{label}: chunk_size"
    assert m["name"] == v["name"], f"{label}: name"
    assert [c.hex() for c in m["chunk_ids"]] == v["chunk_ids"], f"{label}: chunk ids"
    assert t1.content_id(enc).hex() == v["content_id"], f"{label}: content id"
    assert m["sealed"] == (label == "sealed"), f"{label}: sealed flag"

# The leaf is the one whose header has no depth byte, and these two manifests
# carry the same file_id at different offsets because of it. Pinned directly, so
# a decoder that "simplifies" to one fixed offset fails here rather than
# returning a plausible wrong answer.
leaf = bytes.fromhex(f["leaf"]["encoded"])
tree = bytes.fromhex(f["tree"]["encoded"])
file_id = t1.parse_manifest(leaf)["file_id"]
assert leaf[1:17] == file_id, "leaf file_id must start at offset 1 (no depth byte)"
assert tree[2:18] == file_id, "tree file_id must start at offset 2 (after the depth byte)"
assert t1.parse_manifest(tree)["file_id"] == file_id, "the depth byte was mis-skipped"

assert t1.parse_manifest(bytes([t1.CHUNK_TAG]) + b"data") is None, "a chunk is not a manifest"
assert t1.parse_manifest(b"") is None, "empty payload"

# --- forwarding ------------------------------------------------------------
# Not byte layouts: the decisions two implementations must share to interoperate
# at all. Every `observed` field was recorded by driving a real node, so a Python
# decoder cannot re-derive them — what it can check is that the table stays
# honest: every case names its reasoning, and the cases that matter most are the
# ones where the answer is "nothing".
fw = vec["forwarding"]
assert len(fw) >= 8, "the forwarding table should cover more than the happy path"
for case in fw:
    for key in ("input", "state", "observed", "why"):
        assert case.get(key), f"forwarding case missing {key}: {case}"
assert any("seen" in c["state"] for c in fw), "dedup must be in the table"
assert any(c["observed"] == "nothing" for c in fw), "the table must say what is dropped"
assert any("Flood" in c["observed"] for c in fw), "and what is relayed"
assert any("Directed" in c["observed"] for c in fw), "and what is answered on one link"

# The rule that is easiest to get backwards, and the reason this table is
# observed rather than written: a relay forwards what it cannot verify. A second
# implementation that "hardens" by dropping it would also drop every envelope it
# merely lacks the key for.
bad_sig = [c for c in fw if "altered after signing" in c["input"]]
assert bad_sig and bad_sig[0]["observed"] != "nothing", (
    "a failed signature must not stop a relay: verify before binding trust state, "
    "do not verify to forward"
)

print("T1 OK — link framing, KISS, INV/WANT and manifests match the Rust vectors")
