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

<!-- Add `- ` bullets here as work merges. This note is a comment so it
     cannot reach a release page; the bump refuses if there are no bullets. -->

- **Android: the JNI audio-output queue is bounded.** The demodulator's completed
  frames sat in an unbounded queue — the mic thread fills it continuously while the
  poll loop drains one frame per tick, so a stalled consumer (or a fast/hostile
  audio feed) could grow it without limit. It now caps at 64 frames and drops the
  oldest on overflow: a demod backlog is stale audio, not data worth keeping, so the
  freshest frames win. Same "bound every cache" hardening as the store and neighbour
  caps.

- **Docs: an Android device-test checklist, and a forward-secrecy note in the app.**
  New `android/TESTING.md` is the repeatable procedure for the things CI can't prove
  because they need a real device — fresh install, upgrade, seed reveal, that the
  identity is **absent** from a cloud/adb backup and a device transfer, a 24–48 h
  soak with no native abort, and the 7-day forward-secrecy window — each with a
  History section to record runs. `docs/ANDROID_AUDIT.md` links it as §6. The app's
  About card now states the forward-secrecy model in plain terms (prekeys rotate on
  a 7-day window; conversation keys ratchet forward; skipped keys drop after 7 days;
  the seed is in encrypted prefs and excluded from backup). The radio air-interface
  paths keep their existing `docs/HARDWARE.md` checklist; the on-device runs remain
  for hardware QA — this ships the procedure ahead of the run so a green build is
  never mistaken for a green device.

- **Store: a spilled envelope is verified against its id on every read, not just
  when adopted (C-ST4).** The spill directory is on disk, where a backup tool, the
  OS, or a corrupted sector can change a file after we recorded it — and its name is
  only a claim about its content. `Store::wire` now bounds the read, decodes, and
  refuses to return bytes whose recomputed id doesn't match the one asked for;
  a mismatch reads as "not held" so the mesh re-fetches a good copy instead of us
  serving a peer bytes that fail their own content check. The adopt path
  (`set_spill_dir`) already did this at startup; this closes the gap on later reads.
  Unit-tested (intact loads, corrupted → None, truncated → None, no panic).

- **Android: profiles reach the mesh — peers pull your photo and name, and re-pull
  when you change them (PR4b).** A peer's avatar now shows on their Nearby row and in
  the conversation list, fetched from *them* on demand: the app asks a peer for its
  profile over the request/response layer (`GET /profile`), and the peer replies with
  a small record — its recommended name plus the ≤256 px JPEG. The reply is only
  trusted if its **authenticated** sender is the very peer that was asked, so a
  flooded forgery can't poison a contact's picture; serving is rate-limited so a
  tens-of-KB reply can't be used to amplify. When you change your name or photo the
  app floods a tiny change-notify on a deterministic per-identity topic, and anyone
  who cached the old one re-pulls. Entirely an application on top of primitives the
  frozen protocol already ships — a request and a reply are ordinary signed DATA
  envelopes, so **no wire-format change** (the golden vectors are byte-identical).
  The one core tweak is internal: an RPC reply now retains its verified sender so the
  caller can check it. Compiled by CI, device QA is a PR6 item.

- **Android: a local profile photo, and the name framed as public.** The Advanced
  screen's name field is now "Name others see," with a live preview of the avatar +
  name exactly as a peer's Nearby row renders them. You can pick a photo; it's
  downscaled to a ≤256 px JPEG off the main thread and cached locally. This is the
  local half (PR4a); PR4b (above) publishes it to the mesh. Compiled by CI, device
  QA is a PR6 item.

- **Android lifecycle hygiene.** The foreground service now tears the node down on
  `onDestroy` — cancels and *joins* the poll/house loops before `nativeFree`, so no
  coroutine reads a freed handle — and a `START_STICKY` restart mints a fresh node
  rather than reusing a dropped `jlong`. `AudioBridge.stop` nulls its record/track
  after release so a stop→start cycle can't reuse a released object. BLE bridges
  reconnect on an unexpected drop with exponential backoff (1s→60s, reset on
  connect, cancelled by an explicit stop) instead of going dead until re-added, and
  the Meshtastic FromRadio drain is single-flighted so a burst of FromNum
  notifications can't stack coroutines racing on one characteristic. Wi-Fi Direct
  starts its UDP flood only once a group is confirmed up (a `CONNECTION_CHANGED`
  receiver + group-info check), not eagerly when the group is merely requested. Wire
  unchanged; Android compiled by CI, device QA is a PR6 item.

