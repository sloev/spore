#!/usr/bin/env python3
"""SPORE Tier-1 reference decoder — everything above the envelope, in pure Python.

`spore_t0.py` is the smallest node that can *trust* a message: parse an envelope,
derive an address, recompute an id, verify a signature. That is §1 and §2, and it
is where interop starts rather than where it finishes. A node that does all of it
perfectly and cannot parse an INV, cannot reassemble a frame its link had to
split, and cannot read a manifest is wire-compatible and unable to talk.

This file is the next layer: link fragmentation, KISS, the INV/WANT payload
shapes, and the file layer's manifest encodings. Standard library only, and
deliberately written from the *spec* rather than translated from the Rust — a
second implementation that shares the first one's misreading proves nothing.

    python3 reference/test_t1.py     # checks this against versioned_vectors.json

**Tier, and what it promises.** These live one row below §1/§2 in the spec's
tier table: normative, but changeable with a minor release so long as both ends
of one link agree. `reference/vectors.json` is frozen and CI refuses to edit it;
`reference/versioned_vectors.json`, which this reads, is regenerated and
diff-checked but may legitimately change. Build against both, and know which is
which.
"""
import hashlib

# ---------------------------------------------------------------------------
# Link fragmentation — one envelope across one narrow hop.
# ---------------------------------------------------------------------------
#
# [0xF6][set:2][idx:2][count:2][piece …]
#
# The magic is not VER, so a receiver can tell a piece from a whole envelope
# without being told — which is what lets one link carry both.
LINK_FRAG_MAGIC = 0xF6
LINK_FRAG_OVERHEAD = 7


def is_fragment(frame):
    """True if this frame is a link fragment rather than a whole envelope."""
    return len(frame) >= LINK_FRAG_OVERHEAD and frame[0] == LINK_FRAG_MAGIC


def parse_fragment(frame):
    """-> dict(set_id, idx, count, piece). Raises on a frame that is not one."""
    if not is_fragment(frame):
        raise ValueError("not a link fragment")
    return {
        "set_id": int.from_bytes(frame[1:3], "big"),
        "idx": int.from_bytes(frame[3:5], "big"),
        "count": int.from_bytes(frame[5:7], "big"),
        "piece": frame[7:],
    }


def reassemble(frames):
    """Put one set back together, or return None if it is not yet complete.

    Only source pieces (`idx < count`) are used. Indices at or past `count` are
    repair symbols, which need the fountain decoder and are not this file's job —
    but they arrive on the same link and must not be mistaken for source data.
    """
    if not frames:
        return None
    parsed = [parse_fragment(f) for f in frames if is_fragment(f)]
    if not parsed:
        # A link that did not need to fragment sends the frame unwrapped. A
        # decoder that always strips seven bytes passes every fragmenting test
        # and quietly corrupts every link wide enough not to fragment.
        return frames[0] if len(frames) == 1 else None
    count = parsed[0]["count"]
    have = {}
    for p in parsed:
        if p["count"] != count:
            raise ValueError("mixed counts in one set")
        if p["idx"] < count:
            have[p["idx"]] = p["piece"]
    if len(have) != count:
        return None
    return b"".join(have[i] for i in range(count))


# ---------------------------------------------------------------------------
# KISS — the serial framing every radio bridge shares.
# ---------------------------------------------------------------------------
FEND, FESC, TFEND, TFESC = 0xC0, 0xDB, 0xDC, 0xDD


def kiss_encode(frame):
    out = bytearray([FEND, 0x00])  # FEND + command byte
    for b in frame:
        if b == FEND:
            out += bytes([FESC, TFEND])
        elif b == FESC:
            out += bytes([FESC, TFESC])
        else:
            out.append(b)
    out.append(FEND)
    return bytes(out)


