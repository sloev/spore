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
- **Double Ratchet** (§7) — forward-secret sessions: a fresh per-message key from a
  BLAKE2b chain, X25519 ratchet turns on each reply, skipped-key caching for
  out-of-order arrival, replay rejection. X25519 + ChaCha20-Poly1305.
- **Mix mode** (§9) — onion routing as nested sealed envelopes through 2–3 mixes,
  padded to size classes; sender + recipient anonymity, each mix peels one layer.
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
cargo test               # 19 tests: envelope, fountain, send, files, sessions, reliable, ratchet, onion, bridges, seal, KISS, armor
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
[8] MIX onion A~>D via 2 mixes (B,C): D opens Some("burn the ledgers")
```

## Routing across other networks

SPORE treats a whole underlay network — a Meshtastic mesh, a Reticulum network, an
IP subnet — as a **single link**, however many hops it uses inside. To go from node
A to node B on the same mesh, A hands the envelope to that mesh (usually one
broadcast) and the mesh's *own* routing carries it to B; SPORE's hop counter drops
by just one for the entire crossing. If A and B are on *different* networks, a
gateway node that sits on both passes the envelope from one to the other — and that
is where a real SPORE hop happens. The envelope's cryptographic address says *who*
it's for (end-to-end, unchanging); each network's native addressing says *how* to
move the bytes across that one medium. So SPORE routes **between** networks and each
network routes **within** itself — not much stranger than IP over Ethernet.

<details>
<summary>Deep dive: two address spaces, gateways, and the ARP analogy</summary>

**Two independent address spaces.**
- *Who* — the SPORE address (`SHA-256(pubkey)[..8]`) or a topic. End-to-end,
  cryptographic, identical on every medium. Like an overlay/host identity.
- *How* — the underlay's native addressing: a Meshtastic node number, a Reticulum
  destination hash, an IP:port. Local to one link, meaningful only there. Like a
  MAC address.

A **bridge** owns exactly one interface (`Iface`) and translates between the two: it
turns a `Forward` from the router into an underlay send, and turns received underlay
frames into `node.on_rx(bytes, iface, nbr, now)`. The router never learns underlay
addresses — that's the bridge's job, the way an OS's ARP/neighbour table maps
IP→MAC.

**An underlay with its own routing = one SPORE interface.** Meshtastic, Reticulum,
Yggdrasil, BATMAN, Tor, plain IP — each already delivers bytes across many physical
hops. SPORE hands it *one* frame and lets it do that; the underlay's internal hops
are invisible and free. SPORE decrements `hops` exactly **once** for the whole
crossing (a point-to-point backbone link may even *restore* the hop so long hauls
don't burn the 16-hop budget).

**Same network (A and B on one mesh).** A's bridge injects the envelope as an
underlay broadcast (Meshtastic: portnum 256, dst `0xFFFFFFFF`; Reticulum: a PLAIN
`spore`/`v1` destination; UDP: multicast/broadcast). The underlay floods it to every
node; the ones running SPORE call `on_rx`, and B delivers because the `dest` matches
it (or a topic it follows, or public). One SPORE hop, N invisible mesh hops.

**Different networks (A on Meshtastic, B on Reticulum).** A **gateway** node C runs
both bridges. C receives on its Meshtastic interface and the router emits
`Forward::Flood { except: meshtastic_iface }` — send out *all other* interfaces — so
C re-injects onto Reticulum, which carries it to B. SPORE hops count the **gateways
between networks**, not the hops inside them. This is exactly IP routing: routers
between subnets, switching within them.

**Directed vs. flooded.** With no routing state (T0/T1), SPORE just floods on each
interface; dedup and expiry keep it sane, and B is reached because it's on a
reachable underlay. With paths (T2, §4 "first copy wins"), when B's signed envelope
or ANNOUNCE arrives via a given interface A records "B is reachable via *that*
interface" and sends future unicast there — the bridge may resolve it to a specific
underlay address from its own learned `SPORE-addr ↔ underlay-addr` table, or just
broadcast (dedup makes the extra reach harmless).

**The IP analogy, lined up:**

| SPORE | IP world |
|---|---|
| SPORE address (who) | IP address / hostname |
| underlay address (how) | MAC address |
| bridge (per interface) | NIC driver + ARP |
| underlay-with-routing (Meshtastic / Reticulum / IP) | one L2 segment / switched LAN / tunnel |
| gateway node (2+ interfaces) | IP router |
| SPORE hop | IP hop (router-to-router) |
| flood + dedup | broadcast + drop-duplicates |

</details>

## Layout

| Path            | What                                               |
|-----------------|----------------------------------------------------|
| `src/lib.rs`    | the portable core: envelope, fountain, `send`, files, sessions, ratchet, mix, router, crypto, bridges |
| `src/main.rs`   | reference node + demo + UDP / HTTP-bag / folder bridge runners |
| `docs/SPEC.md`  | the one-page SPORE v1 specification                 |
| `docs/DESIGN.md`| the layers above transport: files, sessions, RPC, feeds, ratchet, mix, bridges |

## License

Public domain — released under [The Unlicense](LICENSE). No restrictions.
