# SPORE application layer — design

SPORE's envelope is a dumb pipe: you hand it bytes and it moves them, and the
16-byte header on the wire never changes no matter what carries it. This document
is about everything you'd actually *build* on that pipe — file sharing, web-style
services, live feeds, interactive sessions — and how to do it so that people who
already know the web don't have to learn anything new. The golden rule: none of it
touches relays. Every feature here is just a convention about what goes *inside* a
payload, plus a little bookkeeping at the two endpoints.

<details>
<summary>Deep dive: why "endpoint-only" matters</summary>

The spec freezes the envelope layout and states that "endpoint extras never change
relays." That's what lets a 200-byte LoRa packet, a QR code, or a human reading
armor aloud stay a first-class peer: a relay only ever parses the fixed header,
dedups, stores, and forwards. If a feature required relays to understand it, every
one of those media would have to be upgraded in lockstep — the opposite of the
design. So the entire application layer is built from two ingredients only:

1. **payload conventions** — bytes the destination knows how to interpret, and
2. **endpoint state** — tables and buffers kept by the sender and receiver.

The guiding mantra for the whole layer: **URLs and `fetch()`, over sneakernet.**
</details>

## Everything is one of four shapes

Almost every service you'd want maps onto one of four familiar shapes, and each
one corresponds to a mechanism SPORE already has or can express as a payload
convention. If you've built web apps, you already know all four.

<details>
<summary>Deep dive: the four shapes and their web analogues</summary>

| Shape | Web analogue | SPORE mechanism | State |
|---|---|---|---|
| **Objects** | files / magnet links | content-addressed blobs, fountain + swarm | L1 done, L2 designed |
| **Sessions** | UDP / QUIC / Mosh | datagram links + reliable stream | ✅ implemented |
| **Request/response** | HTTP | `Request`/`Response` envelopes, reverse-path routed | designed |
| **Feeds** | pub/sub, SSE | topics | works via topics |

Note what is deliberately *absent*: a raw reliable byte-stream offered *by the
network*. The spec lists "no stream semantics" as a feature, because a stream
assumes a live, low-latency, bidirectional path — the one thing an opportunistic
network can't promise. Reliability still exists, but as an **endpoint** concern
(the Sessions layer), exactly the way QUIC builds reliability on top of UDP
without the network knowing.
</details>

## How endpoints tell payloads apart: the app tag

Because the header is frozen, there's no "port" or "content-type" field on the
wire. Instead the destination looks at the very first byte of the payload — the
**app tag** — to know what it's holding. Relays never look at it; only the endpoint
that the message is addressed to.

<details>
<summary>Deep dive: the tag registry</summary>

| Tag | Meaning | Status |
|---|---|---|
| `0x01` | object manifest | planned |
| `0x02` | request (SPORE-HTTP) | planned |
| `0x03` | response (SPORE-HTTP) | planned |
| `0x04` | **datagram (session)** | ✅ implemented |
| `0x05` | feed/event | planned |
| `0x06` | receipt/ACK (spec §8) | planned |
| `'O'` (0x4F) | mix onion (spec §9) | planned |

Fragments are the one exception: they're recognised by the `FRAGMENT` header flag,
not a tag, and their payload is `[orig_id:16][index:1][count:1][chunk]`. The chunk
they carry is a slice of an ordinary, already-tagged inner envelope, so tags and
fragmentation compose cleanly.
</details>

## Layer 1 — objects: send anything, any size  ✅ implemented

You call `send(dest, data)` and it just works, whether `data` is 10 bytes or
50 KB. Small things go as a single message; big things are automatically split
into chunks, sent, and reassembled on the far side — and the receiver checks the
signature before your app ever sees it. You never think about packet sizes, loss,
or ordering.

<details>
<summary>Deep dive: fountain coding and relay behaviour</summary>

`Node::send(dest, data, now)` builds and signs one inner envelope. If its wire form
fits `Node::mtu` it ships as-is; otherwise it's fountain-fragmented (§3):

- **Rateless push:** the sender can mint endless distinct repair chunks, so the
  object decodes from *any* sufficient subset — simplex radio, CW, and paper tape
  work with no back-channel at all.
- **Relays stay dumb:** each fragment is an ordinary flooded envelope with its own
  ID. A relay forwards fragments but never buffers or reassembles an object that
  isn't addressed to it — only the destination reassembles.