def kiss_decode(stream):
    """Extract complete frames, command byte stripped."""
    frames, cur = [], bytearray()
    in_frame = got_cmd = esc = False
    for b in stream:
        if b == FEND:
            if in_frame and cur:
                frames.append(bytes(cur))
            cur, in_frame, got_cmd, esc = bytearray(), True, False, False
            continue
        if not in_frame:
            continue
        if not got_cmd:
            got_cmd = True  # the command byte is not part of the frame
            continue
        if esc:
            cur.append(FEND if b == TFEND else FESC if b == TFESC else b)
            esc = False
        elif b == FESC:
            esc = True
        else:
            cur.append(b)
    return frames


# ---------------------------------------------------------------------------
# Gossip — INV and WANT payloads.
# ---------------------------------------------------------------------------
TY_INV, TY_WANT = 1, 2
FL_CANCEL = 0x80


def parse_inv(payload):
    """INV is a bare concatenation of 16-byte ids. Nothing else."""
    return [payload[i:i + 16] for i in range(0, len(payload) - len(payload) % 16, 16)]


def parse_want(payload, flags=0):
    """-> dict(ids, depth, cancel).

    A WANT may carry one trailing byte of remaining depth. Ids are 16 bytes, so
    an odd payload length is unambiguous — and a decoder that only does
    `chunks(16)` drops the byte, answers at its own default budget, and has no
    way to notice. `depth` is None when the asker did not say.

    A cancel never carries the byte, so its length stays even and an older build
    parses it as a plain WANT rather than reading half an id. An *empty* cancel
    payload is the wildcard: retire every interest this peer placed.
    """
    cancel = bool(flags & FL_CANCEL)
    depth = None
    if not cancel and len(payload) % 16 == 1:
        depth = payload[-1]
        payload = payload[:-1]
    return {"ids": parse_inv(payload), "depth": depth, "cancel": cancel}


# ---------------------------------------------------------------------------
# File layer — manifests.
# ---------------------------------------------------------------------------
MANIFEST_TAG, CHUNK_TAG, TREE_TAG, SEALED_TAG = 0x01, 0x07, 0x08, 0x09


def content_id(payload):
    """The name of a file-layer object: SHA-256(payload)[:16].

    Distinct from an envelope id, which hashes the whole envelope and so covers
    `created_at` and `dest`. That is right for a message and wrong for bytes:
    it would make the same chunk published a second later a different object.
    """
    return hashlib.sha256(payload).digest()[:16]


def parse_manifest(p):
    """-> dict for MANIFEST/TREE/SEALED. Returns None for anything else.

    The depth byte is present for TREE and SEALED and *absent* for MANIFEST, so
    a decoder that assumes one fixed header offset reads a leaf's `file_id`
    shifted by one byte and gets a plausible-looking wrong answer.
    """
    if not p:
        return None
    tag = p[0]
    o = 1
    hdr_id = bytes(16)
    if tag == MANIFEST_TAG:
        depth = 0
    elif tag == TREE_TAG:
        depth = p[1]
        o += 1
    elif tag == SEALED_TAG:
        depth = p[1]
        o += 1
        hdr_id = p[o:o + 16]
        o += 16
    else:
        return None
    file_id = p[o:o + 16]; o += 16
    chunk_size = int.from_bytes(p[o:o + 4], "big"); o += 4
    count = int.from_bytes(p[o:o + 4], "big"); o += 4
    total_len = int.from_bytes(p[o:o + 8], "big"); o += 8
    nlen = int.from_bytes(p[o:o + 2], "big"); o += 2
    name = p[o:o + nlen].decode("utf-8", "replace"); o += nlen
    chunk_ids = [p[i:i + 16] for i in range(o, len(p) - (len(p) - o) % 16, 16)]
    return {
        "tag": tag, "depth": depth, "hdr_id": hdr_id, "file_id": file_id,
        "chunk_size": chunk_size, "count": count, "total_len": total_len,
        "name": name, "chunk_ids": chunk_ids,
        # A sealed root names its header rather than carrying it: `total_len` is
        # the *plaintext* length and every chunk below holds ciphertext.
        "sealed": hdr_id != bytes(16),
    }
