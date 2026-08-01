# Developer guide — a map of the repo

Where things live. [`MISSION.md`](../MISSION.md) is the *why* and the decision
test; [`CONTRIBUTING.md`](../CONTRIBUTING.md) is the *rules* (freeze, CI,
branches, releases). Read those once; this one is a lookup table.

## Start here, by goal

| I want to… | Start at |
|---|---|
| Understand the wire format | `docs/SPEC.md`, then `docs/REBUILD.md` for worked bytes |
| Fix a bug in the router core | `src/lib.rs` + the layer file below |
| Add or fix a bridge | `src/bridge/`, and `docs/BRIDGES.md` for the per-protocol reference |
| Work on the Android app | `android/README.md`, then `android/app/src/main/kotlin/org/spore/node/` |
| Work on the docs site | `site/build.mjs` + `site/home.md` |
| Work on the browser node / wasm | `web/README.md`, `src/wasm.rs` |
| Check what's shipped vs planned | `CHANGELOG.md` `## Unreleased` and `docs/ROADMAP.md` — see [Status](#status) |
| Check a security question | `docs/SECURITY_FINDINGS.md`; `SECURITY.md` to report one |
| Verify a 🧪 claim | `docs/HARDWARE.md`, `android/TESTING.md` |
| Add a language binding | `bindings/spec.json` → `bindings/generate.py`; never hand-edit output |
| Change a colour | `design/tokens.json` → `python3 design/generate.py` |
| Decide core vs runtime | `docs/DESIGN.md` § "The spore and the soil". Platform-specific means runtime, not `src/` |

## Repo map

| Path | What it is |
|---|---|
| `src/` | The core crate: router kernel, protocol layers, every bridge. Frozen wire format. Detail below. |
| `src/main.rs` | Demo + YAML config loader (`spore.example.yaml`) running a daemon's bridges on one node. |
| `bindings/` | Generated Python / Go / JS wrappers over the C ABI, plus the `spec.json` they generate from. |
| `design/` | `tokens.json` (every colour, once) → `generate.py`. Emits the palette into its three surfaces — `site/style.css`, `web/build-standalone.mjs`, `android/…/Chrome.kt` — plus the token table in `docs/VISUALDESIGN.md`, and computes WCAG ratios rather than trusting typed ones. |
| `web/` | The browser stack: wasm core, one JS transport per medium (`web/transports/`), and `build-standalone.mjs`, which inlines everything into one self-contained node. Zero network requests, verified by CI. |
| `site/` | The Pages generator (`build.mjs`), `site/seed/` (printable paper-seed tooling), and `site/style.css`, whose token block is generated. |
| `android/` | `android/jni/` is an additive Rust crate exposing an opaque-handle C ABI to Kotlin — checkable with plain `cargo check`. `android/app/…/node/` is the Kotlin app, which needs the SDK/NDK. |
| `reference/` | Dependency-free Tier-0 decoders (pure Python, no crypto libs) plus `vectors.json`, the generated cross-language vectors everything is checked against. |
| `tests/` | `api_freeze.rs` — what makes the freeze mechanical rather than a promise. |
| `examples/` | `gen_vectors.rs` (generates `reference/vectors.json`), `worked.rs` (backs `REBUILD.md`), `direct_loopback.rs`, `gen_fuzz_seeds.rs`. |
| `fuzz/` | `cargo-fuzz` targets, corpus and seeds. Parsers are fuzzed, not only unit-tested. |
| `scripts/` | `check_docs_sync.py` (fails CI if `REBUILD.md` drifts from the vectors), `make-offline-bundle.sh`. |
| `tools/` | Helpers outside the crate and CI — currently `reticulum_companion.py`. |
| `.github/workflows/` | `ci.yml` (the gate) and `pr-guard.yml` (refuses PRs touching frozen files without `allow-frozen-change`). Both are themselves frozen. |

### Inside `src/` — one file per layer

| Path | Layer |
|---|---|
| `lib.rs` | Router kernel: envelope, fountain fragmentation/reassembly, path/ID derivation, `Node`, sealing. Re-exports the frozen public API. |
| `node/` | `Node` split by concern: `identity.rs`, `send.rs`, `ingest.rs`, `sync.rs` (INV/WANT), `datagram.rs`, `files.rs`. |
| `envelope.rs`, `armor.rs`, `kiss.rs` | Wire level: (de)serialization, printable armor, KISS framing. |
| `seal.rs`, `ratchet.rs`, `session.rs` | Crypto: prekey sealing, §7 Double Ratchet, and the bootstrap that picks between them. |
| `topic.rs` | Encrypted topics — KEYROT membership and rotation. |
| `mix.rs` | Onion wrap/peel, size-class padding, batching. Opt-in; not Tor. |
| `file.rs`, `fountain.rs`, `bundle.rs` | Content-addressed files: fountain chunks, manifests, tree-of-manifests. |
| `rpc.rs`, `feed.rs` | Request/response over ordinary signed envelopes; topic-scoped feed. |
| `congestion.rs` | Trickle/CSMA flood damping. |
| `store.rs` | Spillable envelope store — memory to a budget, then a `SpillBackend`. |
| `invite.rs` | The armor-encoded invite blob QR codes and links carry. |
| `direct.rs`, `direct/` | SPORE Direct. See `docs/DIRECT.md`. |
| `bridge/` | One file per transport, plus shared machinery: `hub.rs` (fans one `Node` to every bridge), `driver.rs` (datagram run loop), `stream_link.rs` (KISS-over-stream loop), `neighbors.rs` (ARP-style resolver), `csma.rs`. |
| `cli/` | The binary crate: `config.rs`, `run.rs`, `direct.rs`, `sim.rs`. Not part of the frozen library. |
| `ffi.rs`, `wasm.rs` | The two non-Rust ABIs. Neither is `android/jni`, which is its own crate. |
| `robustness.rs` | Property/fuzz tests asserting "doesn't panic" on arbitrary and corrupted input. |

