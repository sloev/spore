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

- **Envelope** (§2) — the only object: encode/decode, Ed25519 sign/verify,
  content-addressed `Id`, proof-of-work priority `stamp`.
- **`send(dest, data)` of any size** — one call that fragments arbitrarily large
  payloads and reassembles + verifies them on the far side; callers never think
  about MTUs. See [docs/DESIGN.md](docs/DESIGN.md) for the layers above transport.
- **Fragmentation** (§3) — a rateless GF(2) fountain code. Decodes from any
  lossy, out-of-order, one-way subset once `count` independent chunks arrive.
- **Routing** (§4–§5) — path learning ("first copy wins"), damped flood vs.
  directed unicast, dedup, store with eviction, the full `on_rx` router.
- **Sync & custody** (§6) — `ANNOUNCE` / `INV` / `WANT`.
- **Crypto** (§7) — anonymous sealed boxes to a recipient prekey; forward
  secrecy by rotating and deleting prekeys.
- **Bindings** (Page 2) — KISS framing for byte streams; `~S1.…~` Base32 armor
  for text channels, SMS, paper, and voice.

## Build & run

```sh
cargo test          # 11 tests: envelope, signatures, fountain, send/reassembly, seal, KISS, armor, sync
cargo run           # in-memory mesh demo (A — B — C — D), deterministic
cargo run -- udp    # a real node on UDP :7373 with LAN broadcast
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
```

## Layout

| Path            | What                                               |
|-----------------|----------------------------------------------------|
| `src/lib.rs`    | the portable core: envelope, fountain, `send`, router, crypto, KISS, armor |
| `src/main.rs`   | reference node + in-memory demo + UDP transport    |
| `docs/SPEC.md`  | the one-page SPORE v1 specification                 |
| `docs/DESIGN.md`| the application layers above transport: files, HTTP-over-SPORE, feeds |

## License

Public domain — released under [The Unlicense](LICENSE). No restrictions.
