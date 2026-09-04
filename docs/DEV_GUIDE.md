# Developer guide — a map of the repo

Where things live. [Mission](MISSION.md) is the *why* and the decision
test; [Contributing](CONTRIBUTING.md) is the *rules* (freeze, CI,
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
| Check what's shipped vs planned | `git log`, the [releases](https://github.com/sloev/spore/releases), and `docs/ROADMAP.md` — see [Status](#status) |
| Check a security question | `docs/SECURITY_FINDINGS.md`; `docs/SECURITY.md` to report one |
| Verify a 🧪 claim | `docs/HARDWARE.md`, `android/TESTING.md` |
| Add a language binding | `bindings/spec.json` → `bindings/generate.py`; never hand-edit output |
| Change a colour | Edit upstream `supernihil/hardbrut`, then `node web/hardbrut-sync.mjs && python3 android/hardbrut-sync.py` → `python3 design/generate.py` (Android) |
| Change an Android-only size (control/chip/row/touch floor) | `design/tokens.json` → `python3 design/generate.py` |
| Decide core vs runtime | `docs/SPEC.md` Part 4, "Where the core runs". Platform-specific means runtime, not `src/` |

## Repo map

| Path | What it is |
|---|---|
| `src/` | The core crate: router kernel, protocol layers, every bridge. Frozen wire format. Detail below. |
| `src/main.rs` | Demo + YAML config loader (`spore.example.yaml`) running a daemon's bridges on one node. |
| `bindings/` | Generated Python / Go / JS wrappers over the C ABI, plus the `spec.json` they generate from. |
| `design/` | `generate.py` aliases Android's `Chrome.kt` `Palette`/`Metrics` onto `android/app/src/main/kotlin/org/spore/node/vendor/Hardbrut.kt`'s `HardbrutTokens` (light palette, border, shadow, spacing) and parses the four dark-mode hexes from `web/vendor/hardbrut/hardbrut.css` — the one thing the Compose port doesn't define. No SPORE-authored copy of a colour anywhere. `tokens.json` keeps only the Android-only control-size table (control/chip/row, touch floor), which HARDBRUT has no equivalent of. The docs site and the standalone node need no generated block at all — both import the vendored CSS directly (`web/hardbrut-import.mjs`). |
| `web/` | The browser stack: wasm core, one JS transport per medium (`web/transports/`), `hardbrut-import.mjs`/`hardbrut-sync.mjs` (vendors HARDBRUT's real CSS at build time, used by the Pages site and Android), `vendor/hardbrut3/` (the standalone's own HARDBRUT/3 vendor, M10-D), and `build-standalone.mjs`, which inlines everything into one self-contained node. Zero network requests, verified by CI. |
| `site/` | The Pages generator (`build.mjs`, HARDBRUT classes only, no hand-authored CSS) and `site/seed/` (printable paper-seed tooling). |
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
| `git log` / [releases](https://github.com/sloev/spore/releases) | What has **shipped** |
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

## Install & verify a release

The four surfaces in `docs/APPS.md`, with the commands that don't fit on a
picker page.

**Android APK.** Permanent rolling link, rebuilt on every merge:
`<major>.<minor>.<stamp>+<sha>`.

```sh
curl -LO https://github.com/sloev/spore/releases/download/rolling/spore-android.apk
curl -LO https://github.com/sloev/spore/releases/download/rolling/spore-android.apk.sha256
sha256sum -c spore-android.apk.sha256
```

Allow installs from the browser/files app — builds are debug-signed until a
release keystore exists, so Android will warn about an unknown developer.
`nightly-YYYY.MM.DD` keeps the last five dated builds for rollback; `/releases/
latest/download/spore-android.apk` is the last tagged build with assets.

**Single-file web node.** Save it, mail it, put it on a stick — CI asserts
**zero** external requests, so it opens over `file://` with no internet. Its
own "Download a copy" button re-serializes the page so one seed makes the
next; identity and bridges live in `localStorage`. Every
[release](https://github.com/sloev/spore/releases/tag/rolling) carries the
same file as a permanent asset, so a copy doesn't depend on this site staying
up.

```sh
cargo build --release --lib --target wasm32-unknown-unknown
node web/build-standalone.mjs                            # -> web/spore-standalone.html
```

**Desktop daemon.**

```sh
cargo build --release        # -> target/release/spore
cargo run                    # in-memory mesh demo
cargo run -- node.yaml       # bridges from a config file
```

No network to reach crates.io? Every
[release](https://github.com/sloev/spore/releases/latest/download/spore-offline-bundle.tar.gz)
also carries this source tree with every dependency vendored in — it unpacks
flat, so give it a folder:

```sh
mkdir spore-offline && cd spore-offline
curl -LO https://github.com/sloev/spore/releases/latest/download/spore-offline-bundle.tar.gz
tar xzf spore-offline-bundle.tar.gz
cargo build --release --offline
```

**Seed Sheet.** Printable A4, fountain-coded QR on one side, wire format by
hand on the other — a stained or partial print can still recover the payload
(SPORE's own erasure coding, turned on itself). See
[Continuity](continuity.html) for why this exists.

```sh
cd site && npm install && node seed/build-seedsheet.mjs   # -> web/spore-seedsheet.html
```

## Building and testing

`docs/CONTRIBUTING.md` has the full CI command list. This is which commands apply to
which part:

| Area | Commands |
|---|---|
| Core crate | `cargo test --all-targets`, `cargo clippy --all-targets`, `cargo fmt --all --check` |
| Wasm / browser | `cargo build --release --lib --target wasm32-unknown-unknown && node web/build-standalone.mjs && node web/test.mjs` |
| `android/jni` | `cd android/jni && cargo check` — pure Rust, no Android toolchain needed. The `.apk` needs the SDK/NDK; CI's `apk` job is the real gate. |
| Android app UI | Android Studio or SDK/NDK + `gradle`. See `android/README.md`. |
| Docs site | `cd site && npm install && node build.mjs` — fails on a broken internal link, so it is a real check. `node seed/*.test.mjs` covers the paper-seed tooling. |
| C ABI / bindings | `python3 bindings/generate.py` after changing `spec.json`. Never hand-edit `bindings/{python,go,node}/`. |
| Design tokens | `node web/hardbrut-sync.mjs && python3 android/hardbrut-sync.py` after HARDBRUT upstream moves; `python3 design/generate.py` after that or after changing `tokens.json`'s Android sizing table. CI fails on drift in any of these. |
| Fuzz | `cargo fuzz run <target>` from `fuzz/` (nightly + `cargo-fuzz`). |
| Vectors | `cargo run --example gen_vectors > reference/vectors.json`, then `python3 reference/test_t0.py` and `python3 scripts/check_docs_sync.py`. |

## Conventions

- **The wire format, the C ABI and the golden vectors are frozen.**
  `pr-guard.yml`'s file list is the ground truth; `allow-frozen-change` is the
  escape hatch for a deliberate 2.0.
- **No fake UI.** A control that does not do what it visually claims — a toggle
  with no backend, a status that cannot be false, a 🧪 claim with no device
  behind it — either does not ship, or ships visibly disabled with a reason.
- **HARDBRUT upstream (`supernihil/hardbrut`) is normative** for anything a person
  looks at. SPORE keeps no design-language document of its own. The flat
  `web/vendor/hardbrut/` is vendored at build time and trusted as-is for the
  Pages site and Android; the web app (the standalone) instead consumes
  **HARDBRUT/3** at `web/vendor/hardbrut3/` (M10-D).
- **Docs MUST NOT drift from code.** Documented byte values live once, generated,
  in `reference/vectors.json`. CI enforces it.
- **Branch model:** `master` is protected; work happens on a topic branch off it
  (`feat/…`, `fix/…`, `docs/…`) and squash-merges back. No `develop`.

## Android → HARDBRUT token mapping

The Android app consumes HARDBRUT (`supernihil/hardbrut` v0.6) through
`Chrome.kt`. The token source of truth is `design/tokens.json`, regenerated by
`design/generate.py` into that file between the `// >>> design tokens` markers.

| CSS token      | CSS value      | Android `Palette` member (light / dark) |
|----------------|----------------|------------------------------------------|
| `--ink`        | `#000`         | `Ink` / `InkDark`                        |
| `--paper`      | `#fff`         | `Paper` / `PaperDark`                    |
| `--yellow`     | `#ffd23f`      | `Yellow` / `YellowDark`                  |
| `--muted`      | `#666`         | `Muted` / `MutedDark`                    |
| `--bg`         | `#fdfaf2`      | `Bg` / `BgDark`                          |
| `--onyellow`   | `#121210`      | `OnYellow` (dark only)                   |
| `--border`     | `3px solid`    | `Metrics.Border` (3.dp) + `Palette.Ink`  |
| `--shadow`     | `5px 5px 0`    | hand-drawn hard offset rect (no blur)    |
| `--shadow-sm`  | `3px 3px 0`    | hand-drawn hard offset rect (no blur)    |

**CRITICAL**: Compose's `Modifier.shadow()` does **not** do zero blur — it is the
blurred Material elevation shadow this language exists to replace. The hard
shadow is drawn by hand in `crate` / `CrateButton` with a `drawBehind` offset
rect (see `Modifier.crate` in `Chrome.kt`). Reach for `Modifier.shadow()` and a
crate stops being a crate.

**Typography.** Display (headings, labels): system sans at `FontWeight.Black`,
uppercase — HARDBRUT's Impact has no Android equivalent and constraint 1
forbids bundling a webfont, so this is the honest stand-in (`DisplayHeading`).
Body / mono: `FontFamily.Monospace` (badges, captions, status, addresses).

**Buttons (`CrateButton`).** Two kinds, no more: Default (`Palette.Yellow` face)
and Cancel (`Palette.Paper` face), both `Palette.Ink` ink. Same size, same
`Metrics.Border` ink border, same hard shadow. Press translates the face 3dp
down/right and drops the shadow to zero (`Metrics.Throw`). Disabled is frozen
(no recolour).

**Cards (`Crate`).** `Modifier.crate()`: paper fill, 3dp ink border, hard
no-blur offset shadow into reserved padding, zero radius (`CrateShape =
RoundedCornerShape(Metrics.Radius)` where `Radius = 0`).

**Inputs (`ToughbookField`).** Paper field, 3dp ink border, hard shadow; focus
*thickens* the border rather than removing it.

**What was dropped with the old language.** Scanlines/vignette/CRT bloom —
HARDBRUT has no ambient VFX, removed entirely (`scanlines()` is gone). The
pastel pink chibi Baud — restyled flat ink + yellow at HARDBRUT migration, then
removed entirely: SPORE's brand is the wordmark only, no mascot, no icon. The
old `Void/Asphalt/Amber/Pink/Cyan/Phosphor/Kevlar/Dim` palette members.

## Where to go deeper

| Doc | For |
|---|---|
| `docs/MISSION.md` | What SPORE is for, and the decision test |
| `docs/SPEC.md` | The technical reference: wire format (normative), application layer, runtime model |
| `docs/REBUILD.md` | Reimplementing in another language, with worked bytes |
| `docs/BRIDGES.md` | Every bridge: wire format, mapping, security profile |
| `docs/DIRECT.md` | SPORE Direct |
| `docs/CONTINUITY.md` | SPORE as a seed; what survives, and what guarantees it |
| `docs/APPS.md` | What to install |
| `docs/ROADMAP.md` | The engineering plan |
| `docs/SECURITY_FINDINGS.md` / `docs/SECURITY.md` | Findings register / how to report |
| `docs/HARDWARE.md` / `android/TESTING.md` | Device evidence |
| `docs/CONTRIBUTING.md` | Freeze rules, CI, branches, releases |
