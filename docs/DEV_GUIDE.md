# Developer guide — a map of the repo

This is the doc for "I want to change something — where do I even start."
[`../MISSION.md`](../MISSION.md) covers the *why* — what SPORE is for and the
decision test a change is weighed against; `CONTRIBUTING.md` covers the
*rules* (freeze, CI, branches, releases); this one covers the *territory* —
what lives where, why it's split the way it is, and which doc is the source
of truth for which question. Read `MISSION.md` first, this once before your
first PR; you shouldn't need to read this one again.

## Start here, by goal

| I want to… | Start at |
|---|---|
| Understand the wire format | `docs/SPEC.md` (two sides of one sheet), then `docs/REBUILD.md` for worked byte examples |
| Fix a bug in the router core | `src/lib.rs` + the layer file it touches (table below) |
| Add or fix a bridge (transport) | `src/bridge/`, and `docs/BRIDGES.md` for the reference + security profile per protocol |
| Work on the Android app | `android/README.md`, then `android/app/src/main/kotlin/org/spore/node/` |
| Work on the docs site | `site/build.mjs` (the generator) + `site/home.md`; `docs/VISUALDESIGN.md` is normative for anything visual |
| Work on the browser node / wasm | `web/README.md`, `src/wasm.rs` |
| Check what's shipped vs planned | `CHANGELOG.md`'s `## Unreleased` (shipped, unreleased) and `docs/ROADMAP.md`'s PR map (planned/in-flight) — see [Status of truth](#status-of-truth) below |
| Check a security question | `docs/SECURITY_FINDINGS.md` (register of findings + fixes) and `SECURITY.md` (how to report one) |
| Verify a "🧪 needs hardware" claim | `docs/HARDWARE.md` (device evidence log) and `android/TESTING.md` |
| Add a language binding | `bindings/generate.py` + `bindings/spec.json` — bindings are generated, never hand-edited |
| Work out whether code belongs in the core or a platform | `docs/DESIGN.md`'s "The spore and the soil" — the core is the seed, every runtime hosting it is soil supplying five nutrients. Platform-specific means it belongs in the vessel, not `src/`. |

## Repo map

| Path | What it is |
|---|---|
| `src/` | The core Rust crate — the router kernel, protocol layers, and every bridge. Frozen wire format; see `CONTRIBUTING.md`. Detail below. |
| `src/main.rs` | The 12-step demo + a YAML config loader (`spore.example.yaml`) that runs a daemon's bridges on one node. |
| `bindings/` | Generated Python / Go / JS wrappers over the C ABI (`bindings/spore.h`), plus `spec.json` the generator reads. Never hand-edit the generated output — change `generate.py` or `spec.json` and regenerate. |
| `web/` | The browser stack: the wasm build of the core, JS transports (one file per medium — `web/transports/`), and `build-standalone.mjs`, which inlines all of it into the single self-contained `spore-standalone.html` node (zero network requests, verified by CI). |
| `site/` | The GitHub Pages generator (`build.mjs`) that renders `site/home.md` and most of `docs/*.md` into the public site, plus `site/seed/` (the printable paper-seed fountain-code tooling) and `site/style.css` (the design tokens — normative source is `docs/VISUALDESIGN.md`). |
| `android/` | The Android app. `android/jni/` is a small additive Rust crate (`spore-jni`) exposing an opaque-handle C ABI to Kotlin — host-checkable with plain `cargo check`, unlike the app itself which needs the Android SDK/NDK. `android/app/src/main/kotlin/org/spore/node/` is the Kotlin app. |
| `docs/` | Everything below. One doc per concern — see the table in the next section for which one answers which question. |
| `reference/` | Dependency-free Tier-0 decoders (pure Python, no crypto libs) that parse and verify a real envelope — the "trust nothing, reimplement from the spec" sanity check — plus `vectors.json`, the generated cross-language test vectors every binding and reference decoder is checked against. |
| `tests/` | `api_freeze.rs` — the frozen public-API-shape + golden-vector test. This is what makes the freeze mechanical rather than a promise; see `CONTRIBUTING.md`. |
| `examples/` | `gen_vectors.rs` (generates `reference/vectors.json` — part of the frozen contract), `worked.rs` (the worked walkthrough backing `docs/REBUILD.md`), `direct_loopback.rs`, `gen_fuzz_seeds.rs`. |
| `fuzz/` | `cargo-fuzz` targets and corpus/seed data — parsers get fuzzed, not just unit-tested. |
| `scripts/` | `check_docs_sync.py` (fails CI if `docs/REBUILD.md` drifts from the generated vectors) and `make-offline-bundle.sh` (the offline CI artifact). |
| `tools/` | Small standalone helpers that aren't part of the crate or CI — currently `reticulum_companion.py`, a companion process for the Reticulum bridge. |
| `.github/workflows/` | `ci.yml` (the real test/build/lint gate) and `pr-guard.yml` (refuses PRs that touch frozen files without the `allow-frozen-change` label). Both are themselves frozen. |