- **Bridges can be stopped and removed.** `Hub::unregister(iface)` retires an
  interface by emptying its slot rather than removing it — ids are never recycled,
  because `Flood`'s `except` addresses interfaces by index and a shifting vector
  would silently misroute it. A new `nativeUnregisterIface` JNI call exposes it, and
  the Android bridge list gets a **Remove** that cancels the bridge's pumps and
  unregisters its interface (Audio, BLE, Wi-Fi Direct, Web). Core-owned TCP/UDP show
  no control rather than a dead one — no fake UI. Rust side is unit-tested (stop one
  of two interfaces, the other keeps its id and traffic); the Android side is
  compiled by CI, device QA is a PR6 item. Wire unchanged.

- **Android: chat attachments stage until Send, then arrive as one bubble.** Picking
  a file no longer publishes it immediately — it stages in the composer with a
  remove (✕) affordance, and Send produces a single bubble carrying the text and the
  attachment, identical for sender and receiver (a canonical
  `📎 name | spore:<magnet> | mime` marker, documented in `android/UX-ISSUES.md`).
  Images preview inline (decoded off the main thread, sampled to 1080 px); any file
  opens through a `FileProvider` `content://` chooser that vends only a reclaimable
  cache copy, never the private store. The sealed-to-a-known-peer publish path
  (contents and filename) is unchanged. Not yet device-verified — the `apk` job
  compiles it; manual QA is a PR6 device-matrix item.

- **S-024a:** the Double Ratchet's skipped-key cache is now age-bounded (seven days,
  `SKIP_TTL_SECS = PREKEY_LIFETIME_SECS`) and zeroized on drop, closing the last
  forward-secrecy gap in core crypto. `decrypt`/`skip` take `now`; expired keys are
  purged before use. The session layer and the seal layer now read the same window,
  so SPEC §7's seven-day claim matches the code rather than only the prose. Wire
  unchanged — the ratchet is not on the frozen surface. Field-verification of the
  window on a device is tracked for a later PR.

- **`main.rs` dropped from 799 to 38 lines**, finishing task #23. The CLI binary's
  three concerns moved into `src/cli/{sim,config,run}.rs` — the in-memory demo, the
  config parser, and the config-driven daemon — leaving `main.rs` as just `main()`
  and the dispatch. A pure move: a reconstructed diff against the original shows the
  only content changes are the visibility bumps the sibling-module split required
  (`sim`, `parse_config`, `run_config`, `Spec`, `Config` and its fields → `pub(crate)`);
  every other line is byte-identical, and the demo prints the same output. Binary
  only — no wire contract, no frozen file touched.

