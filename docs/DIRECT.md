# SPORE Direct

A **negotiated, non-routed, end-to-end encrypted datagram pipe** between two SPORE
identities over a direct underlay — UDP, TCP, BLE, ESP-NOW. It exists for the one
job the store-and-forward mesh is wrong for: full-duplex, low-latency media (voice,
telemetry, a live terminal), where holding a frame for opportunistic relay would
add exactly the latency the app is trying to avoid.

Direct is an **application profile**, not a change to the protocol. It adds nothing
to the wire: the envelope, store, hub, router, and the frozen v1 contract are
untouched. The only thing that crosses the mesh is opaque `SPDR` signalling, and it
rides the ordinary sealed-and-signed unicast path (`Node::send_direct`). Everything
in `src/direct.rs` is pure — no sockets, no `Node` dependency — so it is fully
unit-tested and compiles everywhere the core does, wasm included.

## The two planes

| | Mesh (v1) | Direct |
|---|---|---|
| Delivery | store-and-forward, multi-hop, opportunistic | one hop, straight over an underlay |
| Latency | seconds to days | the link's own RTT |
| Good for | messages, files, feeds, async | voice, telemetry, interactive streams |
| Reliability | fountain-coded, retried | best-effort datagram (app adds order/retry *above* the record) |
| On the wire | signed envelopes (frozen) | `SPDR` signalling on the mesh; AEAD records on the underlay |

Direct does **not** route its records through the mesh, does not change any T0–T2
relay behaviour, and does not try to guarantee latency on duty-cycled radios — a
LoRa candidate is simply never offered for a voice pipe.

## Handshake

