# Changelog

The v1 wire format is frozen, so this records what changed *around* it: security
fixes, bridges, tooling, and the handful of local-policy changes that alter how a
node behaves without altering what goes on the wire.

Two conventions specific to this project:

- **Wire status is stated on every entry.** "Wire unchanged" means
  `cargo run --example gen_vectors` still produces `reference/vectors.json`
  byte-for-byte, so every reference decoder and every v1 peer is unaffected.
- **Security fixes reference their finding.** `S-0nn` links to
  [`docs/SECURITY_FINDINGS.md`](docs/SECURITY_FINDINGS.md), which carries the
  reproduction, root cause and regression test for each one.

## Unreleased

Nothing yet.

## 0.4.0 — 2026-07-27

- **The release-bump workflow's first run failed on its last line.** Three separate
  defects, all mine: GitHub Actions cannot open pull requests unless the repository
  enables it (off by default, and I did not check), a re-run would have died on the
  branch the failed run left behind, and the "is `## Unreleased` empty" guard counted
  the section's own explanatory boilerplate as content — so it would have cut `0.4.0`
  with a changelog consisting of the text describing what changelogs are for. The PR
  step is now best-effort with a printed link, the push is `--force-with-lease`, and
  the guard requires an actual `- ` bullet.
- **S-030** The tag-cutting step looked up `v0.3.0` but the existing tag was
  `V0.3.0`; git tags are case-sensitive, so it missed it and published a second
  release for the same version. That is S-025's exact trap reproduced inside the fix
  for S-025. Now checks both cases. One accidental upside: the duplicate became
  "latest" and `releases/latest/download/spore-android.apk` returns 206 for the first
  time — a correct outcome from a broken mechanism, which is the most misleading
  state a release pipeline can be in.

Entries accumulate here as work merges. `release.yml` retitles this heading to the
new version when you bump, and refuses to release if it is empty — a release nobody
can read about is the failure this guards against.

Nothing yet.

## 0.3.0 — 2026-07-27

Tagged `V0.3.0` at `27bea16`. Release plumbing only — no Rust behaviour changed, wire
untouched. Both entries are the same finding recurring: a fix verified on the artefact
it was written for and assumed on its neighbour.

### Fixed

- **S-029** The S-026 fix was racy and destroyed the release it repaired. Deleting a
  tag with `--cleanup-tag` and recreating it a second later left
  `nightly-2026.07.27` as a live tag with **no release attached**, on a job that
  reported success at every step. Both release steps now clear the existing
  *assets* and upload over them, never touching the tag or the release. Accumulation
  is still prevented; the cost is that `published_at` lags again, which the release
  name and body make up for.
- **Nightly releases accumulated assets** — the fix for `rolling` (S-021) was not
  applied to the dated nightly beside it, and the versioned filename now embeds a
  minute and a commit sha, so a second merge the same day added a pair rather than
  replacing one. 2026-07-27 ended up holding four assets with nothing marking the
  current one, and its `published_at` sat an hour behind its contents. Today's
  nightly is now replaced per build, like `rolling` (**S-026**).
- **The tag glob was case-sensitive.** `tags: ['v*']` silently ignored `V0.1.0` and
  `V0.2.0`: GitHub created releases for both and no build ever ran, leaving a
  non-prerelease "latest" holding **zero assets** — so
  `releases/latest/download/spore-android.apk`, the link `docs/APPS.md` promises,
  404s from a page that looks like a real release. Worse than having no release at
  all. Now `['v*', 'V*']`, and the tagged path fails loudly if the tag's
  `major.minor` disagrees with `Cargo.toml` — `V0.2.0` was cut while `Cargo.toml`
  still said `0.1.0`, and nothing complained.

Cutting this release also exercised the guard added in it. `V0.3.0` was tagged while
`Cargo.toml` still said `0.2.0`, and the build refused: *"tag V0.3.0 is 0.3.x but
Cargo.toml says 0.2.0"*. That is the drift S-025 was about, caught before it produced
another release nobody could download — bump first, then tag.

## 0.2.0 — 2026-07-27

Tagged `V0.2.0` at `7b2a185`. Wire unchanged. `Cargo.toml`'s `major.minor` is the
only part of a version a human sets, and it must be bumped *before* the tag — the
tagged build now verifies the two agree.

### Security

- **S-022 closed: the prekey ring.** The one-shot seal now has real forward
  secrecy. A node holds up to 16 prekeys, mints a **random** one daily, deletes any
  secret after seven days, and tries every live one when opening — so a sender on a
  stale ANNOUNCE still reaches you until that secret expires. Rotation runs from the
  router's sweep, not from each embedder remembering to ask.
  **This changes what a backup is:** `Node::seed()` no longer restores a node's full
  ability to read its mail, because prekey secrets are random rather than derived
  from the seed — which is the only reason deleting them means anything. Persist
  `Node::prekey_ring()` beside the seed; the browser node and the Android app now
  do. Mail sealed to an expired prekey is unreadable by everyone including the
  recipient, and a backup of the ring defeats the seven-day window. Wire unchanged:
  ANNOUNCE always carried one prekey, it is just a different one each day.
