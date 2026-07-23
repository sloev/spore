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

## Every service is one of four patterns

Almost every service you'd want maps onto one of four familiar **patterns**, and
each one corresponds to a mechanism SPORE already has or can express as a payload
convention. If you've built web apps, you already know all four. (These are
application *patterns* — distinct from the five medium *shapes* a bridge binds to,
below.)

<details>
<summary>Deep dive: the four service patterns and their web analogues</summary>

| Pattern | Web analogue | SPORE mechanism | State |
|---|---|---|---|
| **Objects** | files / magnet links | content-addressed blobs, fountain + swarm | ✅ implemented |
| **Sessions** | UDP / QUIC / Mosh | datagram links + reliable stream | ✅ implemented |
| **Request/response** | HTTP | `Request`/`Response` envelopes, reverse-path routed | ✅ implemented |
| **Feeds** | pub/sub, SSE | topics | ✅ implemented |

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
| `0x01` | file manifest | ✅ implemented |
| `0x02` | request (RPC) | ✅ implemented |
| `0x03` | response (RPC) | ✅ implemented |
| `0x04` | datagram (session) | ✅ implemented |
| `0x05` | feed/event | ✅ implemented |
| `0x06` | receipt/ACK (spec §8) | ✅ implemented |
| `0x07` | file chunk | ✅ implemented |
| `'O'` (0x4F) | mix onion (spec §9) | ✅ implemented |

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

## Layer 2 — files: magnet links, torrents, and folder sync  ✅ implemented

To share a file you publish a tiny **manifest** — a signed note that says "this
file is called X, it's this big, and here are the fingerprints of its pieces." The
manifest's ID is a shareable link (think magnet link or QR code). Anyone who has
that link can then collect the pieces from *whoever happens to have them*, and each
piece verifies itself as it arrives. Point the same machinery at a folder and you
have Syncthing; aim it at a popular file and you have BitTorrent.

<details>
<summary>Deep dive: content-addressed chunks, swarming, and folders</summary>

A file is split into chunks; each chunk is an ordinary envelope
`[0x07][file_id:16][index:4][bytes]`, so it has its own content ID (the hash of its
bytes). The **manifest** is a signed envelope listing those chunk IDs in order:

```
manifest = SIGNED [0x01][file_id:16][chunk_size:4][count:4][total_len:8][name][chunk_id:16 × count]
```

The manifest's own 16-byte ID is the **magnet** — shareable as `spore:<hexid>`, as
`~S1.…~` armor, or as a QR.

- **Integrity is free.** The manifest is signed, so its chunk-ID list is authentic;
  and a chunk envelope's ID *is* the hash of its bytes. A node counts a chunk as
  present only when its store holds an envelope whose ID matches a manifest-listed
  ID — so a forged or corrupt chunk simply never matches. No separate hash list or
  merkle proof needed.
- **Swarming is just WANT.** Only the small manifest floods; the data is pulled.
  `fetch(magnet)` emits a WANT for the chunk IDs it lacks, and any peer holding them
  answers from its store (the existing §6 machinery, untouched). Multi-source and
  resumable, because chunks are named by content, not by origin.

Implemented in `src/lib.rs` as `publish_file` / `absorb_manifest` (auto-called on
delivery) / `fetch` / `has_file` / `file_bytes`, covered by a
publish → flood → pull → verify test.

```rust
let (magnet, forwards) = node.publish_file("photo.jpg", bytes, dest, now);
// … peer learns the manifest, then:
let forwards = node.fetch(&magnet);   // pull missing chunks; verify on arrival
let file = node.file_bytes(&magnet);  // Some(bytes) once complete
```

**Folder sync (Syncthing) ✅ implemented:** `bridge::foldersync::publish_dir` turns
each file in a directory into a manifest on a folder-topic; a subscriber `fetch_all`s
the chunks and `materialize`s the completed files back to disk (newest manifest per
name wins, path-traversal guarded). An **encrypted folder** is sealed manifests
behind a pre-shared-key topic (§7). Tested end-to-end: publish → flood → pull →
materialise.

**Known caveat.** Chunks live in the ordinary store, which evicts under pressure
(lowest-stamp → largest → oldest). Pinning/custody for in-progress downloads is
future work.
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

## Layer 4 — request/response: RPC as a convention  ✅ implemented

Sometimes you want to ask a question and get an answer — a lookup, a form
submission, an API call. That's just two messages with a convention: a request
envelope carrying `method`/`path`/`body`, and a response envelope carried back
along the path the request came in on. It resembles HTTP because request/response
*is* that shape — but HTTP itself is only a bridge (below), never part of this. The
pattern is transport-agnostic: the same request works over LoRa, a folder, or a QR
code.

