# SPORE

**S**tore-and-forward **P**lanetary **O**pportunistic **R**elay **E**nvelope — a
Rust reference implementation of the [SPORE v1 spec](docs/SPEC.md).

A SPORE message is a **signed postcard**: to, from, expiry, payload, signature.
Its SHA-256 fingerprint is its identity. Every node keeps postcards it hasn't
seen, hands copies to anyone it meets who wants them, and drops duplicates and
expired mail. That alone is a working planetary network — no servers, no global
namespace, no always-on internet.

This crate is one portable core with pure-Rust crypto (no libsodium/C), so it
compiles unchanged to native targets and to `wasm32-unknown-unknown`. Transports
are plugins: they hand raw bytes to `Node::on_rx` and execute the `Forward`s it
returns. The router itself never changes across media — UDP, BLE, LoRa, serial,
paper, or a human reading armor aloud.

## What's implemented

The transport core is complete, plus three application layers on top of it:
`send()` for objects of any size, content-addressed files (magnet links + swarm),
and UDP-like sessions with an optional reliable stream. The remaining layers (HTTP,
feeds) are designed in [docs/DESIGN.md](docs/DESIGN.md).

<details>
<summary>Deep dive: the full feature list</summary>

- **Envelope** (§2) — the only object: encode/decode, Ed25519 sign/verify,
  content-addressed `Id`, proof-of-work priority `stamp`.
- **`send(dest, data)` of any size** — one call that fragments arbitrarily large
  payloads and reassembles + verifies them on the far side; callers never think
  about MTUs.
- **Fragmentation** (§3) — a rateless GF(2) fountain code. Decodes from any
  lossy, out-of-order, one-way subset once `count` independent chunks arrive.
- **Files** — `publish_file` mints a **magnet** (a signed manifest indexing chunk
  envelopes by content ID); peers `fetch` missing chunks via WANT and verify each
  for free, because a chunk's ID is the hash of its bytes. BitTorrent from the
  envelope primitive.
- **Sessions** — a UDP-like datagram link keyed on a cryptographic address (so it
  survives roaming), plus a simple QUIC-style Go-Back-N reliable stream for
  SSH/git-shaped needs. Reliability is endpoint state, never a network property.
- **Routing** (§4–§5) — path learning ("first copy wins"), damped flood vs.
  directed unicast, dedup, store with eviction, the full `on_rx` router.
- **Sync & custody** (§6) — `ANNOUNCE` / `INV` / `WANT`.
- **Crypto** (§7) — anonymous sealed boxes to a recipient prekey; forward
  secrecy by rotating and deleting prekeys.
- **Bridges** (Page 2) — a bridge only moves envelope bytes in and out of a node;
  the router never changes. Implemented: UDP, an HTTP `/spore/{push,inv,want}` bag,
  a `<hexid>.spore` folder store, a streaming KISS framer, and `~S1.…~` armor for
  text/SMS/paper/voice. HTTP is just one bridge, no more special than a folder.
</details>

## Build & run

```sh
cargo test               # 17 tests: envelope, fountain, send, files, sessions, reliable, bridges, seal, KISS, armor, sync
cargo run                # in-memory mesh demo (A — B — C — D)
cargo run -- udp         # a real node on UDP :7373 with LAN broadcast
cargo run -- http        # an HTTP "bag" bridge on :7373 (POST /spore/push, GET /spore/inv, POST /spore/want)
cargo run -- folder DIR  # a shared-store bridge over a folder of <hexid>.spore files
```

The demo drives the exact `Node::on_rx` router used in production; only the
transport is swapped for an in-memory "ether".

```
SPORE demo — line topology  A — B — C — D

[1] ANNOUNCE round done. A learned 3 peer prekeys; has D's prekey: true
[2] PUBLIC flood from A delivered to: ["B", "C", "D"]  (4 flood sends)
[3] SEALED unicast A->D delivered to: ["D"]  (3 directed hops, 0 floods)
    D decrypts payload: Some("meet at the north pier, midnight")
[4] SEND 6000 B object -> 7 fragments; reassembled + verified by: ["B", "C", "D"]
[5] FOUNTAIN over 40% loss, one-way: 21 data chunks needed; 25 survived of 53 sent; reassembled + signature-verified: true
[6] DATAGRAM session A->D over 3 directed hops: D decrypts Some("interactive hello over a udp-like link")
[7] FILE 'field-notes.txt' (9000 B) published as magnet 8ac320df4f09…; B pulled + verified: true
```

## Layout

| Path            | What                                               |
|-----------------|----------------------------------------------------|
| `src/lib.rs`    | the portable core: envelope, fountain, `send`, files, sessions, router, crypto, bridges |
| `src/main.rs`   | reference node + demo + UDP / HTTP-bag / folder bridge runners |
| `docs/SPEC.md`  | the one-page SPORE v1 specification                 |
| `docs/DESIGN.md`| the application layers above transport: files, HTTP-over-SPORE, feeds |

## License

Public domain — released under [The Unlicense](LICENSE). No restrictions.