- **S-023** The daemon beaconed a **mesh-wide ANNOUNCE flood every 5 seconds**
  instead of a link-local HELLO every 5 minutes. Two stacked mistakes: the Trickle
  interval was the spec's "5 → 80 min" written as the bare numbers 5 and 80 into a
  timer whose base is seconds (60× too fast), and §4's `hops = 0` HELLO had never
  been implemented, so the frequent beacon was the expensive form. ~45 floods an
  hour against a documented ceiling of one — which on LoRa in EU868 will exceed the
  1 % legal duty cycle. `Node::build_hello` adds the link-local form and the daemon
  runs the two cadences separately. **The Android app had the same bug** — its
  housekeeping loop called `nativeBeacon` every 2-30 s — and is fixed the same way.
- **S-024** Two documentation-versus-code mismatches recorded without fixing: the
  ratchet's skipped-key cache is bounded by count, not by the 7 days §7 claimed
  (and nothing is zeroised on drop), and `mark_seen` computes the 30-day dedup floor
  without a `now`, making its `max` a no-op — harmless, since `ingest` drops expired
  envelopes independently, but it reads as if it does something.

### Fixed

- The freeze guard's own escape hatch did not work. `pr-guard.yml` says "add the
  `allow-frozen-change` label to proceed", but it reads the label from
  `github.event.pull_request.labels` — a snapshot of the payload that started the
  run — and `on: pull_request` defaults to opened/synchronize/reopened. So
  labelling a PR never re-ran the guard, and re-running the failed job replayed the
  original label-free payload. The only way through was to push another commit
  after labelling, which nothing said. `labeled`/`unlabeled` added to the triggers.
### Changed

- **Version scheme.** `major.minor` lives in `Cargo.toml` and is the only
  human-touched part; every merge publishes
  `<major>.<minor>.<YYYYMMDDHHMM>+<short sha>`. No version is derived from a git
  tag again — the previous scheme found the `rolling` tag this workflow creates
  itself and produced "SPORE rolling rolling+2026.07.27".
- "Frozen 1.0 contract" reworded to "frozen **v1** contract" throughout. It was
  always the wire format and API shape that were frozen, never a crate version.

## 0.1.0 — 2026-07-27

Tagged `V0.1.0` at `e41c59a`. **First tagged release.** Three numbers meet here and they version different
things, so rather than let anyone infer it wrongly:

| Number | Versions | Frozen? | Lives in |
|---|---|---|---|
| **SPORE v1** | the wire format — envelope layout, addressing, routing, crypto | yes, CI-enforced | the `VER` byte, `docs/SPEC.md`, `reference/vectors.json` |
| **`spore` 0.1.x** | the crate and the shipped distribution | its *API shape* is CI-enforced | `Cargo.toml`, `tests/api_freeze.rs` |

Freezing the wire at v1 while the software is at 0.1 is not a contradiction. The
protocol is what peers and reimplementations depend on, and it does not move. The
software is early and says so: no radio bridge has been verified against real
hardware — every 🧪 in [`BRIDGES.md`](docs/BRIDGES.md) — and
[`SECURITY_FINDINGS.md`](docs/SECURITY_FINDINGS.md) carried open items, including
that the one-shot seal had no forward secrecy (S-022, closed in 0.2.0). A 1.0 badge
would have said otherwise.

**How the numbers are produced from here.** `major.minor` lives in `Cargo.toml` and
is the only part a human touches — bumping it is a deliberate, reviewable commit.
Everything else is generated: every merge to `master` publishes
`<major>.<minor>.<YYYYMMDDHHMM>+<short sha>`, so a rolling build is uniquely and
monotonically named and points at the exact commit it came from. No version is ever
derived from a git tag again; doing that is how a build ended up called
"SPORE rolling rolling+2026.07.27".

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
- **S-022** The one-shot seal was documented as forward-secret in three places —
  §7 and two comments in `src/seal.rs` — via prekey rotation that does not exist.
  There is one prekey per identity, derived from the seed, forever; and because it
  is a pure function of the seed a node persists, deleting it would achieve nothing.
  The claims are removed rather than softened. No behaviour change; sessions
  (`ratchet`) do have forward secrecy and the docs now distinguish the two.
- **S-021** The Android release advertised a dead "stable release" link, showed a
  three-day-old publication date while building on every merge, accumulated one
  APK per day with no indication which was current, and named itself from its own
  moving tag (`SPORE rolling rolling+2026.07.27`).

### Changed — local policy, not wire

Both alter how a node behaves on a network without changing what it emits, so a
node running this interoperates with a v1 peer that is simply more permissive.

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
  nothing could link to, and `docs/APPS.md` leads with it (**S-021**). Dated
  `nightly-<date>` releases keep the last five builds for rollback, pruned
  automatically so they cannot accumulate the way the old assets did.
- Every in-page anchor on the docs site was dead: 116 of them, including the whole
  bridge index. `marked` emits no heading ids, and the docs are written for GitHub,
  which adds them silently. Headings now carry GitHub's own slugs, and the site
  build fails on any internal link or anchor that does not resolve — which is now a
  CI check, not just a deploy-time one.
- Rust 1.75 compatibility restored in earnest: `zeroize_derive` pinned, lockfile
  regenerated at version 3, and four uses of post-1.75 APIs replaced.

## 1.0.0 — the frozen contract (never tagged)

Not a release; the baseline the freeze is measured against, recorded here because
every entry above is defined by not having moved it. The envelope format, the five
medium shapes, `bindings/spore.h`, `reference/vectors.json` and the reference
decoders. It sits below 0.1.0 in this file because it predates it — the distribution
was first shipped at 0.1.0, on top of an already-frozen v1 protocol.
