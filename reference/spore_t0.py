#!/usr/bin/env python3
"""SPORE Tier-0 reference node — parse, address, ID, and verify, in pure Python.

No packages: only the standard library. It reads a SPORE envelope (hex or text
armor), prints its fields, derives the sender's address, recomputes the message
ID, and verifies the Ed25519 signature. This is the smallest useful node — enough
to *receive and trust* public messages — and it doubles as a cross-language
conformance oracle for `docs/REBUILD.md`.

    python3 spore_t0.py <hex-or-armor>
    echo '~S1.….~' | python3 spore_t0.py

Ed25519 verification is the classic public-domain reference implementation
(ed25519.cr.yp.to / RFC 8032), inlined so this file has zero dependencies. It is
correct but slow — fine for one message; do not use it as a fast library.
"""
import sys
import hashlib

# --------------------------------------------------------------------------
# Ed25519 (reference, public domain) — verify only.
# --------------------------------------------------------------------------
_q = 2 ** 255 - 19
_l = 2 ** 252 + 27742317777372353535851937790883648493


def _inv(x):
    return pow(x, _q - 2, _q)


_d = (-121665 * _inv(121666)) % _q
_I = pow(2, (_q - 1) // 4, _q)


def _xrecover(y):
    xx = (y * y - 1) * _inv(_d * y * y + 1)
    x = pow(xx, (_q + 3) // 8, _q)
    if (x * x - xx) % _q != 0:
        x = (x * _I) % _q
    if x % 2 != 0:
        x = _q - x
    return x


_By = (4 * _inv(5)) % _q
_Bx = _xrecover(_By)
_B = [_Bx % _q, _By % _q]


def _edwards(P, Q):
    x1, y1 = P
    x2, y2 = Q
    x3 = (x1 * y2 + x2 * y1) * _inv(1 + _d * x1 * x2 * y1 * y2)
    y3 = (y1 * y2 + x1 * x2) * _inv(1 - _d * x1 * x2 * y1 * y2)
    return [x3 % _q, y3 % _q]


def _scalarmult(P, e):
    if e == 0:
        return [0, 1]
    Q = _scalarmult(P, e // 2)
    Q = _edwards(Q, Q)
    if e & 1:
        Q = _edwards(Q, P)
    return Q


def _bit(h, i):
    return (h[i // 8] >> (i % 8)) & 1


def _decodeint(s):
    return sum(2 ** i * _bit(s, i) for i in range(256))


def _isoncurve(P):
    x, y = P
    return (-x * x + y * y - 1 - _d * x * x * y * y) % _q == 0


def _decodepoint(s):
    y = sum(2 ** i * _bit(s, i) for i in range(255))
    x = _xrecover(y)
    if x & 1 != _bit(s, 255):
        x = _q - x
    P = [x, y]
    if not _isoncurve(P):
        raise ValueError("point not on curve")
    return P


def ed25519_verify(signature, message, public_key):
    """True iff `signature` (64 bytes) is valid for `message` under `public_key`."""
    if len(signature) != 64 or len(public_key) != 32:
        return False
    try:
        R = _decodepoint(signature[:32])
        A = _decodepoint(public_key)
    except ValueError:
        return False
    S = _decodeint(signature[32:])
    # h is the full 512-bit SHA-512 digest as a little-endian integer (not the
    # 256-bit `_decodeint`, which would truncate the hash).
    h = int.from_bytes(hashlib.sha512(signature[:32] + public_key + message).digest(), "little")
    return _scalarmult(_B, S) == _edwards(R, _scalarmult(A, h))


# --------------------------------------------------------------------------
# SPORE envelope (see docs/REBUILD.md and docs/SPEC.md).
# --------------------------------------------------------------------------
VER = 0x01
F_ENCRYPTED, F_SIGNED, F_FRAGMENT, F_ACKREQ, F_FLOOD, F_SRC8 = 1, 2, 4, 8, 16, 32
TYPES = {0: "DATA", 1: "INV", 2: "WANT", 3: "ANNOUNCE"}
_B32 = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567"


def sha256(b):
    return hashlib.sha256(b).digest()


def addr_of(pubkey):
    return sha256(pubkey)[:8]


def topic_of(name):
    return sha256(name.encode())[:8]


def envelope_id(wire):
    """SHA-256 of the full wire with the hops byte (offset 3) zeroed, first 16."""
    b = bytearray(wire)
    b[3] = 0
    return bytes(sha256(bytes(b))[:16])


def parse(wire):
    if len(wire) < 16 or wire[0] != VER:
        raise ValueError("not a SPORE v1 envelope")
    typ, flags, hops = wire[1], wire[2], wire[3]
    expiry = int.from_bytes(wire[4:8], "big")
    dest = wire[8:16]
    off = 16
    src = pubkey = None
    if flags & F_SIGNED:
        if flags & F_SRC8:
            src = wire[off:off + 8]; off += 8
        else:
            pubkey = wire[off:off + 32]; src = pubkey; off += 32
    plen = int.from_bytes(wire[off:off + 2], "big"); off += 2
    payload = wire[off:off + plen]; off += plen
    sig = wire[off:off + 64] if flags & F_SIGNED else None
    return {
        "typ": typ, "flags": flags, "hops": hops, "expiry": expiry,
        "dest": dest, "src": src, "pubkey": pubkey, "payload": payload, "sig": sig,
    }


def verify(wire):
    """Verify a full-key signed envelope's signature over its zeroed-hops body."""
    e = parse(wire)
    if not (e["flags"] & F_SIGNED) or e["pubkey"] is None or e["sig"] is None:
        return False
    # Pre-image = body with hops=0 and without the trailing 64-byte signature.
    body = bytearray(wire[: len(wire) - 64])
    body[3] = 0
    return ed25519_verify(e["sig"], bytes(body), e["pubkey"])


def armor_unwrap(text):
    """Recover envelope bytes from ~S1.<base32>.<base32(checksum)>~ armor."""
    i = text.find("~S1.")
    if i < 0:
        return None
    j = text.find("~", i + 4)
    body = text[i + 4:j]
    b32, ck = body.rsplit(".", 1)
    env = _b32decode(b32)
    if sha256(env)[:4] != _b32decode(ck):
        raise ValueError("armor checksum mismatch")
    return env


def _b32decode(s):
    buf = bits = 0
    out = bytearray()
    for c in s:
        if c.isspace():
            continue
        buf = (buf << 5) | _B32.index(c.upper())
        bits += 5
        if bits >= 8:
            bits -= 8
            out.append((buf >> bits) & 0xFF)
    return bytes(out)


def _read_input(arg):
    arg = arg.strip()
    if arg.startswith("~S1."):
        return armor_unwrap(arg)
    return bytes.fromhex(arg.replace(" ", ""))


def main():
    data = sys.argv[1] if len(sys.argv) > 1 else sys.stdin.read()
    wire = _read_input(data)
    e = parse(wire)
    fl = e["flags"]
    names = [n for b, n in [(F_ENCRYPTED, "ENC"), (F_SIGNED, "SIGNED"), (F_FRAGMENT, "FRAG"),
                            (F_ACKREQ, "ACKREQ"), (F_FLOOD, "FLOOD"), (F_SRC8, "SRC8")] if fl & b]
    print(f"type    : {TYPES.get(e['typ'], e['typ'])}")
    print(f"flags   : 0x{fl:02x} [{' '.join(names)}]")
    print(f"hops    : {e['hops']}")
    print(f"expiry  : {e['expiry']}")
    print(f"dest    : {e['dest'].hex()}")
    if e["pubkey"]:
        print(f"src key : {e['pubkey'].hex()}")
        print(f"src addr: {addr_of(e['pubkey']).hex()}")
    print(f"id      : {envelope_id(wire).hex()}")
    try:
        printable = e["payload"].decode("utf-8")
    except UnicodeDecodeError:
        printable = e["payload"].hex()
    print(f"payload : {printable!r}")
    if e["sig"] is not None:
        print(f"signature verifies: {verify(wire)}")


if __name__ == "__main__":
    main()