Three messages, all carried as `SPDR` payloads over `send_direct` (so each is
sealed to the peer's prekey and signed by the sender — which is what binds an
offer's ephemeral key to a real SPORE identity):

1. **OFFER** — initiator → responder: a fresh 16-byte `pipe_id`, both addresses,
   what the app needs (`min_bps`, `mtu_needed`, optional `max_latency_ms`), the
   initiator's ephemeral X25519 public key, and the candidate paths it can be
   reached on.
2. **ANSWER** — responder → initiator: either **Ok** with a chosen candidate and
   its ephemeral public key, or **Reject** with a reason (`no_medium`,
   `throughput`, `busy`).
3. **CLOSE** / optional **REKEY** — on the mesh plane only (a later increment).

## Medium selection

The responder keeps only the candidates whose medium it is willing to use *and*
that meet the initiator's `min_bps` and `mtu_needed`, then ranks by latency hint
(lower first), breaking ties by capacity. If candidates overlapped but none were
big/fast enough it answers `throughput`; if none overlapped, `no_medium`.

| medium | e2e? | est. bps | mtu | notes |
|---|---|---|---|---|
| UDP | yes | high | ≥1200 | one datagram = one record |
| TCP | yes | high | ≥1400 | `u16be len ‖ record` framing |
| BLE | yes | low–med | 20–200 | may chunk below the record |
| ESP-NOW | yes | ~200–500 kb | ~250 | optional adapter |

## Key schedule

```
shared = X25519(eph_initiator, eph_responder)
(k0, k1) = BLAKE2b-512( shared ‖ "spore-direct-v1" ‖ pipe_id
                        ‖ initiator_addr ‖ responder_addr ‖ medium )
initiator: tx = k0, rx = k1
responder: tx = k1, rx = k0
```

Both SPORE addresses, the `pipe_id`, and the chosen medium are bound into the KDF,
so a record only opens for the exact pair, pipe, and link that negotiated it. The
media keys never appear on the wire. BLAKE2b is the KDF here for the same reason
the ratchet uses it — one vetted hash construction across the codebase, rather than
pulling in a second — not the sketch's HKDF-SHA256.

## Record format (identical on every medium)

```
offset  size  field
0       1     ver = 1
1       1     type  (0 MEDIA, 1 KEEPALIVE, 2 CONTROL, 3 DATA, 4 STREAM)
2       2     seq   u16 BE   (the AEAD nonce)
4       4     pipe_id[..4]   (demux hint; full binding is in the key)
8       n     ChaCha20-Poly1305(ciphertext ‖ tag)
```

Bytes `0..8` are authenticated as AEAD associated data, so a flipped `type`, `seq`,
or pipe prefix fails the peer's MAC. The pipe is **best-effort datagram**: a
dropped or reordered record is the app's to handle — an ordered `STREAM` or RPC
retry lives *above* this record, so a lost media frame never head-of-line-blocks a
voice call.

- **UDP / ESP-NOW:** one packet = one record.
- **TCP / serial / BLE:** `u16be length ‖ record` (BLE may chunk further).

## Threat model

- **Confidentiality / integrity of media** — the AEAD record. A record only opens
  under keys derived from the ephemeral DH bound to both identities, the pipe, and
  the medium; a forged or corrupted datagram fails the MAC and is dropped, not
  surfaced to the app (`Pipe::poll` skips it rather than tearing the pipe down).
- **Binding the ephemeral key to an identity** — done by the outer `send_direct`
  envelope: the OFFER/ANSWER are signed by the sender's SPORE key, so a
  man-in-the-middle cannot substitute its own `eph_pub` without also forging that
  signature.
- **Replay** — `seq` is the nonce; an app that cares rejects a stale/duplicate seq
  above the record (a sliding window), the same shape the datagram session layer
  uses. Best-effort media typically doesn't bother.
- **Metadata** — the underlay sees two endpoints exchanging encrypted datagrams;
  it learns the medium locators (an `ip:port`), which is inherent to going direct.
  Peers that don't want to reveal a direct address simply don't offer a candidate
  and stay on the mesh.
- **Out of scope** — latency guarantees on duty-cycled radios; a
  feature-complete PBX; routing direct records through the mesh.

## API

```rust
use spore::direct::{Pipe, Offer, Answer, Need, Candidate, Medium, RecordType, DatagramPort};

// Initiator: build an OFFER, send its bytes over the mesh (send_direct), keep `pending`.
let (offer_bytes, pending) = Pipe::<MyPort>::offer(my_addr, peer_addr, pipe_id, need, candidates);

// Responder: on the OFFER, pick a medium, open the chosen adapter, answer.
let offer = Offer::decode(&offer_bytes).unwrap();
let (answer_bytes, resp_pipe) = Pipe::answer(&offer, my_addr, &[Medium::Udp], my_port);

// Initiator: on the ANSWER, open the chosen adapter and finish.
let answer = Answer::decode(&answer_bytes).unwrap();
let mut pipe = Pipe::finish(pending, &answer, my_port).unwrap();

// Then, best-effort, both ways:
pipe.send(RecordType::Data, b"...")?;
while let Some((ty, bytes)) = pipe.poll() { /* … */ }
```

A `DatagramPort` is whatever the chosen medium's adapter provides — `mtu`, `send`,
`try_recv`, driven by polling (the same model the JNI bridges use). The crate ships
an in-memory `Loopback` for tests and for wiring two in-process peers (see
`examples/direct_loopback.rs`), plus two real socket adapters described next.

## Socket adapters

`direct/udp.rs` and `direct/tcp.rs` implement `DatagramPort` over real sockets.
Both use `std::net`, so both are gated `#[cfg(not(target_arch = "wasm32"))]`; the
negotiation core above still compiles for the web.

- **`UdpPort`** — one datagram carries exactly one sealed record, so there is no
  framing to add. The socket is *connected* to the negotiated peer: not a security
  boundary (the record MAC is), but a cheap kernel-level filter that drops stray
  packets before they cost a decrypt. `UdpPort::connect` binds and connects in one
  step; `UdpPort::from_socket` wraps a socket that already carried the out-of-band
  signalling. A datagram larger than the receive buffer is truncated and simply
  fails the MAC — the correct "drop it" outcome.
- **`TcpPort`** — TCP is a byte stream, so each record is written with a 4-byte
  big-endian length prefix and reads accumulate until a whole record is present,
  restoring the one-`send`-one-record shape. Nagle is disabled (latency is the
  point) and the stream is non-blocking with a small out-buffer, so neither `send`
  nor `try_recv` ever blocks the poll loop. A length prefix larger than any record
  could legitimately be is treated as a corrupt or hostile stream: the buffer is
  dropped rather than grown toward the claim, closing an unbounded-buffering DoS.

Both keep the record layer **best-effort** — the pipe's sequence numbers and
per-record AEAD do not assume in-order, gap-free delivery, so layering an ordered
stream on top (which would reintroduce head-of-line blocking) is deliberately
avoided even on TCP.

## Status

The **pure protocol core** landed first: the `SPDR` negotiation codec, the key
schedule, the AEAD record, medium selection, the `DatagramPort` trait, the
`Loopback` transport, and the `Pipe` — all unit-tested end to end. This increment
adds the **real UDP and TCP socket adapters** above, exercised against real kernel
sockets — including a genuine two-process UDP round-trip that re-execs the test
binary and negotiates a live pipe across the process boundary. Still to come: the
mesh signalling glue that carries `SPDR` over `send_direct`, BLE/ESP-NOW adapters,
and `CLOSE`/`REKEY`; those add transport and lifecycle, not protocol.
