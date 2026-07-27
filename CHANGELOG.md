# Changelog

The 1.x wire format is frozen, so this records what changed *around* it: security
fixes, bridges, tooling, and the handful of local-policy changes that alter how a
node behaves without altering what goes on the wire.

Two conventions specific to this project:

- **Wire status is stated on every entry.** "Wire unchanged" means
  `cargo run --example gen_vectors` still produces `reference/vectors.json`
  byte-for-byte, so every reference decoder and every 1.x peer is unaffected.
- **Security fixes reference their finding.** `S-0nn` links to
  [`docs/SECURITY_FINDINGS.md`](docs/SECURITY_FINDINGS.md), which carries the
  reproduction, root cause and regression test for each one.

## Unreleased

Wire unchanged throughout. One frozen *Rust* signature changed (`Node::send`);
`bindings/spore.h` and the vectors are untouched.

### Security

The remote-denial-of-service class is closed: nothing known remains that lets a
peer on the medium crash a node, exhaust its memory, or use it as an amplifier.

- **S-001** A `FRAGMENT` with `count == 0` reached `idx % count` and panicked. One
  public packet, no key, repeatable — any peer could stop any node in earshot.
  Found by the new robustness harness on its first run.
- **S-002 / S-004** Neighbour learning, path learning and quota attribution all
  bound identity from the `SIGNED` **flag** rather than a verified signature. A
  copied public key and 64 zero bytes redirected a victim's directed mail or drained
  their byte budget. Relays now verify before binding trust state — and still do not
  verify merely to forward.
- **S-003** A stamp of class 1 — about two hashes' work, and half of all envelopes
  by luck — bypassed both per-source quotas and backpressure, so §10 bounded
  nothing. `STAMP_QUOTA_BYPASS_BITS = 16` now gates the exemption.
- **S-005** Fifteen `extern "C"` functions could unwind a panic across the C ABI,
  which is undefined behaviour; a wrapper passing a null key triggered it. Guarded,
  and the pointer helpers no longer panic.
- **S-006 / S-013 / S-016 / S-017** Eleven tables a peer could grow without bound —
  neighbours, dedup, incomplete fountain sets, peer records, manifests, receipts,
  inboxes, imported filenames, and the ratchet's skipped-key cache. All capped and
  expired. Every eviction degrades a capability rather than breaking one.
- **S-007 / S-015** Six unbounded filesystem reads on directories other programs
  write to by design, two of them with a `metadata`-then-read gap. One shared
  `store::read_capped` now bounds the read itself.
- **S-008** The Reticulum UDP bridge fed its KISS framer from any source, so a
  stranger's bytes corrupted a frame the companion was midway through.
- **S-012** `WANT` answered every id it was handed with a whole stored envelope:
  32x amplification measured, replayable forever because INV/WANT bypass dedup and
  the quota. Now bounded per packet and per link.
- **S-018** A panic under the hub mutex poisoned it, and every later
  `lock().unwrap()` panicked too — one fault killed every bridge thread
  permanently. Poisoning is now recovered from.
- **S-019** Meshtastic length varints overflowed the parse offset in both of
  `decode`'s protobuf loops — a remote panic on any build with overflow checks on,
  which is every `cargo build` without `--release`. Found by the new `radio_codecs`
  fuzz target within 90 seconds. The sibling parser already did this correctly.
- **S-020** The encrypted-topic ratchet had forward secrecy but no
  post-compromise security: `rotate` is a hash chain, so anyone who obtained one
  group key derived every later one and the group stayed compromised until a human
  noticed. `topic::contribute`/`absorb` now fold sealed fresh entropy into the key,
  so a group heals through ordinary use against an attacker holding the chain.
  Additive — every existing function is byte-for-byte unchanged.
- **S-021** The Android release advertised a dead "stable release" link, showed a
  three-day-old publication date while building on every merge, accumulated one
  APK per day with no indication which was current, and named itself from its own
  moving tag (`SPORE rolling rolling+2026.07.27`).

### Changed — local policy, not wire

