# SPORE application layer — design

SPORE the envelope is a **transport**: bytes in, `Forward`s out, and the frozen
16-byte header never changes across media (§2). This document proposes the thin
layers that sit *above* that transport so people can build real services — files,
web APIs, feeds — **without learning anything new**. Every idea here is a payload
convention plus a little endpoint state; relays are never touched, honoring the
spec's rule that "endpoint extras never change relays."

The guiding mantra:

> **URLs and `fetch()`, over sneakernet.**

Everything reduces to three shapes a web developer already knows:

| Shape | Web analogue | SPORE mechanism |
|---|---|---|
| **Objects** | files / magnet links | content-addressed blobs, fountain + swarm |
| **Request/response** | HTTP | `Request`/`Response` envelopes, reverse-path routed |
| **Feeds** | pub/sub, SSE | topics |

Streams (TCP semantics) are deliberately *not* offered — the spec lists "no
stream semantics" as a feature. These three cover the vast majority of services.

## The demux: an application tag

The envelope has no port/app field, and it's frozen. Endpoints self-describe with
a one-byte **app tag** as the first payload byte. Relays never read it; only the
destination demuxes on it.

| Tag | Meaning | Status |
|---|---|---|
| `0x01` | object manifest | planned |
| `0x02` | request (SPORE-HTTP) | planned |
| `0x03` | response (SPORE-HTTP) | planned |
| `0x05` | feed/event | planned |
| `0x06` | receipt/ACK (spec §8) | planned |
| `'O'` (0x4F) | mix onion (spec §9) | planned |

Fragment payloads keep their existing `[orig_id:16][index:1][count:1][chunk]`
shape and are recognised by the `FRAGMENT` flag, not a tag — they carry the
already-tagged inner envelope.

## Layer 1 — objects: `send(dest, data)` of any size  ✅ implemented

`Node::send(dest, data, now)` is the one call an app makes. Small payloads ride a
single signed envelope; anything over `Node::mtu` is fountain-fragmented (§3) and
reassembled + signature-verified by the receiver before the app sees it. Callers
never think about MTUs, loss, or ordering.

- **Fountain mode (push):** the sender can mint endless repair chunks, so the
  object survives simplex radio, CW, and paper tape with no back-channel.
- Relays forward fragments (each is an ordinary envelope) but do **not** buffer or
  reassemble objects not addressed to them.
- One fountain set caps at ~`mtu`×255 (≈ 50 KB at defaults). Bigger objects are
  the file layer's job.

## Layer 2 — files: manifests, magnet links, swarms  (next)

A large or long-lived file gets a small **manifest** envelope:

```
manifest = SIGNED {
    tag       = 0x01
    name, mime
    total_len, chunk_size, count
    merkle_root        // hash tree over the chunks
}
```

The manifest's 16-byte ID is a **magnet link** — shareable as `spore:<hexid>`, as
`~S1.…~` armor, or as a QR. Anyone holding a manifest can:

- **verify** each chunk against `merkle_root` before committing (integrity on a
  hostile link),
- **swarm** it: pull only the chunks they lack, from *any* peer that has them —
  turning INV/WANT into object-scoped reconciliation ("I hold indices {…} of
  object O" / "send me {…}"). This is BitTorrent, built from the fragment
  primitive: resumable, multi-source, out-of-order.

**Filesync (Syncthing) is the same mechanism pointed at a folder:** each file →
manifest, manifests flood on a folder-topic, followers swarm them; a newer signed
manifest for the same `name` supersedes the old. Encrypted folder = sealed
manifests behind a PSK topic (§7). One codebase yields both torrent and sync.

New API:

```rust
let magnet = node.publish_file("photo.jpg", bytes);  // -> manifest id
node.fetch_file(magnet).await;                        // swarm, verify, return
```

## Layer 3 — services: HTTP over SPORE  (the "nothing new to learn" layer)

Model services on **HTTP**, because every developer and every framework already
speaks it.

- A **service** is a topic or address a node serves.
- A **request** (`tag 0x02`) carries `method`, `path`, `headers`, `body` — sealed
  to the service's prekey, optionally signed or anonymous. A request-id nonce
  correlates the reply.
- A **response** (`tag 0x03`) goes back to the requester's address. The reverse
  path was learned automatically when the request arrived (§5, "first copy wins").
- Bodies of any size ride Layer 1 transparently.

Why this shape wins the two goals:

1. **Build for SPORE without learning SPORE.** A dev writes an ordinary
   `(req) -> res` handler; a thin adapter maps a SPORE request envelope to their
   existing Express/Flask/axum handler and back. The client call is `fetch`-shaped:
   `spore.fetch("spore://<addr>/path", { method, body }) -> Response`.
2. **Bridge the existing web for free.** A gateway proxies SPORE requests to
   `http://localhost:PORT` and back — so any IP-reachable service becomes reachable
   over LoRa/paper/sneakernet with zero rewrite, and vice versa.

**Free CDN.** Responses are content-addressed envelopes that every relay already
stores. Mark a `GET` response cacheable and *any* node that carried it can answer
a later identical request — the store is the cache, INV/WANT is the cache-fill.
Store-and-forward request/response is a CDN by construction.

Proposed API:

```rust
node.serve(topic("weather"), handler);          // handler: Request -> Response
let res = node.request(addr, Request { .. }).await;
```

## Layer 4 — feeds: pub/sub

Already native: a **topic** is a feed. Publishers `send(topic, event)`, subscribers
follow the topic and receive every event via the normal deliver path. Retention =
expiry; late joiners backfill from any peer's store via INV/WANT. This is Nostr's
signed-gossip model minus JSON, relays, and always-on internet.

## What's built vs. next

| Layer | State |
|---|---|
| Transport (envelope, router, sync, seal, KISS, armor) | ✅ in `src/lib.rs` |
| L1 objects — `send()` auto-fragment + reassembly | ✅ implemented |
| L2 files — manifest, magnet, swarm, folder-sync | ▢ designed |
| L3 services — SPORE-HTTP request/response + gateway | ▢ designed |
| L4 feeds — pub/sub over topics | ◑ works via topics; ergonomic API pending |

Each layer is additive and endpoint-only. The transport underneath never learns
they exist — which is exactly why a 200-byte LoRa link, a QR code, or a human
reading armor aloud remains a first-class peer for every one of them.