- **Cap:** one fountain set is ≤ ~`mtu`×255 (≈ 50 KB at defaults). Larger data is
  the file layer's job (below), which stitches many objects under one manifest.

Implemented in `src/lib.rs` (`Node::send`, `Fountain`, the `ingest` reassembly
path) and covered by tests for single-envelope sends, large-object
fragment/reassembly, and relay-forwards-without-reassembling.
</details>

## Layer 2 — files: magnet links, torrents, and folder sync  (designed)

To share a file you publish a tiny **manifest** — a signed note that says "this
file is called X, it's this big, and here are the fingerprints of its pieces." The
manifest's ID is a shareable link (think magnet link or QR code). Anyone who has
that link can then collect the pieces from *whoever happens to have them*, checking
each piece as it arrives. Point the same machinery at a folder and you have
Syncthing; aim it at a popular file and you have BitTorrent.

<details>
<summary>Deep dive: manifests, swarming, and encrypted folders</summary>

```
manifest = SIGNED {
    tag       = 0x01
    name, mime
    total_len, chunk_size, count
    merkle_root        // hash tree over the chunks
}
```

The manifest's 16-byte ID is the magnet handle — shareable as `spore:<hexid>`, as
`~S1.…~` armor, or as a QR. Holding a manifest lets a node:

- **verify** each chunk against `merkle_root` before committing it — integrity on
  an actively hostile link;
