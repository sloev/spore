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

Direct does not try to guarantee latency on duty-cycled radios: a LoRa candidate
is simply never offered for a voice pipe.

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
The handshake is those two messages. There is no third.

**Not in v1 of this profile.** `CLOSE` and `REKEY` are not built and are not part
of the handshake — a reimplementer must not wait for a third message. When they
land they bump `direct::VERSION`, because the record and the signalling share one
profile version. `REKEY` is also the principled answer to nonce exhaustion
(S-033), which v1 handles by refusing to send instead.

## Medium selection

The responder keeps only the candidates whose medium it is willing to use *and*
that meet the initiator's `min_bps` and `mtu_needed`, then ranks by latency hint
(lower first), breaking ties by capacity. If candidates overlapped but none were
big/fast enough it answers `throughput`; if none overlapped, `no_medium`.

A medium is a **name, not a code** — convention rather than an enum in the core.
The conventional ones:

| medium | e2e? | est. bps | mtu | notes |
|---|---|---|---|---|
| `udp` | yes | high | ≥1200 | one datagram = one record |
| `tcp` | yes | high | ≥1400 | `u32be len ‖ record` framing |
| `ble` | yes | low–med | 20–200 | may chunk below the record |
| `esp-now` | yes | ~200–500 kb | ~218 | no adapter in-tree yet; MTU is [zh_network](https://github.com/aZholtikov/zh_network)'s measured real-world ceiling for this mechanism, not a guess |

Nothing enforces that list, which is the point: SPORE Direct runs over a medium
this codebase has never heard of, and the core does not need an edit or an
allocation from anyone to allow it — the same reason [Spec](SPEC.md) keeps the
*bridge* list open while the nutrient list stays closed. Two implementations that
spell a medium differently have two mediums, so use the conventional name where
one exists and namespace anything new (`acme.lora-p2p`).

**An unrecognised medium is skipped, never fatal.** It decodes fine, and is then
simply a candidate nobody declared willingness for — so a peer offering one new
path alongside three usable ones still gets a pipe. An offer of *only* unknown
mediums answers `no_medium`, which is a reason rather than silence. The medium
name is bound into the KDF, so a record cannot be replayed onto a different one.

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
0       1     ver = direct::VERSION (3 today — the record carries the same
              profile version as the signalling, so one bump moves both)
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
- **TCP / serial / BLE:** `u32be length ‖ record` (BLE may chunk further).

## Threat model

- **Confidentiality / integrity of media** — the AEAD record. A record only opens
  under keys derived from the ephemeral DH bound to both identities, the pipe, and
  the medium; a forged or corrupted datagram fails the MAC and is dropped, not
  surfaced to the app (`Pipe::poll` skips it rather than tearing the pipe down).
- **Binding the ephemeral key to an identity** — done by the outer `send_direct`
  envelope: the OFFER/ANSWER are signed by the sender's SPORE key, so a
  man-in-the-middle cannot substitute its own `eph_pub` without also forging that
  signature.
- **Replay** — an app that cares rejects a stale or duplicate `seq` above the
  record (a sliding window), the same shape the datagram session layer uses.
  Best-effort media typically doesn't bother.
- **Nonce exhaustion (S-033)** — a separate matter from replay, and not optional.
  `seq` *is* the ChaCha20-Poly1305 nonce and `tx_key` never changes for the life
  of the pipe, so reusing a `seq` reuses a nonce: the XOR of both plaintexts
  leaks and the Poly1305 one-time key repeats, which makes forgery possible. No
  receiver-side window can undo that — it is a cryptographic failure, not an
  application preference. `u16` gives 65_536 records, about **22 minutes** of a
  50 records/s voice pipe. The pipe therefore **refuses to send** past the last
  sequence number rather than wrapping; the caller opens a fresh pipe (new
  ephemeral DH, new keys, `seq` 0). A 64-bit nonce or a mandatory REKEY would
  raise the ceiling, and both are profile changes — see "Not in v1" below.
- **Metadata** — the underlay sees two endpoints exchanging encrypted datagrams;
  it learns the medium locators (an `ip:port`), which is inherent to going direct.
  Peers that don't want to reveal a direct address simply don't offer a candidate
  and stay on the mesh.
- **Out of scope** — latency guarantees on duty-cycled radios; a
  feature-complete PBX; routing direct records through the mesh.

## API

A `DatagramPort` is whatever the chosen medium's adapter provides — `mtu`,
`send`, `try_recv`, driven by polling. The crate ships an in-memory `Loopback`
for tests (`examples/direct_loopback.rs`), two real socket adapters, and an iroh
adapter behind the `bridge-iroh` feature.

`Signalling` is the entry point. It holds no sockets and no clock: `now` is a
parameter, `willing` is the runtime declaring what it can actually open.

```rust
use spore::direct::{Signalling, Signal, Pipe, Medium};

let mut sig = Signalling::new(my_addr);

// Initiator: the SPDR bytes go out as an ordinary sealed DM.
let (offer_bytes, pipe_id) = sig.offer(peer, need, candidates, now);
node.send_direct(peer, &offer_bytes, now);

// Either side, on the plaintext of a delivered DM:
match sig.on_signal(sender, &plaintext, &[Medium::udp()]) {
    Signal::Offer { peer, accepted } => {
        // Answer FIRST, open SECOND — see "answer before you open" below.
        let (answer_bytes, answering) = accepted.answer(my_addr, local);
        node.send_direct(peer, &answer_bytes, now);
        let port = open_port_for(&chosen);                   // the runtime's job
        let pipe = answering.over(port);
    }
    Signal::Decline { peer, reply } => node.send_direct(peer, &reply, now),
    Signal::Answered { pending, answer, dial, .. } => {
        let port = open_port_for_target(&dial);
        let pipe = Pipe::finish(pending, &answer, port).unwrap();
    }
    Signal::Refused { reason, .. } => { /* tell the user, honestly */ }
    Signal::NotSignal => { /* an ordinary message — hand it to the app */ }
}

pipe.send(RecordType::Data, b"...")?;
while let Some((ty, bytes)) = pipe.poll() { /* … */ }
```

Rules this shape enforces:

- **Deciding is the core's; opening is the runtime's.** `accept` chooses without a
  port; the runtime then opens one. They are split because `UdpPort::connect` needs
  the peer's locator, which exists only after a candidate has been chosen.
- **Answer before you open.** The key schedule binds the shared secret, the pipe
  id, both addresses and the medium — never the socket — so `Accepted::answer`
  produces the ANSWER with no port at all, and `Answering::over` attaches one
  afterwards. That ordering is not a style choice: opening a punched candidate
  *blocks* for a punch window, and the initiator does not begin punching until the
  ANSWER reaches it, so a responder that opens first puts its whole window before
  the peer's and the punch can never land. `Pipe::answer_with` still fuses both and
  is correct only where opening cannot block — a loopback, a LAN socket, a link
  already up.
- **`dial` is the responder's own locator**, carried in the ANSWER. `chosen` is a
  candidate from the *initiator's* offer, so an initiator dialling `chosen` would
  dial itself.
- A runtime that can open nothing passes `&[]`. Every offer is then declined with
  a reason rather than accepted and quietly left unable to connect.
- Unanswered offers MUST be expired (`Signalling::expire`): a NAT binding dies in
  30s–5min, so a stale candidate set is better restarted than resumed.

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

| Piece | State |
|---|---|
| `SPDR` codec, key schedule, AEAD record, medium selection, `Pipe` | ✅ pure, unit-tested |
| UDP / TCP adapters | ✅ real sockets, incl. a two-process round trip |
| Mesh signalling (`Signalling`, `accept`/`answer`/`over`) | ✅ tested over the real `send_direct`/`on_rx` path |
| Daemon (`src/cli/direct.rs`) | ✅ two processes negotiate, punch, and carry a record |
| Android (JNI + Kotlin) | 🧪 compile-checked only — no device has run it |
| BLE / ESP-NOW adapters, `CLOSE`/`REKEY` | ⬜ not built |

Reachability, by candidate — see [Roadmap](ROADMAP.md)'s P-Direct-NAT track:

| Path | State |
|---|---|
| LAN | ✅ |
| Global IPv6 | ✅ no NAT in front of it |
| Declared overlay (`direct-also:`) | ✅ already routes |
| Reflexive + hole punch | 🧪 both ends punch concurrently and report `Via::Punched`, but only on loopback — where there is no NAT, so a punch that never happened looks identical to one that worked. [Hardware verification](HARDWARE.md) row 19 is the procedure that would make it ✅. A punch that does not land still falls back to a plain connect and says so |
| iroh (`direct-iroh:`) | ✅ opt-in; relay posture is never defaulted |

The daemon prints which locators it offers and how each pipe was established, so
a failure is diagnosable without a packet capture. Nothing consumes Direct
traffic in either runtime yet: inbound records are counted and dropped.