## Status

Exactly two places record state, and they answer different questions:

| Source | Says |
|---|---|
| `CHANGELOG.md` `## Unreleased` | What has **shipped** since the last release |
| `docs/ROADMAP.md` | What is **planned, in review, or carried forward** |

Check both: a PR can be merged while part of its original scope stays open.

Do **not** add a third status table, an unlinked TODO, or a doc claiming
something neither agrees with. `docs/SECURITY_FINDINGS.md` is full of findings
of exactly that shape — claims with no implementation behind them — and this
project treats that as a bug class.

Two docs carry a narrow slice of state and MUST NOT be duplicated elsewhere:
`docs/SECURITY_FINDINGS.md` (findings register) and `docs/HARDWARE.md` +
`android/TESTING.md` (device evidence — 🧪 means verified in code, not on
hardware).

## Building and testing

`CONTRIBUTING.md` has the full CI command list. This is which commands apply to
which part:

| Area | Commands |
|---|---|
| Core crate | `cargo test --all-targets`, `cargo clippy --all-targets`, `cargo fmt --all --check` |
| Wasm / browser | `cargo build --release --lib --target wasm32-unknown-unknown && node web/build-standalone.mjs && node web/test.mjs` |
| `android/jni` | `cd android/jni && cargo check` — pure Rust, no Android toolchain needed. The `.apk` needs the SDK/NDK; CI's `apk` job is the real gate. |
| Android app UI | Android Studio or SDK/NDK + `gradle`. See `android/README.md`. |
| Docs site | `cd site && npm install && node build.mjs` — fails on a broken internal link, so it is a real check. `node seed/*.test.mjs` covers the paper-seed tooling. |
| C ABI / bindings | `python3 bindings/generate.py` after changing `spec.json`. Never hand-edit `bindings/{python,go,node}/`. |
| Design tokens | `python3 design/generate.py` after changing `tokens.json`. CI fails on drift. |
| Fuzz | `cargo fuzz run <target>` from `fuzz/` (nightly + `cargo-fuzz`). |
| Vectors | `cargo run --example gen_vectors > reference/vectors.json`, then `python3 reference/test_t0.py` and `python3 scripts/check_docs_sync.py`. |

## Conventions

- **The wire format, the C ABI and the golden vectors are frozen.**
  `pr-guard.yml`'s file list is the ground truth; `allow-frozen-change` is the
  escape hatch for a deliberate 2.0.
- **No fake UI.** A control that does not do what it visually claims — a toggle
  with no backend, a status that cannot be false, a 🧪 claim with no device
  behind it — either does not ship, or ships visibly disabled with a reason.
- **`docs/VISUALDESIGN.md` is normative** for anything a person looks at, and
  names which surfaces consume it. Keep that table honest.
- **Docs MUST NOT drift from code.** Documented byte values live once, generated,
  in `reference/vectors.json`. CI enforces it.
- **Branch model:** `master` is protected; work happens on `develop`.

## Where to go deeper

| Doc | For |
|---|---|
| `MISSION.md` | What SPORE is for, and the decision test |
| `docs/SPEC.md` | The wire format — normative |
| `docs/REBUILD.md` | Reimplementing in another language, with worked bytes |
| `docs/DESIGN.md` | Application layers, and the core-vs-runtime model |
| `docs/BRIDGES.md` | Every bridge: wire format, mapping, security profile |
| `docs/DIRECT.md` | SPORE Direct |
| `docs/CONTINUITY.md` | SPORE as a seed; what survives, and what guarantees it |
| `docs/APPS.md` | What to install |
| `docs/ROADMAP.md` | The engineering plan |
| `docs/SECURITY_FINDINGS.md` / `SECURITY.md` | Findings register / how to report |
| `docs/HARDWARE.md` / `android/TESTING.md` | Device evidence |
| `docs/VISUALDESIGN.md` | The design language |
| `CONTRIBUTING.md` | Freeze rules, CI, branches, releases |