<details>
<summary>Deep dive: the request/response convention and its free CDN</summary>

- A **service** is a topic or address a node serves.
- A **request** (`tag 0x02`) carries `method`, `path`, `body`, sealed to the
  service's prekey with a request-id nonce; a **response** (`tag 0x03`) goes back to
  the requester's address along the reverse path SPORE learned when the request
  arrived (§5, "first copy wins"). Large bodies ride Layer 1.
- **Build it like a normal handler.** A service is a `(request) -> response`
  function; the SPORE side is a thin adapter, so nobody learns a new programming
  model. To reach the *existing* web, run an HTTP bridge that proxies a SPORE
  request to a local `http://…` service and the reply back — the bridge moves
  bytes, the web app is untouched.
- **Free CDN.** Responses are content-addressed envelopes every relay stores. Mark
  a read-only response cacheable and any node that carried it can answer a later
  identical request — the store is the cache, INV/WANT is the cache-fill.

Implemented in `src/lib.rs`: the router auto-queues delivered requests and matches
responses to their pending nonce, so the app loop stays tiny. Tested with a
request → response round-trip.

```rust
// client
let (id, forwards) = node.request(service, rpc::Request { method, path, body }, now);
// … later, after the reply is delivered:
let resp = node.take_response(id);               // Some(Response { status, body })

// service
for (from, id, req) in node.poll_requests() {
    let forwards = node.respond(from, id, rpc::Response { status: 200, body }, now);
}
```
</details>

## Layer 5 — feeds: pub/sub  ✅ implemented

A feed is just a topic. Whoever wants to broadcast sends to the topic; whoever
cares follows it and gets everything. It's the signed-gossip model behind Nostr,
minus the JSON, the dedicated relays, and the always-on internet.

<details>
<summary>Deep dive: subscribe / publish / poll, retention and backfill</summary>

`subscribe(topic)` follows a feed, `publish(topic, event)` floods a tagged event
(`0x05`), and the router auto-queues delivered events for `poll_feed` to drain as
`(topic, from, data)`. Retention is just message expiry, and a late joiner backfills
history from any peer's store via INV/WANT — no special infrastructure; a feed is
emergent from topics plus the store-and-sync the router already does. Tested with a
publish reaching a subscriber.

```rust
node.subscribe("alerts");
let forwards = node.publish("alerts", event, now);
for ev in node.poll_feed() { /* ev.topic, ev.from, ev.data */ }
```
</details>

## Forward-secret sessions — Double Ratchet  ✅ implemented

The sealed box (§7 baseline) already gives you privacy, but if a device is seized
its prekey can read every message sent to it that week. The Double Ratchet fixes
that: each message uses a brand-new key that's thrown away immediately, and the
whole key schedule "turns" every time the other side replies. Crack one message and
you learn nothing about the ones before it or after the next turn.

<details>
<summary>Deep dive: chains, turns, and out-of-order handling</summary>