### Inside `src/` — one file per layer

The crate is organized so each concern has exactly one file (or, for the
larger ones, one directory) — if you're looking for where something happens,
it's almost always named for what it is:

| Path | Layer |
|---|---|
| `lib.rs` | The router kernel: envelope structure, fountain fragmentation/reassembly, path/ID derivation, `Node`, sealing. Also re-exports the public API surface, part of the freeze. |
| `node/` | `Node`'s own behavior split out by concern: `identity.rs` (keys, address), `send.rs`, `ingest.rs` (inbound processing), `sync.rs` (INV/WANT), `datagram.rs`, `files.rs`. |
| `envelope.rs`, `armor.rs`, `kiss.rs` | Wire-level: the envelope's own (de)serialization, the printable-armor encoding, KISS framing for byte-stream media. |
| `seal.rs`, `ratchet.rs`, `session.rs` | Crypto: one-shot prekey sealing, the §7 Double Ratchet, and the session bootstrap/state that picks between them. |
| `topic.rs` | Encrypted topics — KEYROT membership/rotation for a shared-key channel. |
| `mix.rs` | Onion wrap/peel, size-class padding, batching — the anonymity-harder async path. Opt-in, not Tor; see `docs/SPEC.md` and the audit-tour honesty notes in `docs/ROADMAP.md`. |
| `file.rs`, `fountain.rs`, `bundle.rs` | Content-addressed files: fountain-coded chunks, manifests, and the tree-of-manifests shape a big file becomes. |
| `rpc.rs`, `feed.rs` | Request/response over ordinary signed envelopes (the profile-pull feature rides this with zero wire changes), and the topic-scoped microblog feed. |
| `congestion.rs` | Trickle/CSMA-style flood damping so a burst doesn't starve the mesh. |
| `store.rs` | The spillable envelope store — in-memory up to a budget, then disk, with verification on what it spilled. |
| `invite.rs` | The armor-encoded invite blob (address + name hint + shareable bridge specs) QR codes and links carry. |
| `direct.rs`, `direct/` | SPORE Direct — the negotiated, non-routed low-latency E2E pipe. See `docs/DIRECT.md`. |
| `bridge/` | Every transport, one file each (`udp.rs`, `tcp.rs`, `audio.rs`, `meshtastic.rs`, `reticulum.rs`, `tor.rs`, `i2p.rs`, `ssb.rs`, `store.rs`/`foldersync.rs`, `bag.rs`, `icmp.rs`, `iroh.rs`, `ax25.rs`, `copyparty.rs`, …), plus the shared machinery: `hub.rs` (fans one `Node` out to every bridge), `driver.rs` (the generic datagram-transport run loop every UDP-shaped medium reuses), `stream_link.rs` (the generic KISS-over-byte-stream loop every TCP-shaped medium reuses), `neighbors.rs` (ARP-style snoop/resolve table), `csma.rs`. See `docs/BRIDGES.md` for the reference + per-bridge privacy profile. |
| `cli/` | The binary crate's own code (not part of the frozen library): `config.rs` (YAML bridge specs), `run.rs` (stands up every configured bridge on a thread), `sim.rs`. |
| `ffi.rs`, `wasm.rs` | The two non-Rust-native ABIs: the C ABI bindings are generated from (`bindings/spec.json` → `bindings/spore.h` + wrappers), and the browser `wasm32` export surface `web/` builds against. Neither is `android/jni` — that's its own crate, additive over the frozen core, not part of it. |
| `robustness.rs` | Property/fuzz-style tests that throw arbitrary and corrupted-real bytes at every parser and assert "doesn't panic," not "parses correctly." |

## Status of truth

This project deliberately has exactly two places that say what state something
is in, and they answer different questions:

- **`CHANGELOG.md`'s `## Unreleased`** — what has *shipped* (merged to `master`)
  since the last release, in plain prose a user reads to know what changed.
- **`docs/ROADMAP.md`'s PR map + each PR's own Status line** — what's *planned,
  in review, or carried forward* — the engineering plan, including deliberately
  deferred scope and why.

