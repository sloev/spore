# Spike 001 — WebTransport / QUIC native endpoint for browser↔native Direct

## Question

Can a native Rust endpoint accept a browser's `new WebTransport("https://host")`
connection and exchange QUIC datagrams with it, without a heavyweight
ICE/DTLS/SCTP stack — closing the browser↔native conformance gap (M2) using
the iroh/QUIC direction the roadmap already committed to?

## Why this spike

PR #125 re-scoped the M2 conformance gap from "native WebRTC ice-lite" (declined
— ICE/DTLS/SCTP is the largest dep this repo would take) to "browser↔native over
QUIC/WebTransport, reusing the iroh path." This spike validates whether that
re-scope is actually feasible, and at what dependency cost.

## Background findings (from the 0.7.0 tree)

- **iroh speaks its own QUIC dialect (`noq`), not standard HTTP/3 WebTransport.**
  `Cargo.lock` has `noq`/`noq-proto`/`noq-udp` (iroh 1.0's QUIC stack), and no
  `quinn`. A browser's `new WebTransport(url)` opens an HTTP/3 connection with
  the WebTransport-over-H3 handshake — a different ALPN and handshake than
  iroh's endpoint. So "reuse the iroh path" is **not** a free win: iroh gives
  us the `DatagramPort` *shape* and QUIC datagrams node-to-node, but the
  browser↔native path needs its own HTTP/3 + WebTransport server endpoint.
- **`rustls` and `ring` are already in the tree** (iroh pulls them), so
  WebTransport's TLS deps are not net-new.
- **`quinn` is NOT in the tree** — iroh uses `noq`. Adding `wtransport` brings
  `quinn` 0.11 + `wtransport-proto` as a second QUIC implementation. Two QUIC
  stacks in one binary is the real cost.

## Approach researched

`wtransport` 0.7.2 (the main Rust WebTransport server crate):

- Direct deps (normal): `quinn` 0.11, `rustls` 0.23, `wtransport-proto`,
  `rustls-pki-types`, `sha2`, `socket2`, `pem`, `x509-parser`, `rcgen` (optional,
  self-signed certs for dev).
- `rustls`/`ring` already present via iroh → not net-new.
- `quinn` 0.11 is net-new but is a mature, widely-deployed QUIC stack (not the
  sprawling ICE/DTLS/SCTP monster that native WebRTC was). Bounded addition.
- API: `wtransport::Server::builder().bind().await` → `accept().await` →
  `session.open_datagrams()` / `session.accept_datagrams()`. Maps cleanly onto
  SPORE's `DatagramPort` trait (`mtu`/`send`/`try_recv`), exactly as `IrohPort`
  wraps an iroh `Connection`.

## Verdict: VALIDATED (with one constraint)

### What worked (validated by research)

- **Feasible.** A native `wtransport` server endpoint can accept a browser
  WebTransport connection and exchange datagrams. The API maps cleanly onto
  `DatagramPort` — a `WebTransportPort` would be a sibling of `IrohPort`.
- **Browser side is free.** The browser already has `WebTransport` (no
  polyfill, no dep). A `web/transports/webtransport.mjs` is a thin shim, same
  shape as the existing `webrtc.mjs`/`websocket.mjs`.
- **No ICE/DTLS/SCTP.** This is the whole point — QUIC datagrams replace the
  declined WebRTC stack. The browser↔native gap closes without it.

### What didn't (the one constraint)

- **"Reuse the iroh path" is only half true.** iroh's `noq` QUIC is node-to-node
  and does not speak HTTP/3/WebTransport. The native side needs a *separate*
  `wtransport`/`quinn` listener. So the browser↔native path is a **new QUIC
  server**, not a reuse of iroh's endpoint. The reuse is the `DatagramPort`
  *abstraction* and the Direct *signalling* (OFFER/ANSWER carry the
  WebTransport locator as a candidate), not the QUIC connection itself.

### Surprises

- `rustls`/`ring` are already in the tree (via iroh) — so the TLS story is
  cheaper than expected. Only `quinn` + `wtransport-proto` are net-new.
- Two QUIC stacks (`noq` for iroh, `quinn` for WebTransport) in one binary is
  the real cost. They don't conflict (different ALPNs, different sockets) but
  it's a binary-size and compile-time hit, exactly the kind the repo's budget
  rule weighs.

### Recommendation for the real build

1. **Feature-gate it** like `bridge-iroh`: `direct-webtransport` off by default,
   off the MSRV/CI matrices, its own CI job on stable. No default-build cost.
2. **`src/direct/webtransport.rs`** — a `WebTransportPort: DatagramPort` wrapping
   a `wtransport::Server` connection, sibling to `IrohPort`. Same ALPN/attestation
   story: the WebTransport locator rides inside a sealed, signed OFFER candidate.
3. **`web/transports/webtransport.mjs`** — browser-side Direct medium: open a
   `WebTransport` to the native server, feed datagrams in/out of the hub.
4. **Medium name:** `"webtransport"` (conventional, like `udp`/`tcp`/`iroh`).
5. **The M2 conformance row stays open** until this adapter exists and a
   browser actually reaches a native node through it. The doc decision in
   PR #125 is right; this spike confirms the path is real and bounds the cost.

### Dependency budget honest check

`wtransport`+`quinn` is a meaningfully smaller commitment than the WebRTC stack
that was declined (`str0m` = ICE+DTLS+SCTP, ~the largest dep in the repo).
`quinn` is one focused QUIC implementation. But it is still a second QUIC stack
alongside iroh's `noq`, so the decision to add it is a real budget call — make it
explicitly, behind a feature gate, not by default.