Two chains and a root. A BLAKE2b **chain KDF** produces one message key per message
and then advances, so every message key is unique and deleted after use (forward
secrecy within a chain). When a message carries a new **X25519 ratchet public** in
its header, both sides run the **root KDF** over a fresh DH — a *ratchet turn* that
reseeds the chains with new entropy (recovery from compromise). The wire form is the
spec's `[dh_pub:32][n:2][pn:2][ct]`, with `ct = ChaCha20-Poly1305(mk, nonce=n,
ad=header)`.

Out-of-order arrival is normal in SPORE, so a receiver caches the keys it skips
(bounded by `MAX_SKIP`) and opens the stragglers when they arrive; a replay of an
already-consumed message is refused. In `src/lib.rs` as
`ratchet::Ratchet::{init_alice, init_bob, encrypt, decrypt}`, tested through a DH
turn, out-of-order delivery, and a rejected replay. In practice the root bootstraps
from the first sealed message and each device keeps its own ratchet key — never
share one ratchet across devices.
</details>

## Anonymity — mix mode  ✅ implemented

If you need to hide *who is talking to whom* (not just the contents), wrap your
message in an onion: layers of encryption addressed to a chain of volunteer relays
("mixes"). Each mix can peel only its own layer, learning just the next hop, so no
single relay sees both ends. Make the innermost layer public and everyone carries it
while only the true recipient can open it.

<details>
<summary>Deep dive: onion construction, padding, and what it defeats</summary>

An onion is nested sealed envelopes. Each layer is a DATA envelope addressed to one
mix, its payload sealed to that mix as `'O' ‖ next_full_envelope`. `mix::onion_wrap`
builds it inside-out through 2–3 mixes (learned from ANNOUNCEs on topic `mix`) and
pads each layer to a **256 / 1024 / 4096-byte size class** so onion depth never
shows on the wire. A mix runs `Node::onion_peel` — open, check the `'O'` marker,
re-inject the inner envelope as its own traffic.

- **Sender anonymity:** outer layers are left unsigned.
- **Recipient anonymity:** the innermost `dest` is `0x00..00` and its payload is
  sealed to the recipient — every node carries it, one node opens it.

Honest limits (from the spec): this beats local observers and any subset of mixes,
but a *global* passive observer is only defeated while cover traffic flows. The
batching core is implemented — `mix::Batch` holds peeled inner envelopes and
releases them only once a minimum batch has gathered *and* each item's delay has
elapsed, breaking the arrival/re-send timing link. The remaining timing policy
(Poisson delays, decoy onions) is the mix *runner's* job. Tested: the onion peels
cleanly through two mixes, and the batch queue holds until full-and-due.
</details>

## Encrypted topics (§7)  ✅ implemented

A public topic is readable by anyone who follows it. An *encrypted* topic adds a
shared password: members seal every message under a pre-shared key, so the mesh
still carries and stores the traffic but only key-holders can read it.

<details>
<summary>Deep dive: topic seal/open and key rotation</summary>

`topic_seal(msg, psk)` / `topic_open(ct, psk)` use XChaCha20-Poly1305 with a 32-byte
pre-shared key and a random 24-byte nonce prefixed to the ciphertext (spec §7). The
envelope rides a normal topic with `ENCRYPTED` set; relays are oblivious, endpoints
with the key decrypt. **Key rotation** stays a documented convention: flood a
`KEYROT` carrying the new key, signed by the old one, so members roll forward while
outsiders can't. Tested: a key-holder round-trips, a wrong key fails.
</details>

## Delivery receipts (§8)  ✅ implemented

Flood-and-forget is fine for most mail, but sometimes you want to *know* it arrived.
Set one flag and the recipient automatically floods a tiny signed receipt back to
you; when it returns, your node marks the message delivered. If no receipt comes,
the sender re-floods on a backoff schedule until one does or it gives up.

<details>
<summary>Deep dive: the ACKREQ round-trip and backoff resend</summary>

`originate_ackreq(dest, payload)` sends a unicast DATA with the `ACKREQ` flag and
remembers it. When such a message is delivered to one of our own addresses, the
router floods a signed receipt — `[0x06][orig_id:16]` — back to the sender; being a
signed envelope it also teaches reverse paths (§4). The sender absorbs the receipt,
`acked(id)` flips true, and the pending entry clears.

`resend_unacked(now)` re-floods any still-unacked message whose `Backoff` timer has
elapsed (§5.6: flooding *is* route discovery, so a resend can find a path a
blackhole was hiding), giving up after `Backoff::MAX`. Tested with an end-to-end
ACKREQ → receipt round-trip. Known simplification: a *lost receipt* isn't re-
requested — a duplicate of the original is deduped before it could re-trigger one.
</details>

## Congestion control (§5.4)  ✅ implemented

The one hard feature that touches traffic, kept as four small independent knobs the
originator and bridges apply — the router itself stays dumb. Cap how much you relay,
slow your beacons when nothing's new, back off when a peer is swamped, and retry
lost mail with growing gaps.

<details>
<summary>Deep dive: the four knobs</summary>

All four live in `congestion` as plain primitives:

- **(a) Token bucket** (`TokenBucket`, `ten_percent`) — caps relayed bytes to a
  sustained rate; on ISM bands the law is ≤ 10 % airtime, and dedup makes a dropped
  relay harmless. The TCP bridge gates every relay through one.
- **(b) Trickle** (`Trickle`) — the HELLO/ANNOUNCE interval doubles from 5→80 min
  while nothing new is heard and snaps back to the minimum on novelty, so a quiet
  mesh goes quiet. The TCP bridge paces its beacon with one.
- **(c) Backpressure** (`admit`) — a peer advertises a `busy` byte (store fill) in
  its ANNOUNCE; neighbours admit sends with probability (255−busy)/255 and always
  let stamped (proof-of-work) mail through. `Node::busy` produces it, `peer_busy`
  reads a neighbour's.
- **(d) Exponential backoff** (`Backoff`) — FLOOD retries at 30 s, doubling, capped
  at 1 h, at most 5 attempts; powers receipt resend above.

Tested as primitives; (a) and (b) are wired live into the TCP bridge, (c) into the
ANNOUNCE busy byte, (d) into receipts.
</details>

## Bridges & bindings — SPORE rides everything

A bridge is *not part of the protocol*. It only moves envelope bytes in and out of
a node. Every medium on Earth has one of five shapes (spec Page 2), and you bind by
shape — the router never changes. HTTP, a serial cable, a folder on a USB stick are
all bridges, none more special than another.

<details>
<summary>Deep dive: the five shapes and what's implemented</summary>

| Shape | Examples | Binding | Status |
|---|---|---|---|
| 1. Message pipe | UDP, WebRTC, LoRa, Meshtastic | one envelope per message | ✅ UDP + Meshtastic WiFi-UDP |
| 2. Byte stream | TCP, serial, RFCOMM, KISS TNCs | KISS framing (`bridge::KissStream`) | ✅ framer + TCP runner |
| 3. Text channel | SMS, email, Usenet, paper, voice | `~S1.…~` armor (`armor::wrap`) | ✅ codec |
| 4. Shared bus | walkie-talkie, CB, ham FM | KISS + CSMA + CRC tail | ✅ `Csma` damping + `crc_*` (AFSK runner needs a sound card) |
| 5. Shared store | folder/USB/Syncthing, HTTP bag, BBS | `bridge::store`, `bridge::bag` | ✅ folder + HTTP bag |

- **HTTP bag** (`bridge::bag`, `cargo run -- http`): `POST /spore/push`,
  `GET /spore/inv`, `POST /spore/want`, MIME `application/x-spore`. HTTP is one bag
  transport; a folder or a pastebin serves the same three ops.
- **Folder** (`bridge::store`, `cargo run -- folder DIR`): envelopes are files named
  `<hexid>.spore`; the folder *is* a persistent INV. Drop it in Syncthing or on a
  USB stick and two folders become one link.
- **KISS stream** (`bridge::KissStream`): a stateful de-framer for byte streams, so
  a frame split across reads still reassembles. The `tcp` runner frames envelopes
  over a TCP stream and shows congestion control live — Trickle-paced beacons and
  token-bucket-gated relays.
- **Armor** (`armor::wrap` / `unwrap`): Base32 text you can paste, print, or read
  aloud.
- **`Neighbors<U>`** (`bridge::Neighbors`): the shared address resolver every bridge
  uses — SPORE's ARP/NDP. `U` is the medium's own peer name (`SocketAddr`, MAC,
  Meshtastic `u32`, connection handle, or `()`). Learned by snooping signed frames;
  `resolve` turns a directed send into an underlay unicast, else the bridge
  broadcasts. Wired into the UDP bridge; the README has the full per-medium table.
- **`Csma` + `crc_append`/`crc_check`** (`bridge`): shared-bus damped flooding
  (listen-before-talk, cancel on overhearing) and the SHA-256[0:4] CRC tail buses
  need. Drop-in for a walkie-talkie/AFSK runner.
- **`foldersync`** (`bridge::foldersync`): publish a directory as manifests and
  materialise fetched files — Syncthing over SPORE.
- **`meshtastic`** (`bridge::meshtastic` + the `meshtastic` runner): wrap envelopes
  as Meshtastic packets (portnum 256) over the WiFi-UDP broadcast group so a LoRa
  mesh carries SPORE. Portable hand-rolled protobuf codec; a template pending
  hardware validation (field numbers, multicast values, and channel encryption).

Underlays with their own routing (an IP network, Reticulum, a VPN) count as *one*
interface: decrement hops once and let them handle their own delivery.
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
| L2 files — manifest, magnet, swarm-by-WANT, folder sync | ✅ implemented |
| L3 sessions — datagram link + reliable stream | ✅ implemented |
| L4 request/response — RPC convention | ✅ implemented |
| L5 feeds — pub/sub over topics | ✅ implemented |
| §7 crypto — Double Ratchet + encrypted topics | ✅ implemented (KEYROT convention) |
| §8 receipts — ACKREQ + signed receipt + backoff resend | ✅ implemented |
| §9 anonymity — mix-mode onion + batch timing | ✅ implemented (Poisson/decoys = runner policy) |
| §5.4 congestion — backoff, Trickle, token bucket, busy-byte | ✅ implemented |
| Bridges — UDP, TCP/KISS, HTTP bag, folder+sync, CSMA/CRC, `Neighbors` | ✅ implemented (radio runners need hardware) |

Every layer is additive and endpoint-only. The transport underneath never learns
they exist — which is exactly why a 200-byte LoRa link, a QR code, or a human
reading armor aloud remains a first-class peer for all of them.
</details>