Both alter how a node behaves on a network without changing what it emits, so a
node running this interoperates with a 1.0 peer that is simply more permissive.

- Mail must be stamped to **16 leading zero bits** (was: any non-zero stamp) to skip
  the per-source quota or a busy peer's backpressure.
- Relays **verify a signature before writing identity into local state** (neighbour,
  path and quota tables). Forwarding still does no crypto. Rationale and the cost on
  constrained hardware: [`docs/DESIGN.md`](docs/DESIGN.md).

### Changed — API

- `Node::send` returns `Result<Vec<Forward>, TooLarge>` instead of panicking when an
  object needs more than 255 fountain chunks (**S-011**). `Hub::send` forwards the
  error. This changed a frozen signature and landed under `allow-frozen-change`; the
  wire format did not move.

### Added — bridges

**NFC (Web NFC)** — tap-to-transfer as an `application/x-spore` NDEF record. The
NDEF codec is pure and tested; the tap needs a phone, so the bridge ships 🧪. It
lives in the browser rather than the daemon because a Rust NFC bridge needs
`libnfc` or PC/SC — a C library, which is the dependency rule that also kept TLS
out.

AX.25/KISS, Tor (SOCKS5), I2P (SAM v3, dial and accept), copyparty/WebDAV, UDP
multicast on any address family (which unblocked Yggdrasil, cjdns, BATMAN and
Thread), Reticulum over TCP/UDP, ICMP echo, and NNCP/UUCP via a new spool bridge.
Every native stream bridge now shares `bridge::stream_link` with automatic
reconnection and exponential backoff.

### Added — tooling

- `src/robustness.rs` — always-on malformed-input harness over every parser a
  stranger can reach. It found S-001.
- `fuzz/` — six libFuzzer targets in their own workspace, plus a scheduled
  workflow, so nightly never becomes a build requirement. The `radio_codecs`
  target found **S-019** within 90 seconds of existing.
- `.github/workflows/msrv.yml` — installs the toolchain named by `rust-version` and
  runs the suite. **S-014** was that claim being false in two ways at once, so it is
  now enforced by something that executes.
- `.github/workflows/supply-chain.yml` — `cargo-audit`, `cargo-deny`, a
  bindings-in-sync check, and an offline-bundle job that vendors and then builds
  with an empty `CARGO_HOME`.
- `scripts/make-offline-bundle.sh` — **S-010**: `CONTINUITY.md` promised an offline
  rebuild the repo could not perform. The claim is now true and CI proves it.
- Dependabot, and SHA-256 checksums on release assets.

### Documentation

`docs/SECURITY_FINDINGS.md` (the findings register, also published to the site),
`SECURITY.md`, this changelog, per-bridge privacy profiles in `BRIDGES.md`, the
stamp deployer note, and the DESIGN section explaining what "relays never verify"
does and does not mean.

### Fixed

- ICMP assumed a 20-byte IP header, silently dropping every packet carrying options
  — which is what a "diagnostics only" network adds, and that is the network the
  bridge exists for (**S-009**).
- The offline bundle was 290 MB of build output; excluding artifacts properly brings
  it to 6.6 MB, verified to still build cold from a clean extraction.
- The Android APK now has a permanent download URL —
  `releases/download/rolling/spore-android.apk` — instead of a versioned filename
  nothing could link to, and `docs/APPS.md` leads with it (**S-021**).
- Every in-page anchor on the docs site was dead: 116 of them, including the whole
  bridge index. `marked` emits no heading ids, and the docs are written for GitHub,
  which adds them silently. Headings now carry GitHub's own slugs, and the site
  build fails on any internal link or anchor that does not resolve — which is now a
  CI check, not just a deploy-time one.
- Rust 1.75 compatibility restored in earnest: `zeroize_derive` pinned, lockfile
  regenerated at version 3, and four uses of post-1.75 APIs replaced.

## 1.0.0

The frozen contract: envelope format, the five medium shapes, `bindings/spore.h`,
`reference/vectors.json`, and the reference decoders. Everything since has been
built without moving any of it.