If you're checking whether something is done, check both — a PR can be merged
(CHANGELOG) while a piece of what it originally scoped is still open (ROADMAP's
carried-forward notes). Don't add a third status table, a TODO comment that
isn't linked from either, or a doc claiming something is done that neither of
these agrees with — `docs/SECURITY_FINDINGS.md`'s own history is full of
findings that were exactly this shape ("claims with no implementation behind
them"), and this project has a standing habit of treating that as a bug class,
not a documentation nitpick.

Two more docs carry their own narrow slice of "current state," worth knowing
about rather than duplicating:

- **`docs/SECURITY_FINDINGS.md`** — the findings register: what was found, the
  root cause, the fix, the regression test. `SECURITY.md` is how to report a
  new one.
- **`docs/HARDWARE.md`** and **`android/TESTING.md`** — real device evidence.
  Anything marked 🧪 in the roadmap or changelog means "verified in code, not
  yet on hardware" — don't upgrade that marker without an entry here.

## Building and testing each part

`CONTRIBUTING.md`'s "Running everything locally" section is the exact command
list CI runs for the core crate; this is the map of *which* commands apply to
*which* part of the repo, since most of it doesn't need the whole list:

| Area | Commands |
|---|---|
| Core crate (`src/`) | `cargo test --all-targets`, `cargo clippy --all-targets`, `cargo fmt --all --check` |
| Wasm / browser | `cargo build --release --lib --target wasm32-unknown-unknown && node web/build-standalone.mjs && node web/test.mjs` |
| `android/jni` | `cd android/jni && cargo check` (or `clippy`/`fmt --check`) — pure Rust, builds on any host with no Android toolchain. The actual `.apk` needs the SDK/NDK; CI's `android` workflow (`gradle :app:assembleDebug`) is the real gate if you don't have those installed locally. |
| Android app UI | Needs Android Studio or the SDK/NDK + `gradle`; see `android/README.md`. |
| Docs site | `cd site && npm install && node build.mjs` — also fails on a broken internal link or anchor, so it's a real check, not just a preview. `node seed/fountain.test.mjs && node seed/seedsheet.test.mjs` covers the paper-seed tooling. |
| C ABI / bindings | Regenerate with `bindings/generate.py` after changing `bindings/spec.json`; never hand-edit `bindings/python|go|node/`. |
| Fuzz targets | `cargo fuzz run <target>` from `fuzz/` (needs nightly + `cargo-fuzz`); seeds live in `fuzz/seeds`. |
| Vectors / reference decoders | `cargo run --example gen_vectors > reference/vectors.json`, then `python3 reference/test_t0.py` and `python3 scripts/check_docs_sync.py`. |

## Conventions worth knowing before your first PR

- **The wire format, the C ABI, and the golden test vectors are frozen.**
  `CONTRIBUTING.md` has the full list and the escape hatch (`allow-frozen-change`
  label) for a deliberate `2.0`. If you're not sure whether something you're
  touching is in that set, `pr-guard.yml`'s file list is the ground truth.
- **No fake UI, ever.** A control that doesn't do what it visually claims —
  a toggle with no backend, a status that can't actually be false, a "🧪 works
  on hardware" claim with no device behind it — is a bug class this project
  actively hunts for (see `docs/SECURITY_FINDINGS.md` and `docs/ROADMAP.md`'s
  audit notes), not a shortcut. If a control can't be made real yet, it either
  doesn't ship or ships visibly disabled with why.
- **`docs/VISUALDESIGN.md` is normative** for anything a person looks at —
  tokens, contrast, motion, the forbidden pink-on-olive pairing. It names which
  surfaces (site, standalone web node, Android) actually consume it; keep that
  table honest if you touch styling anywhere.
- **Docs can't drift from code.** Concrete documented byte values live once, in
  `reference/vectors.json`, generated — not typed by hand into a doc. CI
  enforces this; see `CONTRIBUTING.md`.
- **Branch model:** `master` is protected; `develop` is where work happens. See
  `CONTRIBUTING.md` for the exact PR/branch rules and how a release is cut.

## Where to go deeper

| Doc | For |
|---|---|
| `MISSION.md` | What SPORE is for and the decision test a change is weighed against — read before proposing a feature. |
| `docs/SPEC.md` | The wire format itself — the two-page normative spec. |
| `docs/REBUILD.md` | Reimplementing SPORE in another language, with worked byte examples. |
| `docs/DESIGN.md` | The application layers above transport: files, sessions, RPC, feeds, ratchet, mix — plus "The spore and the soil," the core-vs-runtime model every platform follows. |
| `docs/BRIDGES.md` | Every bridge: wire format, SPORE mapping, security profile, spec links. |
| `docs/DIRECT.md` | SPORE Direct — the negotiated non-routed E2E pipe. |
| `docs/CONTINUITY.md` | SPORE as a seed: single-file node, cold-start playbooks, offline trust. |
| `docs/RESILIENCE.md` | How Continuity, Rebuild, the reference decoders, and release artifacts fit together. |
| `docs/APPS.md` | What to install, with download links — the user-facing app index. |
| `docs/ROADMAP.md` | The engineering plan: PR map, carried-forward items, status. |
| `docs/SECURITY_FINDINGS.md` / `SECURITY.md` | The findings register / how to report a new one. |
| `docs/HARDWARE.md` / `android/TESTING.md` | Real device verification evidence. |
| `docs/VISUALDESIGN.md` | The design language — tokens, components, motion, checklist. |
| `CONTRIBUTING.md` | Freeze rules, CI gates, branch model, cutting a release. |