- **swarm** it — pull only the chunks it lacks, from any peer that has them,
  turning INV/WANT into object-scoped reconciliation ("I hold indices {…} of object
  O" / "send me {…}"). Resumable, multi-source, out-of-order — BitTorrent from the
  fragment primitive.

**Folder sync (Syncthing):** each file becomes a manifest; manifests flood on a
folder-topic; followers swarm them; a newer signed manifest for the same `name`
supersedes the old. An **encrypted folder** is just sealed manifests behind a
pre-shared-key topic (§7). One mechanism yields both torrent and sync.

Proposed API:

```rust
let magnet = node.publish_file("photo.jpg", bytes);  // -> manifest id
node.fetch_file(magnet).await;                        // swarm, verify, return
```
</details>

## Layer 3 — sessions: a UDP-like link, and SSH over it  ✅ implemented

Sometimes you want a live back-and-forth, not a one-shot message — a chat, a
remote shell, a game. The session layer gives you a **UDP-like link** between two
addresses: fire packets, they arrive best-effort, either side can talk. Because the
"address" is a cryptographic identity rather than an IP, the link doesn't break
when you switch WiFi to cellular — it just keeps going, the way Mosh survives
roaming. On top of that link you can turn on a **reliable, in-order stream** when
you need one (for SSH, git, file copies) — that reliability lives entirely at the
two ends, so the network stays dumb.

<details>
<summary>Deep dive: datagrams, replay protection, and Go-Back-N reliability</summary>

**Datagram (`tag 0x04`).** A datagram is an ordinary unicast envelope whose payload
is `[0x04][port:2][seq:8][sealed_bytes]`, signed by the sender and sealed to the
peer's prekey. `Node::dial(peer, port)` returns a `Session` — pure local state, no
handshake, because identity *is* the address. `dg_send` emits one; `dg_recv`
verifies the sender (its key must hash to the session peer), decrypts, and runs a
DTLS-style 64-wide replay window. The first datagram floods to discover a route;
the signed reply teaches the reverse path (§5's "first copy wins"), so the rest go
directed. Roaming and NAT rebinding stop being special cases — the address is
unchanged, the path just re-learns.

**Reliable stream (QUIC-style, kept simple).** `Node::reliable(session)` wraps a
session in a **Go-Back-N** ARQ:

- the sender streams `[F_DATA][offset:8][len:2][bytes]` frames within a fixed
  window and, on an ACK-progress timeout (`poll(now)`), rewinds to the last acked
  offset and resends;
- the receiver accepts only in-order bytes and cumulatively ACKs the next offset it
  needs (`[F_ACK][recv_next:8]`).

No AIMD, no fancy congestion control — a fixed window and a fixed retransmit
timeout, on purpose. It's enough to carry an ordered byte stream, and it's
**endpoint state only**, so it never reintroduces stream semantics into relays —
exactly how QUIC rides UDP.

**SSH specifically.** Stock `ssh` needs a reliable byte stream, so it rides the
reliable shim directly (great on a good link, slow on a bad one — physics, not a
bug). The better fit is the **Mosh model**: use SSH only for the initial key
exchange, then run the interactive session as *state-sync* over raw datagrams —
roaming, loss-tolerant, and instant-feeling with local/predictive echo.

**The honest limit.** Interactivity tracks the real path RTT. On a LAN, WireGuard
tunnel, or direct radio link it's Mosh-grade; over a multi-hop opportunistic path
the datagrams still flow but degrade to store-and-forward. The abstraction is
uniform; the *experience* follows the physics of the link. That's the right
behaviour: interactive when it can be, still delivering when it can't.

Implemented in `src/lib.rs` (`session::Session`, `session::Reliable`,
`Node::dial`/`dg_send`/`dg_recv`/`reliable`) with tests for datagram
roundtrip/replay/wrong-recipient and a reliable stream reassembling across 30%
loss.
</details>

## Layer 4 — services: HTTP over SPORE  (designed)

This is the "nobody learns anything new" layer. A service is just a request/
response endpoint that speaks HTTP — the same `method`, `path`, `headers`, `body`,
and status codes every web developer already uses. You write an ordinary handler;
a thin adapter carries it over SPORE. And because responses are stored by every
relay that carries them, popular results end up cached across the whole network for
free.

<details>
<summary>Deep dive: request/response, bridging the web, and the free CDN</summary>

- A **service** is a topic or address a node serves.
- A **request** (`tag 0x02`) carries `method`, `path`, `headers`, `body` — sealed to
  the service's prekey, optionally signed or anonymous, with a request-id nonce to
  correlate the reply.
- A **response** (`tag 0x03`) goes back to the requester's address; the reverse path
  was learned automatically when the request arrived. Bodies of any size ride
  Layer 1.

Why this shape wins:

1. **Build for SPORE without learning SPORE.** Write a normal `(req) -> res`
   handler; an adapter maps a SPORE request envelope to your existing Express/
   Flask/axum handler and back. The client call is `fetch`-shaped:
   `spore.fetch("spore://<addr>/path", { method, body }) -> Response`.
2. **Bridge the existing web for free.** A gateway proxies SPORE requests to
   `http://localhost:PORT` and back, so any IP-reachable service becomes reachable
   over LoRa/paper/sneakernet with zero rewrite — and vice versa.
3. **Free CDN.** Responses are content-addressed envelopes every relay already
   stores. Mark a `GET` cacheable and any node that carried it can answer a later
   identical request — the store is the cache, INV/WANT is the cache-fill.
   Store-and-forward request/response is a CDN by construction.

Proposed API:

```rust
node.serve(topic("weather"), handler);          // handler: Request -> Response
let res = node.request(addr, Request { .. }).await;
```
</details>

## Layer 5 — feeds: pub/sub  (works via topics)

A feed is just a topic. Whoever wants to broadcast sends to the topic; whoever
cares follows it and gets everything. It's the signed-gossip model behind Nostr,
minus the JSON, the dedicated relays, and the always-on internet.

<details>
<summary>Deep dive: retention and backfill</summary>

Publishers `send(topic, event)`; subscribers follow the topic and receive each
event through the normal deliver path. Retention is just message expiry, and a late
joiner backfills history from any peer's store via INV/WANT. No special
infrastructure — a feed is emergent from topics plus the store-and-sync the router
already does. An ergonomic wrapper (subscribe callbacks, watermark cursors) is the
only thing left to add.
</details>

## Status at a glance

The transport and the first pieces of the application layer exist and are tested;
the rest is designed and slots into the same tag/endpoint model.

<details>
<summary>Deep dive: what's built vs. next</summary>

| Layer | State |
|---|---|
| Transport (envelope, router, sync, seal, KISS, armor) | ✅ in `src/lib.rs` |
| L1 objects — `send()` auto-fragment + reassembly | ✅ implemented |
| L3 sessions — datagram link + reliable stream | ✅ implemented |
| L2 files — manifest, magnet, swarm, folder-sync | ▢ designed |
| L4 services — SPORE-HTTP request/response + gateway | ▢ designed |
| L5 feeds — pub/sub over topics | ◑ works via topics; ergonomic API pending |

Every layer is additive and endpoint-only. The transport underneath never learns
they exist — which is exactly why a 200-byte LoRa link, a QR code, or a human
reading armor aloud remains a first-class peer for all of them.
</details>