- **`lib.rs` dropped from 3977 to 2205 lines.** The 1776-line `impl Node` block
  moved into `src/node/{identity,send,ingest,sync,datagram,files}.rs`, each an
  `impl Node` in a descendant module of the crate root — so the methods keep full
  access to `Node`'s private fields with **no field's visibility widened**. Nine
  private methods called across the new group boundaries became `pub(crate)` (the
  compiler's exact list); their bodies are unchanged. `Node`'s private fields, which
  were already crate-visible via the crate-root descendant rule, are now reachable
  only from the `node::` tree — a slightly *tighter* wall than before. Wire
  unchanged: `reference/vectors.json` reproduces byte-for-byte and the frozen API
  surface is untouched, so no frozen file was edited. Task #23.

## 0.6.0 — 2026-07-28

<!-- Add `- ` bullets here as work merges. This note is a comment so it
     cannot reach a release page; the bump refuses if there are no bullets. -->

- **The Android app now looks like the design language instead of describing it.**
  `VISUALDESIGN.md` §3's shapes — the ammo crate, the Toughbook input with its screw
  dots, the radio-switch button that physically throws, the segmented LED, the
  sticker badges — exist as Compose primitives in `Chrome.kt`, and every screen was
  rebuilt on them from a Claude Design mock. Chat gets right/left-aligned crate
  bubbles; Feed gets inline markdown, image attachments and a dedicated Compose Post
  screen; Bridges is grouped by transport with a status LED per row. Three places
  Android cannot match the spec exactly (no Impact font, the hard shadow is drawn by
  hand because Compose's is blurred, reduced motion is inferred from
  `ANIMATOR_DURATION_SCALE`) are now recorded in the spec rather than left to be
  found as bugs.
- **File transfers report their fragmentation in both directions.** `Msg` carries the
  magnet, so a file bubble reads chunk state out of the existing `transfers` flow
  rather than keeping a second copy that can drift. Incoming shows `have/count ·
  fetching` and fills as chunks land; outgoing fills at once and says "served from
  this node" — not "delivered", because whether a peer pulled a chunk is not
  observable from here.
- **Feed posts can carry an image, referenced from the markdown body.** A post is one
  signed envelope of UTF-8, so the bytes ride the ordinary manifest-and-chunk path
  and the body carries `![name](spore:<magnet>)` pointing at them. Readers without
  the chunks see the transfer fill; clients that do not know the marker see a plain
  markdown image link. Decoding is `inSampleSize`-capped on `Dispatchers.IO`, since a
  phone photo decoded whole for a 220 dp row costs ~100 MB of heap.
- **"Reveal seed" showed `unavailable` on every upgraded install.** The encryption
  change moved the seed into the Keystore-backed store and cleared the plaintext
  file, but the Advanced screen still read the plaintext prefs directly — all the
  call sites in `NodeController` were replaced and none in the UI. The same shape as
  S-015, S-019, S-023, S-025, S-026, S-029 and S-030: verified on the artefact the
  change was written for, assumed on its neighbour. There is now one accessor and the
  UI cannot go around it.
- One thing from the mock was **not** implemented: its "+ subscribe" chip puts pink
  text on kevlar olive, which is 2.32:1 and the single pairing §1 forbids outright.
  It is outlined on void instead. `StickerBadge` takes its own background rather than
  inheriting the crate fill, specifically so this is hard to reintroduce.

## 0.5.0 — 2026-07-27

<!-- Entries accumulate here as work merges, as `- ` bullets. `release.yml`
     retitles this heading to the new version when you bump, and refuses to
     release if there is no bullet under it. This note is an HTML comment
     precisely so it cannot be swept into published release notes — v0.4.0's
     notes ended with the prose version of it, which is not a changelog entry. -->

- **The Android identity seed and every live prekey secret were being uploaded to
  Google Drive.** `allowBackup="true"` plus plaintext `SharedPreferences` meant Auto
  Backup carried both off the device by default, which specifically destroys the
  seven-day forward-secrecy window S-022 added — `CONTINUITY.md` says a backup of
  the ring defeats it, and Android was performing exactly that backup on a schedule
  nobody chose. Now `allowBackup="false"` with extraction rules covering
  device-to-device transfer as well, and `EncryptedSharedPreferences` over a
  Keystore master key, with a migration so an upgrade is not a factory reset.
- **Both Save buttons in the Android app looked broken and were not.** Petname and
  own-name saves persisted on the first click with no snackbar and no visible state
  change. They now confirm, and stay disabled until the field differs from what is
  stored — compared against the value the setter will *actually* write, since both
  trim and one caps at 32. `setMyName` also silently did nothing before the node
  was up; it returns `Boolean` now and the UI says so rather than confirming a save
  that did not happen.
- **S-031** Any sound in the room could saturate a CPU core indefinitely.
  `Demod::push` rescanned its whole retained buffer every call, so work grew with
  the buffer rather than with the new samples — 13 ms per 100 ms push at 1 s
  buffered, 94 ms at 6 s, and the buffer caps at 175 s, about 27x real time. No key
  or protocol participation needed; on Android it runs in a foreground service off
  the mic. A scan cursor makes it flat at 1.5 ms. Found by the discovery audit and
  measured before and after.
- **The visual design language is implemented, not just written.** `VISUALDESIGN.md`
  described an appearance no surface had: `site/style.css` still carried the old
  green-and-blue palette and Android an inline Compose scheme. Both now consume the
  same tokens, with `prefers-reduced-motion` honoured and no webfont anywhere (the
  standalone must make zero network requests). The spec gains an
  implementation-status table so it can never again claim more than the code does.

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
