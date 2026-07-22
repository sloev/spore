"""Auto-generated SPORE bindings (ctypes) — do not edit; run bindings/generate.py.

Loads libspore from $SPORE_LIB or ../../target/release/. Every function takes and
returns `bytes`; functions that can fail return `None`.
"""
import ctypes
import os

def _load():
    names = ("libspore.so", "libspore.dylib", "spore.dll")
    here = os.path.dirname(os.path.abspath(__file__))
    tried = []
    if os.environ.get("SPORE_LIB"):
        tried.append(os.environ["SPORE_LIB"])
    for n in names:
        tried.append(os.path.join(here, "..", "..", "target", "release", n))
        tried.append(n)
    last = None
    for path in tried:
        try:
            return ctypes.CDLL(path)
        except OSError as e:
            last = e
    raise last

_l = _load()

class SporeBytes(ctypes.Structure):
    _fields_ = [("data", ctypes.POINTER(ctypes.c_ubyte)), ("len", ctypes.c_size_t)]

_l.spore_bytes_free.argtypes = [SporeBytes]
_l.spore_bytes_free.restype = None

def _take(b):
    if not b.data:
        return None
    out = ctypes.string_at(b.data, b.len)
    _l.spore_bytes_free(b)
    return out


_l.spore_keypair.argtypes = [ctypes.POINTER(ctypes.c_ubyte), ctypes.POINTER(ctypes.c_ubyte)]

_l.spore_keypair.restype = None

def keypair():
    """Generate an Ed25519 signing keypair (secret, public)."""
    out_sk = (ctypes.c_ubyte * 32)()
    out_pk = (ctypes.c_ubyte * 32)()
    _r = _l.spore_keypair(out_sk, out_pk)
    return (bytes(out_sk), bytes(out_pk))

_l.spore_prekey.argtypes = [ctypes.POINTER(ctypes.c_ubyte), ctypes.POINTER(ctypes.c_ubyte)]

_l.spore_prekey.restype = None

def prekey():
    """Generate an X25519 encryption prekey pair (secret, public)."""
    out_sec = (ctypes.c_ubyte * 32)()
    out_pub = (ctypes.c_ubyte * 32)()
    _r = _l.spore_prekey(out_sec, out_pub)
    return (bytes(out_sec), bytes(out_pub))

_l.spore_addr_of.argtypes = [ctypes.c_char_p, ctypes.POINTER(ctypes.c_ubyte)]

_l.spore_addr_of.restype = None

def addr_of(pk):
    """SPORE address = SHA-256(pubkey)[..8]."""
    out = (ctypes.c_ubyte * 8)()
    _r = _l.spore_addr_of(pk, out)
    return bytes(out)

_l.spore_topic_of.argtypes = [ctypes.c_char_p, ctypes.c_size_t, ctypes.POINTER(ctypes.c_ubyte)]

_l.spore_topic_of.restype = None

def topic_of(s):
    """Topic address = SHA-256(utf8)[..8]."""
    out = (ctypes.c_ubyte * 8)()
    _r = _l.spore_topic_of(s, len(s), out)
    return bytes(out)

_l.spore_seal.argtypes = [ctypes.c_char_p, ctypes.c_size_t, ctypes.c_char_p]

_l.spore_seal.restype = SporeBytes

def seal(msg, recip_prekey):
    """Anonymous sealed box to a recipient prekey."""
    _r = _l.spore_seal(msg, len(msg), recip_prekey)
    return _take(_r)

_l.spore_open.argtypes = [ctypes.c_char_p, ctypes.c_size_t, ctypes.c_char_p]

_l.spore_open.restype = SporeBytes

def open(sealed, prekey_sec):
    """Open a sealed box with a prekey secret (None on failure)."""
    _r = _l.spore_open(sealed, len(sealed), prekey_sec)
    return _take(_r)

_l.spore_topic_seal.argtypes = [ctypes.c_char_p, ctypes.c_size_t, ctypes.c_char_p]

_l.spore_topic_seal.restype = SporeBytes

def topic_seal(msg, psk):
    """Encrypt for a topic under a 32-byte pre-shared key."""
    _r = _l.spore_topic_seal(msg, len(msg), psk)
    return _take(_r)

_l.spore_topic_open.argtypes = [ctypes.c_char_p, ctypes.c_size_t, ctypes.c_char_p]

_l.spore_topic_open.restype = SporeBytes

def topic_open(ct, psk):
    """Decrypt a topic payload (None on failure)."""
    _r = _l.spore_topic_open(ct, len(ct), psk)
    return _take(_r)

_l.spore_armor_wrap.argtypes = [ctypes.c_char_p, ctypes.c_size_t]

_l.spore_armor_wrap.restype = SporeBytes

def armor_wrap(env):
    """Armor envelope bytes into ~S1.…~ text."""
    _r = _l.spore_armor_wrap(env, len(env))
    return _take(_r)

_l.spore_armor_unwrap.argtypes = [ctypes.c_char_p, ctypes.c_size_t]

_l.spore_armor_unwrap.restype = SporeBytes

def armor_unwrap(text):
    """Recover envelope bytes from armor (None if absent)."""
    _r = _l.spore_armor_unwrap(text, len(text))
    return _take(_r)

_l.spore_message_new.argtypes = [ctypes.c_char_p, ctypes.c_char_p, ctypes.c_uint32, ctypes.c_char_p, ctypes.c_size_t]

_l.spore_message_new.restype = SporeBytes

def message_new(sk, dest, expiry, payload):
    """Build and sign a DATA envelope; returns wire bytes."""
    _r = _l.spore_message_new(sk, dest, expiry, payload, len(payload))
    return _take(_r)

_l.spore_message_verify.argtypes = [ctypes.c_char_p, ctypes.c_size_t]

_l.spore_message_verify.restype = ctypes.c_ubyte

def message_verify(wire):
    """Verify a signed envelope's signature."""
    _r = _l.spore_message_verify(wire, len(wire))
    return bool(_r)

_l.spore_message_id.argtypes = [ctypes.c_char_p, ctypes.c_size_t, ctypes.POINTER(ctypes.c_ubyte)]

_l.spore_message_id.restype = ctypes.c_ubyte

def message_id(wire):
    """Envelope 16-byte content ID (None if it doesn't decode)."""
    out = (ctypes.c_ubyte * 16)()
    _r = _l.spore_message_id(wire, len(wire), out)
    return (bytes(out)) if _r else None

_l.spore_message_payload.argtypes = [ctypes.c_char_p, ctypes.c_size_t]

_l.spore_message_payload.restype = SporeBytes

def message_payload(wire):
    """Extract an envelope's payload (None if it doesn't decode)."""
    _r = _l.spore_message_payload(wire, len(wire))
    return _take(_r)
