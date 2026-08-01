# SPORE roadmap — the single living plan

**Project:** `sloev/spore` · **Version:** 0.6.0 (`Cargo.toml`).

This is the one forward-looking plan: the PR map with status, the full detailed PR
bodies (kept, not summarised away), and the docs / Android-UX / palette / site / web
tracks. "What shipped" lives in exactly two places — [`../CHANGELOG.md`](../CHANGELOG.md)
`## Unreleased` and the **Status** column below — so no third progress table can drift.

Each PR is self-contained: files, code sketches, tests, acceptance, CHANGELOG text.
Wire format and C ABI stay frozen; no `allow-frozen-change` is required for this series.
Derived from a static audit of the 0.6.0 tree; update as PRs land and hardware results
arrive.

New here and want the repo map — what lives where, how to build and test each
part — rather than the plan? See [`DEV_GUIDE.md`](DEV_GUIDE.md).

---

## Hard rules (do not violate)

- **Frozen:** wire format, C ABI (`bindings/spore.h`), `reference/vectors.json`, and the
  API surface in `tests/api_freeze.rs`. No change without the `allow-frozen-change` label.
- **Honesty over polish:** 🧪 markers, "Still open", served-vs-fetching language, and
  **no fake UI** — never a control whose backend is missing.
- **[`VISUALDESIGN.md`](VISUALDESIGN.md) is normative** for colour, contrast, motion,
  components. Never pink on olive/kevlar (measured 2.32:1). Never signal failure by
  colour alone (pink is both accent and "bad").
- **Zero external network requests** in `web/spore-standalone.html` (CI greps for it).
- Motion fully static under reduced motion / `ANIMATOR_DURATION_SCALE == 0`. Sound and
  particle bursts stay **off** until the user enables them.
- One concern per PR (CI-green alone). Security P0 and UX are orthogonal when independent.
- Distinguish **Verified** (code/CHANGELOG) vs **Reasoned** vs **Needs device run**. Do
  not claim hardware verification that was not run, or invent protocol features.

## PR write-up template (match this density)

Every proposed PR uses, in order: **1** Title `# PRN — name (finding id)` · **2** Urgency
(P0 security | Critical/High UX | High reliability | Medium | Low | Process | Feature) ·
**3** Status (todo/partial/done) · **4** Why (plain language; cite S-/C-/ANDROID_AUDIT
themes) · **5** Depends / Parallel · **6** Files table (path | modify/new/delete/move) ·
**7** Current shape · **8** Implementation steps (numbered, code sketches in project
style) · **9** Non-goals · **10** Tests (unit/instrumentation/manual device) · **11**
Acceptance (checkboxes) · **12** CHANGELOG bullets (state wire status) · **13** Branch
name · **14** Freeze impact (almost always None).

---

## Principles

1. One concern per PR — reviewable, revertible, CI-green alone.
2. No fake UI — never ship a control whose backend is missing.
3. Honesty preserved — served/fetching language, 🧪 markers, Still open, VISUALDESIGN.
4. Security P0 is orthogonal to UX — PR0 does not block PR1.
5. Micro-commits inside a PR are fine; merge units stay the PR boundaries below.

---

## PR map

| PR | Title | Urgency | Status | Depends | Parallel with |
|----|-------|---------|--------|---------|---------------|
| **PR0** | Ratchet TTL + zeroize **and** offline crypto lifetime knobs | **P0 security + honesty** | ✅ **shipped** — Part A (#40); PR0b ratchet wired into real DM traffic (#70); Part B offline-window knob + Android UI (#71); device field-verify carried to PR6 | — | PR1, PR5 |
| **PR1** | Chat stage/attach/preview/FileProvider | **Critical UX** | 🟡 **partial** — merged (#41); items carried forward | — | PR0, PR5 |
| **PR2** | Hub unregister + bridge stop/remove | **High UX** | 🟡 **partial** — merged (#42); items carried forward | — | PR0, PR1 |
| **PR3** | Service / Audio / BLE lifecycle | High reliability | ✅ merged (#44) | **PR2** | — |
| **PR4** | Name others see + local avatar + mesh profile | Medium product | ✅ 4a merged (#45); 4b (mesh pull) merged (#46) | — | PR3+ |
| **PR5** | Store spilled id verify | Medium hardening | ✅ merged (#47) | — | PR0–PR2 |
| **PR6** | Device matrix + HARDWARE honesty | Process | 🟡 docs in review; on-device runs deferred to hardware QA | PR0–PR3 ideally | — |
| **PR7** | Polish batch | Low | 🟡 demod_out cap in review; a11y/intervals already met; UI features remain | PR4 | — |
| **PR8** | SPORE Direct: negotiated E2E pipe (general) | Feature / product | ✅ core + UDP/TCP adapters merged (#50–#52); mesh glue (PR8c) + BLE/ESP-NOW (PR8d) carried | — (no core freeze) | PR0–PR7 |
| **PR9** | Iroh bridge (QUIC p2p + relay fallback) | Feature / networking | ✅ merged (#53) — `bridge-iroh`, MSRV→1.85, dedicated iroh CI job | — | PR2 helpful for stop/unregister |
| **Docs-1/2/3** | Docs cull → ROADMAP; retire deep-audit + ANDROID_AUDIT; absorb PLAN/UX-ISSUES | Process | ✅ merged (#55/#56/#57) | — | — |
| **B1** | Chat nav: Back + scroll-to-latest + IME insets | Critical UX | ✅ merged (#58) 🧪 | — | B2–B8, C1 |
| **B2** | Send/error feedback (no silent no-op) | Critical UX | ✅ merged (#61) 🧪 | — | B-series |
| **B3** | Empty states + PUBLIC/broadcast confirm | High UX | ✅ merged (#62) 🧪 | — | B-series |
| **B4** | Notifications + transfers overflow | High UX | ✅ merged (#66) 🧪 | — | B-series |
| **B5** | Advanced: ring health + cautious export | Medium | ✅ merged (#67) 🧪 | — | B-series |
| **B6** | Bridges: status enum + permission recovery | High UX | ✅ merged (#68) 🧪 | — | B-series |
| **B7** | Accessibility + density pass | High UX | ✅ merged (#69) 🧪 | B1–B6 | C1 |
| **B8** | Feed polish | Medium | ✅ merged (#72) 🧪 | — | B-series |
| **C1** | Token parity + forbidden-pair audit | High UX | ✅ merged (#73) | — | B7 |
| **Site** | Readability + less generic + more fun UI (à la gitingest.com) | Medium UX | ✅ story cards (#63); contrast pass (#64); copy-code buttons (#65) | — | C1 |
| **P-Direct-NAT** | Direct NAT traversal: STUN reflexive locators + coordinated hole-punch + relay candidate | Feature / networking — **top of priority compass** | ⬜ todo — see "Product decisions" below for the staged plan | PR8 | — |
| **P-Mix-Runner** | Example mix operator + app-level anonymity toggle (mix-preferred / mix-only) | Feature — anonymity path operable | ⬜ todo | `src/mix.rs` (have) | — |
| **P-Group-Roster** | Signal-style membership (signed roster, add/remove epochs, sender binding) | Feature / protocol — **not v1, do not fake in UI** | ⬜ future — sealed-topic + honest UX is the shippable answer today | — | — |
| **W-series** | Web node: browser as a daily-driver peer (Mail/Rooms/Feed/Files/Folder), not a transport demo | Feature / product | ⬜ todo — phased W0–W8, see "Web node as a full daily-driver peer" below | — | — |

**Minimum credible phone node:** PR0 + PR1 + PR2 + one device-matrix pass.

**Direct-pipe track (orthogonal):** PR8 can start anytime; does not block phone-node definition of done.

**Iroh track:** PR9 is a normal bridge (like tor/i2p/tcp): carry SPORE envelopes over iroh QUIC; 🧪 until exercised.

**Note:** Former “PR9 offline lifetime knobs” is **folded into PR0** so crypto default and user-facing policy ship together.

---

## Carried forward from shipped PRs (TODO)

PR0–PR2 each shipped their core and **deliberately deferred** parts. Recorded here
so a deferral does not vanish into a merged PR body. Each notes *why* it was held
and *what unblocks it*. None of these is a regression; they are scope the shipped PR
did not claim to cover.

### From PR0 — ratchet TTL + zeroize (merged #40, `fix/ratchet-skip-ttl`)

Only **Part A** (the crypto: age-bound + zeroize on drop) shipped at first. **Part
B — offline-lifetime knobs and UI** was deferred, then unblocked by PR0b, then
shipped:

- [x] **Daemon config knob** `Node.prekey_lifetime_secs` (default `604800`), a
      single field read by both `sweep_prekeys` (the seal-layer ring) and session
      bootstrap (the ratchet's `skip_ttl_secs`) — one value, not two consts that
      could drift apart. `Node::offline_window_secs()` / `set_offline_window_secs()`
      (clamped to `[PREKEY_PERIOD_SECS, 365 d]`) are the read/write API.
- [x] **Android Advanced presets** — 7 d (default) / 14 d / 30 d / custom, persisted
      via the single secret-accessor pattern (same `secretPrefs()` the seed/ring use).
- [x] **About / security blurb** stating the active window in plain language —
      interpolates the live value instead of a hard-coded "7-day".
- [x] **Decrypt-failure UI** — a failed open of an ENCRYPTED, verified envelope from
      a known contact appends "couldn't decrypt this — the key may have expired, or
      ask them to resend" to that thread, rather than silently dropping it. Mesh
      noise from an unrecognized address still drops silently, as before.
- [x] **Raise-above-default warning** — any preset or custom value above the 7-day
      default routes through the existing `ConfirmDialog` (same component B3/B5 use)
      before taking effect.
- **Why deferred (was):** the Double Ratchet was a tested primitive **not yet wired
      into any production send/receive path** (`decrypt` was called only from tests),
      so a runtime TTL knob would have configured dead code, and the Android sliders
      couldn't be device-verified here — shipping them would break Principle #2 (no
      fake UI).
- **✅ PR0b shipped (#70, `feat/pr0b-ratchet-sessions`): the ratchet is wired into
      real DM traffic.** `Node::send_direct`/`open_dm` now use the §7 Double Ratchet
      once a session exists, falling back to the one-shot prekey seal otherwise.
      Sessions bootstrap from ANNOUNCE (a static-static X25519 DH both sides derive
      independently the moment they know each other's current prekey — no message
      exchange needed), with the numerically-lower address always the pair's
      deterministic initiator so two peers who each message the other before hearing
      back still converge on one session. Wire discriminator: new envelope flag
      `fl::RATCHET` (bit 64, previously unused — forward/backward compatible, `decode`
      already copies flags verbatim). Sessions are in-memory only, like
      `peer_prekeys`/`peer_busy`/`peer_names` — not persisted, self-heal from the next
      ANNOUNCE after a restart. Bounded via the existing `trim_map`/`MAX_PEERS`.
      A real, load-bearing bug was found and fixed along the way: a fresh node's
      bootstrap prekey (`born=0`) rotates unconditionally on its *own* very first
      `on_rx` (an existing, deliberate "upgrade onto the ring" behaviour) — if that
      happened *after* the node had already sent its first ANNOUNCE but *before* it
      processed a peer's, the node would bootstrap its own session using its
      just-rotated prekey while the peer still believed the pre-rotation one was
      current, permanently desyncing that session (never re-seeded by design). Fixed
      by settling any due rotation (`maybe_rotate_prekey`) before *both* building an
      ANNOUNCE and bootstrapping a session from one, so "what I just told the world"
      and "what I use for myself" can never disagree. Found by a flaky test (~50% fail
      rate, tied exactly to which peer's address happened to sort lower) rather than
      assumed away — traced with temporary instrumentation comparing both sides'
      derived keys byte-for-byte before landing on the real cause.
      This unblocked the rest of Part B above.
- **✅ Part B shipped (#71, `feat/pr0-partb-offline-window`).**
- [ ] **Field-verify the offline window end to end** on a device — already tracked in
      **PR6**; unit tests prove the deadline/clamping logic, not a real node's
      clock/delivery.

### From PR1 — chat attachments (merged #41, `feat/android-attachments`)

Shipped: staging, one bubble, image preview, FileProvider Open. Carried forward:

- [ ] **Multiple files per send** (v1 is one attachment per message).
- [ ] **ExoPlayer** audio/video inline preview + playback (v1 previews images only;
      other types are a tap-to-Open file chip).
- [ ] **Edit / remove an attachment after send.**
- [ ] **Merge the bubble for public/unsealed files too.** Only **sealed DM**
      attachments collapse to one bubble today, because only they are guaranteed a
      marker sender; a public or legacy marker-less file still shows the old
      "incoming…/received…" status bubbles. Needs a way to correlate a manifest
      envelope to its magnet at `route()` time (no `nativeEnvId` yet).
- [ ] **Device QA** — stage/remove/send/open, large image, peer-without-file,
      reduced motion — tracked in **PR6**.

### From PR2 — hub unregister + bridge stop/remove (merged #42, `feat/hub-unregister-bridges`)

Shipped: `Hub::unregister`, `nativeUnregisterIface`, Remove for Audio/BLE/Wi-Fi
Direct/Web. Carried forward:

- [x] **Enable/disable *toggle*** distinct from Remove (keep the row, stop/restart
      the transport) — depends on **PR3**'s reconnect/backoff so a re-enable has a
      clean start path. **Shipped (#74, `feat/bridge-enable-disable-toggle`),
      scoped to Audio + the two BLE radios** (Meshtastic, RNode) — the bridges
      whose full config `NodeController` can hold onto and replay on Resume with
      no new UI input. A Pause stops the transport and unregisters its hub iface
      but keeps the row and a restart closure; Resume calls the same closure back
      into the *same* row (a stable `BridgeState.id` now, since `iface` goes to
      `null` while paused and `kind` alone was never unique across two same-type
      BLE bridges). **Wi-Fi Direct and Web deliberately excluded, not
      forgotten:** Wi-Fi Direct's real transport is the core-owned UDP bridge
      (`nativeStartUdpLimited`, no stop hook) — its own `stop()` only tears down
      the P2P group, so a Pause would leave the socket running under a row that
      claims otherwise, the same honesty problem as the TCP/UDP item below. Web
      aggregates however many relays/swarms were added under one row; replaying
      that whole set on Resume isn't built, so it stays Remove-only rather than
      a Resume that quietly comes back empty. Both are real remaining scope, not
      superseded by this PR.
- [ ] **Edit a bridge in place** (change a URL/params) — today it is Remove +
      re-add; needs a native mutate or re-register helper.
- [x] **Stop/remove for core-owned TCP/UDP** (and, per the toggle work above,
      Wi-Fi Direct — it turns out to ride the same core-owned UDP path) — they
      show no control today (honest, not a dead button); a clean core-side stop
      for a specific TCP/UDP iface would let them be removable too, and would
      also be the prerequisite for a real Wi-Fi Direct toggle. **Shipped (#75,
      `feat/core-owned-bridge-stop`).** `bridge::udp::run`/`run_primary`/
      `run_group` and `bridge::tcp::run` (and the shared `driver::run_datagram`/
      `stream_link::run`/`run_reconnecting` underneath every stream/datagram
      bridge, Android or CLI) now take a `stop: Arc<AtomicBool>`, checked once
      per already-short-timeout read — free for every datagram medium, which
      all already poll on a 200ms timeout, but real work for TCP: a blocking
      `TcpListener::accept()` has no read-timeout equivalent, so listen mode is
      non-blocking and polls the flag instead (a plain check after a blocking
      `accept()` would still hang forever with no peer ever connecting — see
      `tcp::accept_or_stop`, covered by a test that proves exactly that
      scenario). JNI's `nativeStartUdp`/`nativeStartTcp`/`nativeStartUdpLimited`
      now return the hub iface (they used to return nothing, so Kotlin never
      even *had* a handle to stop these by); `nativeUnregisterIface` is the one
      call that tears down both a Kotlin-driven and an OS-thread-owned bridge
      now, so no new Kotlin-facing stop API was needed. `WifiDirectBridge.stop()`
      used to only tear down the P2P group and leak the UDP bridge thread
      underneath forever — it now unregisters that too. Folded "UDP broadcast"
      (the always-on default LAN bridge, with no manual re-add UI) into the
      Pause/Resume toggle system alongside Remove, since a plain Remove would
      have been an unrecoverable one-way trip until the app restarts; TCP and
      Wi-Fi Direct got Remove only — TCP because re-adding is a one-line retype,
      Wi-Fi Direct because a real Resume needs the same start/stop restructuring
      as the toggle-capable bridges and wasn't done here (documented as the
      still-open follow-up, not silently dropped). CLI/daemon-only bridges
      (ax25, i2p, reticulum's own TCP/UDP companions, tor) pass a stop flag that
      is constructed and never set — no per-bridge stop control from the CLI
      yet, Ctrl-C still ends the whole process.
- [ ] **Device QA** — add/stop/remove/re-add round-trip on hardware — **PR6**.

---

# PR0 — Ratchet TTL + zeroize + offline crypto lifetime knobs (S-024a + FS/DTN honesty)

## Why
SPEC §7 claims a 7-day window. Code is count-only (`MAX_SKIPPED_KEYS = 2048`). Nothing zeroizes on drop. Last High forward-secrecy gap in core crypto.

## Files
| Path | Action |
|------|--------|
| `src/ratchet.rs` | Primary: type change, purge, Drop, `now` on skip path |
| Call sites of `Ratchet::decrypt` / session wrappers | Pass `now: u32` |
| `src/session.rs` (if it owns ratchet open) | Thread `now` |
| Tests in `src/ratchet.rs` | New TTL + Drop tests |

## Current shape (0.6.0)

```rust
// src/ratchet.rs
const MAX_SKIP: u16 = 512;
const MAX_SKIPPED_KEYS: usize = 4 * MAX_SKIP as usize; // 2048

pub struct Ratchet {
    dhs_sec: [u8; 32],
    dhs_pub: [u8; 32],
    rk: [u8; 32],
    cks: Option<[u8; 32]>,
    ckr: Option<[u8; 32]>,
    nr: u16,
    skipped: HashMap<([u8; 32], u16), [u8; 32]>,  // no age, no zeroize
}
```

`skip` inserts bare `[u8; 32]`; `bound_skipped` drops by count only; no `Drop` impl.

## Implementation steps

### 1. Skipped entry type + TTL constant

```rust
use zeroize::Zeroize;

struct SkippedKey {
    key: [u8; 32],
    inserted_at: u32,
}

const SKIP_TTL_SECS: u32 = 7 * 24 * 3600; // matches SPEC §7 / PREKEY_LIFETIME
```

Map becomes `HashMap<([u8; 32], u16), SkippedKey>`.

### 2. Thread `now` into decrypt/skip

```rust
pub fn decrypt(&mut self, msg: &[u8], now: u32) -> Option<Vec<u8>> {
    self.purge_skipped(now);
    // ... existing header parse ...
    if let Some(sk) = self.skipped.remove(&(dh_pub, n)) {
        let mk = sk.key;
        return Self::open(&mk, n, header, ct);
    }
    // ...
    self.skip(n, now)?;
    // ...
}

fn skip(&mut self, until: u16, now: u32) -> Option<()> {
    if until > self.nr.saturating_add(MAX_SKIP) {
        return None;
    }
    if let (Some(mut ckr), Some(dhr)) = (self.ckr, self.dhr) {
        while self.nr < until {
            let (nck, mk) = kdf_ck(&ckr);
            self.skipped.insert(
                (dhr, self.nr),
                SkippedKey { key: mk, inserted_at: now },
            );
            ckr = nck;
            self.nr += 1;
        }
        self.bound_skipped();
        self.ckr = Some(ckr);
    }
    Some(())
}
```

### 3. Purge by age

```rust
fn purge_skipped(&mut self, now: u32) {
    self.skipped.retain(|_, sk| {
        let live = now.saturating_sub(sk.inserted_at) < SKIP_TTL_SECS;
        if !live {
            sk.key.zeroize();
        }
        live
    });
}
```

### 4. Drop zeroize

```rust
impl Drop for Ratchet {
    fn drop(&mut self) {
        self.rk.zeroize();
        if let Some(ref mut c) = self.cks { c.zeroize(); }
        if let Some(ref mut c) = self.ckr { c.zeroize(); }
        self.dhs_sec.zeroize();
        for (_, mut sk) in self.skipped.drain() {
            sk.key.zeroize();
        }
    }
}

impl Drop for SkippedKey {
    fn drop(&mut self) {
        self.key.zeroize();
    }
}
```

### 5. Call-site wiring

`now` is already `spore::bridge::hub::now()` / Node paths. Grep for `.decrypt(` and `Ratchet::` in `src/session.rs`, `src/node/*`, `src/lib.rs` and pass `now`. Do not invent a second clock.

### 6. Tests (in `src/ratchet.rs`)

```rust
#[test]
fn skipped_keys_expire_after_ttl() {
    // alice encrypts msgs 0..3; bob receives only 3 first -> skips 0..2
    // advance now by SKIP_TTL_SECS + 1; bob must fail to open 0 and skipped.is_empty()
}

#[test]
fn skipped_keys_live_inside_ttl() {
    // same setup; now += SKIP_TTL_SECS - 60; open still works
}
// Keep: the_skipped_key_cache_cannot_grow_without_bound, absurd_gap_refused
```

## Freeze impact
None.

## Acceptance
- [ ] `cargo test` green including new TTL tests
- [ ] `cargo clippy --all-targets` clean under `-D warnings`
- [ ] Spec §7 claim matches behaviour
- [ ] Fuzz targets still build (no API freeze touch)

## CHANGELOG (`## Unreleased`)
```markdown
- **S-024a:** Ratchet skipped-key cache is age-bounded (7 days) and zeroized on drop.
```

## Still open update
Mark S-024a closed in `docs/SECURITY_FINDINGS.md`; leave **field-verification** of the window for PR6.

## Part B — Offline lifetime knobs (FS vs sneakernet honesty)

Sneakernet can deliver ciphertext weeks late; default prekey + ratchet skip TTL are ~**7 days**. Users who expect “message in a bottle” will hit undecryptable sealed mail without explanation. Ship **disclose + optional raise + theft warning** in the same PR as the crypto fix so policy and code cannot drift.

### Config / UI
| Surface | Keys |
|---------|------|
| Daemon YAML / CLI | `prekey_lifetime_secs`, `ratchet_skip_ttl_secs` (default `604800`) |
| Android Advanced | Presets: 7d (default) / 14d / 30d / custom; persist via **single accessor** |
| About / security blurb | “Encrypted DMs readable ~N days offline…” |
| Decrypt-failure UI | “Key expired for offline window; ask resend or raise offline encrypted mail” |

### Implementation notes
- `SKIP_TTL_SECS` and prekey lifetime read from one **policy** object (default 7d), not scattered consts.
- Raising above default requires an explicit warning: longer window ⇒ stolen device reads more history.
- Seed restore must **not** resurrect deleted prekey secrets; only a ring backup does (label that separately).
- Topic/group key rotation (S-020) is **out of scope** for the slider; copy should say sealed DMs / ratchet sessions only.

### Extra acceptance (Part B) — ✅ shipped (#71, `feat/pr0-partb-offline-window`)
- [x] Default remains ~7 days for prekey + ratchet skip TTL
- [x] User/daemon can raise lifetime; value survives restart
- [x] Warning shown when raising above default
- [x] About/Advanced states the active window in plain language
- [x] Failed open of expired sealed mail shows actionable message

### CHANGELOG (single bullet set for PR0)
```markdown
- **S-024a:** Ratchet skipped-key cache is age-bounded (default 7 days) and zeroized on drop.
- Config/UI: offline encrypted-mail window (prekey + ratchet skip TTL) adjustable; longer = more theft exposure.
```

---

# PR1 — Chat attachments: stage -> one bubble -> preview -> Open

## Why
Release APK: pick file publishes immediately; no stage; bubbles lack preview/Open; no FileProvider. Highest daily annoyance. JNI file APIs already exist (`nativePublishFile`, `nativeOpenFile`, `nativeSaveFile`).

## Files
| Path | Action |
|------|--------|
| `android/.../ChatScreens.kt` | Composer staging, marker parse, bubble layout, viewer entry |
| `android/.../NodeController.kt` | Extract publish-only; add `sendTextWithAttachment` |
| `android/app/src/main/AndroidManifest.xml` | FileProvider provider |
| `android/app/src/main/res/xml/file_paths.xml` | **New** |
| `android/UX-ISSUES.md` | **New** — problem, marker, acceptance |

## Current behaviour (broken UX)

```kotlin
// ChatScreens.kt ~183
val pickFile = rememberLauncherForActivityResult(ActivityResultContracts.GetContent()) { uri ->
    // ...
    if (data != null) NodeController.sendFile(peer, name, data)  // immediate publish
}
```

## Implementation steps

### 1. Staging model (composer state)

```kotlin
data class StagedAttachment(
    val name: String,
    val bytes: ByteArray,
    val mime: String,
)

var staged by remember(peer) { mutableStateOf<StagedAttachment?>(null) }

val pickFile = rememberLauncherForActivityResult(ActivityResultContracts.GetContent()) { uri ->
    if (uri == null) return@rememberLauncherForActivityResult
    val name = /* DISPLAY_NAME as today */ ?: "file.bin"
    val mime = ctx.contentResolver.getType(uri) ?: "application/octet-stream"
    val data = ctx.contentResolver.openInputStream(uri)?.use { it.readBytes() }
        ?: return@rememberLauncherForActivityResult
    staged = StagedAttachment(name, data, mime)
    // do NOT call sendFile here
}
```

UI under composer:
- If `staged != null`: chip with name / optional thumbnail + X clears staged.
- Send:
  ```kotlin
  {
      val s = staged
      if (s != null) {
          NodeController.sendTextWithAttachment(peer, text, s)
          staged = null
          text = ""
      } else {
          NodeController.send(peer, text)
          text = ""
      }
  }
  ```

### 2. Send path — one body with marker

```kotlin
// NodeController.kt
fun sendTextWithAttachment(peer: String, text: String, att: StagedAttachment) {
    val magnet = publishFile(peer, att.name, att.bytes) ?: return
    val marker = "📎 ${att.name} | spore:$magnet | ${att.mime}"
    val body = if (text.isBlank()) marker else "$text\n\n$marker"
    send(peer, body)
}
```

Extract publish-only from current `sendFile` so we do not double-append messages.

**Marker (canonical):**
```text
📎 <filename> | spore:<hex-magnet> | <mime>
```
- Last line matching `^📎 .+ \| spore:[0-9a-fA-F]+ \| \S+`
- Application convention only; relays see opaque UTF-8.

### 3. Bubble parse + layout

```kotlin
data class ParsedAttach(val name: String, val magnet: String, val mime: String)

fun parseAttachmentMarker(body: String): Pair<String, ParsedAttach?> {
    val lines = body.lines()
    val last = lines.lastOrNull() ?: return body to null
    val re = Regex("""^📎 (.+) \| spore:([0-9a-fA-F]+) \| (\S+)$""")
    val m = re.matchEntire(last) ?: return body to null
    val text = lines.dropLast(1).joinToString("\n").trimEnd()
    return text to ParsedAttach(m.groupValues[1], m.groupValues[2], m.groupValues[3])
}
```

Bubble (preserve side + border for mine/theirs):
- message text (marker stripped)
- attachment chip / image preview
- existing SegmentedLed + served/fetching
- time / lock / badges

Rules:
- `image/*` + enough bytes: `inSampleSize` from ~280.dp on `Dispatchers.IO` (reuse Feed).
- Incomplete: placeholder + LED only — never partial corrupt bitmap.
- Tap chip -> AttachmentViewer.

### 4. FileProvider

**AndroidManifest.xml** (inside `<application>`):
```xml
<provider
    android:name="androidx.core.content.FileProvider"
    android:authorities="${applicationId}.files"
    android:exported="false"
    android:grantUriPermissions="true">
    <meta-data
        android:name="android.support.FILE_PROVIDER_PATHS"
        android:resource="@xml/file_paths" />
</provider>
```

**res/xml/file_paths.xml** (new):
```xml
<?xml version="1.0" encoding="utf-8"?>
<paths>
    <cache-path name="attachments" path="attachments/" />
    <files-path name="store" path="store/" />
    <external-files-path name="ext" path="." />
</paths>
```

### 5. AttachmentViewer + cache

```kotlin
val cacheRoot = File(ctx.cacheDir, "attachments/${magnet.take(16)}")
cacheRoot.mkdirs()
val out = File(cacheRoot, safeName)
if (!out.exists()) {
    val bytes = SporeNative.nativeOpenFile(ptr, magnet) ?: return
    out.writeBytes(bytes)
}
val uri = FileProvider.getUriForFile(ctx, "${ctx.packageName}.files", out)
val intent = Intent(Intent.ACTION_VIEW)
    .setDataAndType(uri, mime)
    .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
ctx.startActivity(Intent.createChooser(intent, "Open"))
```

- Preview/open: `cacheDir` (reclaimable).
- Explicit Save: externalFiles / MediaStore.
- Eviction: ~14 days or ~50 MB.
- Never write sealed ciphertext world-readable.

### 6. Crypto honesty
- Same sealed-publish path as today's `sendFile` when peer key known.
- Keep "served from this node" / "fetching".

### 7. Docs
Create `android/UX-ISSUES.md`: problem, current vs target, marker, acceptance, non-goals.

## Non-goals (v1)
Multi-file per send; ExoPlayer; editing after send.

## Acceptance
- [ ] Pick -> composer chip; thread unchanged until Send
- [ ] Remove staged -> text-only Send
- [ ] Send -> **one** bubble text+attach for sender and receiver
- [ ] LED while fetching; preview when image decodable
- [ ] Open via FileProvider for images
- [ ] No pink-on-olive; contentDescription on chips
- [ ] Sealed path preserved

## CHANGELOG
```markdown
- Android: chat attachments stage until Send; one bubble with preview; FileProvider Open/Share/Save.
```

## Manual QA
1. Stage image + text, Send -> one bubble both sides.
2. Stage then X -> Send text only.
3. Large image -> LED then preview; Open works.
4. Peer without file -> fetching language, no crash on Open.
5. Reduced motion respected.

---

# PR2 — Hub unregister + bridge stop/remove UI

## Why
Hub slots and JNI ifaces only grow. UI cannot stop/delete bridges. A live-looking switch without backend repeats the "Save did nothing" failure mode.

## Files
| Path | Action |
|------|--------|
| `src/bridge/hub.rs` | `unregister(iface)` |
| `android/jni/src/lib.rs` | `nativeUnregisterIface` |
| `android/.../SporeNative.kt` | external fun |
| `android/.../NodeController.kt` | stop helpers, list removal |
| `android/.../NodeScreens.kt` | BridgeRow actions |
| Bridge classes | Call unregister from existing `stop()` |

## Current shape

```rust
pub fn register(&self) -> (Iface, Receiver<Forward>) {
    let mut o = lock(&self.out);
    let iface = o.len() as Iface;
    o.push(Slot { tx: Some(tx), bulk: None });
    // never shrinks
}
```

```kotlin
external fun nativeRegisterIface(ptr: Long): Int
external fun nativeRegisterIfaceLimited(ptr: Long, bulkBytesPerSec: Int): Int
// no unregister
```

## Implementation steps

### 1. Hub unregister

```rust
/// Drop the outbound sender for `iface`. Receivers see disconnect.
/// Slot stays as a hole so iface indices remain stable.
pub fn unregister(&self, iface: Iface) {
    let mut o = lock(&self.out);
    if let Some(slot) = o.get_mut(iface as usize) {
        slot.tx = None;
        slot.bulk = None;
    }
}
```

Do **not** `Vec::remove` (would renumber). Document: iface ids never recycled within process lifetime.

### 2. JNI

```rust
#[no_mangle]
pub extern "system" fn Java_org_spore_node_SporeNative_nativeUnregisterIface(
    _env: JNIEnv, _class: JClass, ptr: jlong, iface: jint,
) {
    if let Some(r) = rt(ptr) {
        r.ifaces.lock().unwrap().remove(&iface);
        r.hub.unregister(iface as Iface);
    }
}
```

### 3. Kotlin façade

```kotlin
external fun nativeUnregisterIface(ptr: Long, iface: Int)
```

### 4. Wire bridge stop()

Each bridge that registered an iface must:
1. Cancel pump jobs
2. `nativeUnregisterIface(ptr, iface)`
3. Release platform resources

Store `iface: Int` on the bridge instance or in `BridgeState` at register time.

```kotlin
fun stopBridge(kind: String) {
    when (kind) {
        "Audio" -> audioBridge?.stop()
        "Meshtastic BLE" -> meshtasticBridge?.stop()
        // ...
    }
    bridges.value = bridges.value.filterNot { it.kind == kind }
}
```

### 5. BridgeRow UI

```kotlin
@Composable
private fun BridgeRow(b: BridgeState) {
    // existing LED + kind + detail + status word
    Row {
        CrateButton("Stop", { NodeController.stopBridge(b.kind) }, enabled = b.canStop)
        CrateButton("Remove", { NodeController.stopBridge(b.kind) })
    }
}
```

- Edit (URL bridges): Remove + re-add until native mutate exists.
- Unsupported stop: omit or disable with caption — **never** a grey no-op switch.

### 6. Tests
- Rust: register two, unregister first, forwards only to live slots.
- Android manual: add Audio, Stop, Remove, re-add.

## Acceptance
- [ ] Remove drops UI row, stops pumps, unregisters iface
- [ ] Start/Stop round-trip for >=1 Kotlin and >=1 URL bridge
- [ ] Unsupported Stop does not look live
- [ ] No renumbering of still-live ifaces

## CHANGELOG
```markdown
- Hub/JNI: unregister interface; Android bridge list supports stop/remove.
```

---

# PR3 — Android lifecycle hygiene (depends on PR2)

## Why
Sticky restart can orphan native handles; AudioBridge stop not re-entrant-safe; BLE drain stacks coroutines; no reconnect; Wi-Fi Direct may start UDP before group is up.

## Files
| Path | Action |
|------|--------|
| `NodeService.kt` | Controlled shutdown on destroy |
| `NodeController.kt` | `stopFromService()` |
| `AudioBridge.kt` | Null fields after release |
| `BleBridges.kt` | Single drain job; reconnect backoff |
| `WifiDirectBridge.kt` | Wait for group up |

## Implementation steps

### 1. Service shutdown

```kotlin
// NodeService.kt
override fun onDestroy() {
    multicastLock?.release()
    multicastLock = null
    NodeController.stopFromService()
    super.onDestroy()
}

// NodeController
fun stopFromService() {
    // cancel housekeeping / poll jobs
    audioBridge?.stop(); audioBridge = null
    // all bridges stop (unregister via PR2)
    val p = ptr
    if (p != 0L) {
        SporeNative.nativeFree(p)
        ptr = 0L
    }
}
```

Sticky restart must `nativeNew` again — never reuse a freed jlong.

### 2. AudioBridge.stop

```kotlin
fun stop() {
    rxJob?.cancel(); txJob?.cancel()
    rxJob = null; txJob = null
    try { record?.stop(); record?.release() } catch (_: Exception) {}
    try { track?.stop(); track?.release() } catch (_: Exception) {}
    record = null; track = null
}
```

### 3. Meshtastic single drain

```kotlin
private var drainJob: Job? = null

fun requestDrain(g: BluetoothGatt) {
    if (drainJob?.isActive == true) return
    drainJob = scope.launch {
        try { /* existing FromRadio loop */ }
        finally { drainJob = null }
    }
}
```

### 4. BLE reconnect

On DISCONNECTED: status `"reconnecting"`; exponential backoff 1s, 2s, 4s … cap 60s; cancel on explicit stop/Remove.

### 5. Wi-Fi Direct

Start UDP only after CONNECTION_CHANGED / group info confirms iface up.

## Acceptance
- [ ] Destroy does not leave zombie pumps after sticky restart
- [ ] AudioBridge start -> stop -> start without crash
- [ ] Rapid FromNum does not stack drains
- [ ] BLE disconnect shows status and backs off

## CHANGELOG
```markdown
- Android: service/native shutdown; AudioBridge stop hygiene; BLE single-drain + reconnect.
```

---

# PR4 — Profile: "Name others see" + local avatar

## Why
ANNOUNCE already carries petname; Nearby already prefers local -> quoted announced -> address. UX does not frame the field as public-facing. No avatar.

## Files
| Path | Action |
|------|--------|
| `NodeScreens.kt` / Connect / Advanced | Label + live preview |
| `NodeController.kt` / prefs | Avatar magnet in EncryptedSharedPreferences |
| `ChatScreens.kt` Nearby / headers | Optional avatar slot |

## Implementation steps

### 1. Name copy
- Label: **"Name others see"**
- Live preview matching `ChatsList` Nearby rules
- Existing dirty-enabled Save + confirm stays

### 2. Local avatar
1. Pick image; size-cap (max edge 256 px / <=128 KB) before publish
2. Publish via existing file API -> magnet
3. Store magnet via **single seed/prekey accessor pattern** (load-bearing)
4. Show on own Connect/profile/header; letter fallback

### 3. Mesh avatar — PR4b, shipped as an RPC **pull** (not a topic push)
Investigating the topic-record option showed the avatar can't ride it: fetching a
file by a bare magnet doesn't bootstrap the manifest (`Node::missing` returns empty
without it), so a peer reading a record could never pull the image. Chosen design
instead (no wire-format change):
- **Pull.** A peer asks us for `GET /profile` over the existing request/response
  layer; we reply with `"SPR1" · nameLen[2] · name · avatarLen[4] · avatar(JPEG)`.
  Tens of KB doesn't fit one envelope, so the reply is sent through the
  fountain-fragmenting `send` path (the core's `respond` can't fragment) and
  reassembles into the RPC demux on the caller.
- **Authenticity.** An RPC reply now retains its verified `Src::Full` sender
  (`take_response_from`); the caller drops any reply whose sender isn't the peer it
  asked, so a flooded forgery can't poison a contact's avatar. Serving is
  rate-limited (one reply per requester per cooldown) against amplification.
- **Notify + advertise.** On name/photo change we flood a tiny marker on a
  deterministic per-identity topic `spore:profile:<addr>`; watchers re-pull. The
  reply's name is the advertised recommended petname.
- **Core change:** internal only — `rpc_responses` keyed value gains the sender
  `Addr`. Wire vectors byte-identical; `api_freeze` unaffected.

## Acceptance
- [x] Preview matches Nearby (PR4a)
- [x] Set/change local avatar; visible on own surfaces (PR4a)
- [x] Size cap enforced before publish (PR4a, ≤256 px / ≤40 KB)
- [x] Peer avatar pulled on demand, verified to come from that peer, and cached (PR4b)
- [x] Change floods a notify; watchers re-pull (PR4b)

## CHANGELOG
```markdown
- Android: "Name others see" framing; local profile avatar publish/cache.
- Android: mesh profile — peers pull name+avatar over RPC, verified + rate-limited,
  with a change-notify on a per-identity topic. No wire-format change.
```

---

# PR5 — Store spilled-file id verify (C-ST4)

## Why
Spill path trusts filename == content id. Bit-rot or replaced file would be served as valid.

## Files
| Path | Action |
|------|--------|
| `src/store.rs` | Verify on `wire()` disk read |

## Current

```rust
Body::Evicted => std::fs::read(self.spill.as_ref()?.join(filename(id))).ok(),
```

## Implementation

```rust
Body::Evicted => {
    let path = self.spill.as_ref()?.join(filename(id));
    let bytes = std::fs::read(path).ok()?;
    if let Ok((env, n)) = crate::envelope::Envelope::decode(&bytes) {
        if n == bytes.len() && env.id() == *id {
            return Some(bytes);
        }
    }
    None // treat as not held; mesh can re-fetch
}
```

Confirm exact `Envelope::decode` / `id()` names in `envelope.rs`.

## Tests
- Intact spill -> OK
- Corrupt one byte -> None
- Truncated -> None

## Acceptance
- [x] Intact spill loads
- [x] Mismatch -> None, no panic
- [x] No freeze surface touch (only `src/store.rs`; wire vectors unchanged)

## CHANGELOG
```markdown
- Store: verify spilled envelope id on disk read.
```

---

# PR6 — Field verification + docs honesty

## Why
Still open: no field-verified 7-day FS; all radios 🧪; HARDWARE.md procedure only; Android backup exclusion untested on device.

## Deliverables

### 1. Device matrix (`docs/ANDROID_AUDIT.md` / `android/TESTING.md`)

| # | Test | Pass criteria |
|---|------|---------------|
| 1 | Fresh install | Node starts, address shown |
| 2 | Upgrade from prior APK | Seed reveal works |
| 3 | Reveal seed | Matches; single accessor |
| 4 | adb/cloud backup attempt | Identity + prekey ring **absent** |
| 5 | Device transfer | spore prefs/store absent |
| 6 | 24–48 h soak | No native crash |
| 7 | 7-day FS procedure | Documented; result when run |

### 2. HARDWARE.md
Run procedure for one of Meshtastic BLE or RNode **or** demote README/APPS radio readiness language.

### 3. Surface Still open
Docs site + app Advanced/About short FS blurb (prekey 7d; ratchet age-bounded after PR0).

## Acceptance
- [x] Checklist exists (`android/TESTING.md`, 7 rows + History), linked from ANDROID_AUDIT §6
- [ ] Backup + migration run once on hardware — **deferred to hardware QA** (no device in CI)
- [x] Marketing already honest: README caveats radios ("need real hardware to test"),
      HARDWARE.md marks every radio 🧪 with a History section — no demotion needed
- [x] Forward-secrecy blurb in the app's About card

## CHANGELOG
```markdown
- Docs: Android device-test checklist; app forward-secrecy blurb. Radio readiness
  already honest (HARDWARE.md 🧪 + History); on-device runs remain for hardware QA.
```

---

# PR7 — Polish (batchable)

Landed in slices. Status per item:

| Item | Sketch | Status |
|------|--------|--------|
| demod_out cap | JNI VecDeque max ~32, drop oldest | ✅ in review — bounded at 64, drops oldest (the security-relevant one: unbounded audio-output queue) |
| contentDescription | LEDs, badges, attachment chips | ✅ already covered — attachment/feed/QR/profile images are labelled; status badges render readable text |
| Housekeeping assert | Android intervals match SPEC 5->80 min / 1 h | ✅ already correct — `ANNOUNCE_FLOOD_INTERVAL_MS` = 1 h (§5.4b); HELLO is the frequent link-local form |
| Ring health UI | `Prekeys: N live · oldest Xd · next mint ~Yh` + Export with FS warning | ⬜ todo (UI feature) |
| Group key_id badge | Warn on mismatch; never claim roster consensus | ⬜ todo (UI feature) |
| Boot receiver | Optional, **default off** | ⬜ todo (UI feature) |
| Sound/particles | Behind setting, default off | ⬜ todo (UI feature) |
| Docs sync | Android bridge list ⊆ BRIDGES.md | ⬜ todo (honesty check) |
| Docs sync | Android bridge list ⊆ BRIDGES.md |

---


---

# PR8 — SPORE Direct: negotiated non-routed E2E pipe (general)

## Why
Apps need a **general low-latency encrypted pipe** (voice, telemetry, interactive data, optional streams) between two SPORE identities when a **direct underlay** exists (UDP, ESP-NOW, TCP, BLE, …). Store-and-forward remains correct for async mesh; it is the wrong plane for full-duplex media.

This milestone implements that as an **application-level profile** on top of existing unicast send/recv — **no envelope, store, hub, or freeze-surface changes**.

## Goals
1. **Negotiate** on the SPORE plane: throughput need (`min_bps` / MTU / optional latency), cipher suite, ephemeral key agreement, and a **subset of mediums** that are E2E-capable and meet capacity.
2. **Open one direct port** on the chosen medium.
3. **Same frame/record spec on every medium**; only the adapter differs.
4. **General pipe** — not audio-only: `DATA` / `MEDIA` / optional channels; reliability optional *above* the pipe.

## Non-goals
- Routing direct records through the postcard mesh
- Changing T0–T2 relay behaviour
- Guaranteeing latency on LoRa / duty-cycled radios (reject or PTT-fallback only)
- Replacing SIP/Asterisk feature-complete PBX

## Files (suggested layout — new code only)

| Path | Action |
|------|--------|
| `direct/` or crate `spore-direct` | Offer/answer, HKDF, record AEAD, `Pipe` API |
| `direct/port.rs` | `DatagramPort` trait: `mtu` / `send` / `try_recv` / `close` |
| `direct/udp.rs` | UDP adapter (1 datagram = 1 record) |
| `direct/tcp.rs` | TCP adapter (`u16be len ‖ record`) |
| `direct/espnow.rs` | Optional / `cfg` — ESP-IDF or stub + docs |
| `docs/DIRECT.md` | **New** — profile, threat model, candidate table |
| `examples/direct_udp.rs` | Loopback or localhost two-peer smoke test |
| Signaling only | Existing `Node::send` / `send_direct` with opaque `SPDR` payloads |

Do **not** modify `src/envelope.rs`, `store.rs`, `hub.rs`, or frozen contract files.

## Signaling (`SPDR` app payloads)

Magic prefix `SPDR` + version byte so other apps ignore the payload.

**OFFER**

```text
ver, pipe_id (16 B),
min_bps, max_bps, mtu_needed, max_latency_ms (optional),
features: datagram | multiplex | …,
cipher_suites: ["chacha20poly1305"],
eph_pub (X25519),
candidates[]: { medium, locator, est_bps, mtu, rtt_hint? }
```

**ANSWER**

```text
pipe_id, status ok|reject,
chosen candidate (or reject reason: no_medium | throughput | busy),
eph_pub, agreed_bps, agreed_mtu, cipher
```

**CLOSE** / optional **REKEY** on the SPORE plane only.

Negotiation messages should be sealed/signed like normal peer traffic so `eph_pub` is bound to the SPORE identity.

## Medium selection

Local capability table (not in core):

```text
medium   e2e?   est_bps     mtu
udp      yes    high        ≥1200
espnow   yes    ~200–500kb  ~250
tcp      yes    high        ≥1400 (framed)
ble      yes    low–med     20–200
spore    fallback only      envelope path — not for full-duplex voice
```

Algorithm: intersect offer candidates with local E2E capability; drop if `est_bps < min_bps` or `mtu < mtu_needed`; sort by latency then capacity; else `reject/throughput` or `no_medium`.

## Key schedule

```text
shared = X25519(eph_a, eph_b)
tx_key, rx_key, salt = HKDF-SHA256(
  shared,
  info = "spore-direct-v1" || pipe_id || addr_a || addr_b || medium
)
```

Bind both SPORE addresses into `info`. Media keys never appear on the wire.

## Record format (same on every medium)

```text
offset  size  field
0       1     ver = 1
1       1     type  (0=MEDIA 1=KEEPALIVE 2=CONTROL 3=DATA 4=STREAM …)
2       2     seq   u16 BE
4       4     pipe_id trunc
8       n     AEAD ciphertext + tag
```

- **UDP / ESP-NOW:** one packet = one record  
- **TCP / serial / BLE:** `u16be length ‖ record` (BLE may chunk further)

Pipe is **best-effort datagram**. Optional ordered `STREAM` or RPC retries live in the app library, not in the outer AEAD record (avoids HOL on voice).

## API sketch

```rust
// conceptual — spore-direct
let offer = PipeOffer {
    min_bps: 5_000,
    mtu_needed: 64,
    features: Feature::DATAGRAM,
    candidates: local_candidates(),
};
let mut pipe = Pipe::negotiate(&node, peer_addr, offer)?;
// uses node only to send/recv SPDR; then:
pipe.send(Type::Data, payload)?;
while let Some((ty, bytes)) = pipe.poll() { /* … */ }
pipe.close()?;
```

## Tests
- Unit: offer/answer encode/decode; candidate filter (throughput / mtu / empty → reject)
- Unit: record seal/open; seq; wrong peer binding fails MAC
- Integration: two processes, UDP port, `DATA` round-trip
- Optional: TCP framing round-trip
- Document ESP-NOW as 🧪 until hardware procedure exists (same honesty as bridges)

## Status (increment 1 — the pure protocol core)

Landed in `src/direct.rs` (+ `docs/DIRECT.md`, `examples/direct_loopback.rs`), a new
`pub mod direct` — additive, no envelope/store/hub/wire change, vectors byte-identical,
compiles on wasm:
- `SPDR` OFFER/ANSWER codec (bounds-checked reader; truncated/alien → `None`).
- Key schedule: X25519 (reusing `ratchet::keypair`) + BLAKE2b KDF binding both addrs,
  pipe id and medium; directional tx/rx keys; media keys never on the wire.
- AEAD record `ver·type·seq·pipe_id[..4]·ChaCha20-Poly1305`, header authenticated as AAD.
- `choose()` medium selection (throughput/MTU filter → latency-then-capacity rank →
  `NoMedium`/`Throughput`).
- `DatagramPort` trait + in-memory `Loopback`; `Pipe` (`offer`/`answer`/`finish`/
  `send`/`poll`).
- 8 unit tests: offer/answer round-trips, malformed→None, candidate filter, full
  negotiate→data round-trip both ways, a record only opens on the pipe that
  negotiated it, an AAD tamper fails the MAC, wrong pipe-id doesn't finish.

**Deferred to increment 2 (transport, not protocol):** real `direct/udp.rs` +
`direct/tcp.rs` adapters and the two-process socket integration test; the mesh
signalling glue that carries `SPDR` over `send_direct`; `CLOSE`/`REKEY`; ESP-NOW
adapter documented 🧪 until a hardware procedure exists.

## Acceptance
- [x] Negotiation selects a medium that meets `min_bps` + MTU or rejects clearly (`choose`)
- [x] Keys derived; records authenticate; no media in the SPORE store (records ride the underlay)
- [ ] Same record bytes work on ≥2 real adapters (UDP + TCP framing) — increment 2
- [x] General `DATA` path works without any audio codec in tree
- [ ] Zero changes to frozen contract / envelope layout
- [ ] `docs/DIRECT.md` states threat model (underlay can drop/delay/record ciphertext)

## CHANGELOG
```markdown
- **Direct (optional):** application profile for negotiated E2E datagram pipes (throughput + medium + key); same record, per-medium ports; does not alter v1 relay behaviour.
```

## Branch
```
feat/spore-direct-pipe
```

## Depends / parallel
- **No dependency** on PR0–PR7 for a library + UDP example
- Android/ESP32 UI and Codec2-on-pipe can follow as PR8b / separate apps
- Complements session layer (`src/session.rs`): sessions ride envelopes; Direct rides a sideband port after SPORE signaling
- Offline sealed-mail windows are **PR0 Part B**; Direct (PR8) does not replace them

---

# PR9 — Iroh bridge (QUIC p2p envelopes)

## Why
[Iroh](https://github.com/n0-computer/iroh) (n0) gives **QUIC** connections between endpoints identified by keys, with **hole punching** and **relay fallback** when direct paths fail. That fills a gap between LAN UDP and Tor/I2P: internet-friendly peer paths without requiring a stable public IP.

Fit for SPORE: a **shape-2 byte-stream / datagram** bridge — same role as `tcp` / `tor` / `stream_link`, carrying opaque SPORE envelopes. Router stays medium-agnostic.

## Fit / non-fit

| Iroh provides | SPORE use |
|---------------|-----------|
| Endpoint ID (key-based) | Locator in bridge config / invite candidate — **not** a replacement for SPORE Ed25519 identity |
| QUIC streams or datagrams | One envelope per datagram **or** length-prefixed frames on a stream (match existing stream bridges) |
| Relay servers (public or self-hosted) | Optional fallback underlay; document trust (relay sees ciphertext only if envelopes are sealed; metadata/timing still visible) |
| NAT traversal | Main product win vs raw UDP |

**Not** a substitute for store-and-forward mesh; offline peers still use S&F. Iroh is for **when both ends can reach the network**.

## Suggested design

```text
bridge::iroh
  - Endpoint from iroh (or configured secret)
  - Map: dial peer by iroh EndpointId exchanged out-of-band
    (SPORE invite payload / ANNOUNCE extension / manual config)
  - On connect: stream_link-style pump
      poll SPORE forward queue → write framed envelopes
      read frames → hub.on_rx
  - register_limited bulk budget like other internet bridges
  - 🧪 until integration test + manual two-host run
```

### Framing
Prefer reuse of existing patterns:
- **Datagram path** if iroh exposes unreliable datagrams sized for envelopes, or
- **Stream + length prefix / KISS** consistent with `stream_link` / TCP bridges so code stays familiar.

### Identity binding (important)
- SPORE address ≠ iroh EndpointId unless you deliberately derive one from the other (usually **don’t** — keep layers separate).
- After connect, only accept/forward envelopes that verify as usual (S-002 class: no trust from underlay id alone).
- Optional: first message must be a signed SPORE hello binding SPORE addr ↔ this session.

### Dependencies / supply chain
- Add `iroh` behind a **Cargo feature** e.g. `bridge-iroh` so default/MSRV/offline-bundle builds stay lean.
- Check `deny.toml` licences; pin versions compatible with MSRV 1.75 if possible — **if iroh requires newer Rust**, document “iroh bridge needs toolchain ≥ X” and keep feature off in MSRV CI (same pattern as nightly-only fuzz).
- Public relay use: document in BRIDGES.md (phone-home, operator trust, self-host option).

## Files (indicative)

| Path | Action |
|------|--------|
| `src/bridge/iroh.rs` | **New** — endpoint, dial, frame, pump |
| `src/bridge/mod.rs` | Feature-gate module |
| `Cargo.toml` | Optional dep + feature `bridge-iroh` |
| `docs/BRIDGES.md` | Table row + security notes + 🧪 |
| `docs/HARDWARE.md` or networking runbook | Two-machine checklist (direct + relay path) |
| Daemon config | `iroh:` section: endpoint secret path, relay URLs, peer EndpointIds |
| Android (optional follow-up) | Only if JNI/feature story is clear; desktop/daemon first is enough |

## Tests
- Unit: framing round-trip with mock read/write
- `#[cfg(feature = "bridge-iroh")]` integration: two endpoints localhost / relay if available in CI
- Fuzz: framer only if custom parser (prefer proven length-prefix)

## Acceptance
- [ ] Feature-gated build; default CI green without iroh
- [ ] Two nodes exchange sealed SPORE envelopes over iroh
- [ ] Works with relay fallback documented (or explicit “direct only” mode)
- [ ] Unregister/stop clean when PR2 exists
- [ ] BRIDGES.md entry with 🧪 and trust notes (relays, EndpointId ≠ SPORE id)
- [ ] Licence / MSRV story documented if toolchain diverges

## CHANGELOG
```markdown
- Bridges: optional `bridge-iroh` — QUIC p2p (hole punch + relay fallback) for envelope transport (🧪).
```

## Branch
```
feat/bridge-iroh
```

## Notes vs PR8 Direct
- **Iroh bridge:** underlay for **SPORE envelopes** (mesh/S&F still applies once injected into the hub).
- **SPORE Direct (PR8):** sideband **non-stored** pipe after negotiation; can later list `iroh` as a **Direct candidate medium** once both exist — not required for iroh PR9.

---

# Track: docs consolidation ✅

Culled the overlapping status/plan docs down to canonical roles (Docs-1/2/3, merged
#55/#56/#57). `docs/` is now: `SPEC`, `DESIGN`, `BRIDGES`, `SECURITY_FINDINGS`,
`VISUALDESIGN`, `APPS`, `CONTINUITY`, `HARDWARE`, `DIRECT`, **`ROADMAP`**. Retired
`SPORE_DEEP_AUDIT.md` (its plan is this file) and `ANDROID_AUDIT.md` (open items below,
device checks in [`../android/TESTING.md`](../android/TESTING.md)); absorbed
`android/PLAN.md` (→ README milestones) and `android/UX-ISSUES.md` (→ `VISUALDESIGN.md`
Appendix A, attachment-marker regex preserved verbatim). Rule: CHANGELOG + this file's
Status column are the only "what shipped" surfaces.

---

# Track B — Android shell UX (each a full PR; one concern each)

The M0–M5 app works but the shell is weak. Each of these is 🧪 until PR6 runs on a device
(no Android SDK in CI; the `apk` job only compiles). Expand steps from `ChatScreens.kt`,
`FeedScreens.kt`, `NodeScreens.kt`, `MainActivity.kt`, `Chrome.kt`, `NodeController.kt`,
`NodeService.kt`, the bridge classes, and the manifest.

## B1 — Chat navigation: BackHandler + scroll-to-latest + IME insets — ✅ merged (#58)
**Why:** single `var screen` stack, so system Back leaves the app from a nested screen;
the chat list doesn't stay on the latest message; the keyboard covers the composer.
**Files:** `MainActivity.kt`, `ChatScreens.kt`, maybe `Chrome.kt`.
**Steps:** hierarchical Back (Chat→Chats, Compose→Feed, else→Chats; only Chats
backgrounds); `rememberLazyListState` + `LaunchedEffect(thread.size) { scrollToItem }`;
`imePadding()` on the composer; keep 48 dp targets.
**Non-goals:** full Navigation-Compose rewrite. **Acceptance:** Back never exits from a
nested chat; new message visible; composer usable with the keyboard open.

## B2 — Send/error feedback — ✅ merged (#61)
**Why:** the plain-text `send` path clears the composer even when `ptr==0` or the native
call fails — silent data loss.
**Files:** `ChatScreens.kt`, `NodeController.kt`, the Feed compose send path.
**Steps:** send helpers return `Boolean`/result; snackbar via `LocalSnackbar` on failure;
clear the composer only on success; mirror the existing "Node not started" path from
`setMyName`. **Acceptance:** a failed send keeps the text and says why.

## B3 — Empty states + PUBLIC/broadcast confirm — ✅ merged (#62)
**Why:** empty chat/feed/bridge lists give no guidance; PUBLIC is a mis-tap from
everyone. **Files:** `ChatScreens`, `FeedScreens`, `NodeScreens`, `Chrome` (Baud).
**Steps:** one plain-language empty line + a decorative `aria-hidden` Baud; a confirm
dialog before a PUBLIC/broadcast send; **no** unread badges (no read tracking → would be
fake). **Acceptance:** first-run comprehensible; PUBLIC requires confirm.

## B4 — Notifications + transfers overflow — 🟢 in review (#66)
**Why:** the foreground-service notification is static; `TransfersBar` shows only the
first few. **Files:** `NodeService.kt`, `MainActivity.kt` (Transfers/ReceivingBar).
**Steps:** notification text with short address / peer count / "relaying"; tap opens
MainActivity; transfers show `+N more`. **Non-goals:** per-message notification privacy
design. **Acceptance:** notification informative; no crash at zero peers.

**Shipped (`feat/android-b4-notifications-overflow`).** `NodeService.buildNotification()`
now shows the address's first 8 hex chars (or "starting…" before the node is up), a
peer count, and "relaying" once `storeCount` shows we're holding envelopes for the mesh
— refreshed via a coroutine that collects `NodeController.address`/`peers`/`storeCount`
and re-notifies on change (`NotificationManagerCompat.notify` silently no-ops without
`POST_NOTIFICATIONS` rather than throwing, so a pre-permission cold start or zero peers
can't crash it). Tapping the notification opens `MainActivity` via a `PendingIntent`.
`TransfersBar` now shows `+N more` once active transfers exceed the 3 it already
displayed, instead of silently dropping the rest.

Not verified by an actual build: this environment has no Android SDK (network-restricted
— `sdkmanager` can't reach `dl.google.com`), and CI's `apk` job is the only place this
compiles, matching the established pattern for the earlier B1–B3 PRs. Verified by careful
manual review instead: existing imports/signatures cross-checked against `NodeController`'s
actual `StateFlow` types, `Caption`'s real signature, and the already-granted
`POST_NOTIFICATIONS` manifest permission + runtime request in `MainActivity`.

## B5 — Advanced: ring health + cautious export — 🟢 in review (#67)
**Why:** the audit asked for prekey-ring health; any export defeats the 7-day window.
**Files:** `NodeScreens` (Advanced), `NodeController` (`secretPrefs` accessor only).
**Steps:** show live count / oldest age / next mint (thin JNI read, no widened freeze);
Export only behind a "this defeats the 7-day window" modal; single accessor, no second
seed copy (S-015 class). **Non-goals:** adjustable lifetime sliders (PR0b gate — dead-code
UI forbidden). **Acceptance:** health readout; export gated + warned.

**Shipped (`feat/android-b5-ring-health-export`).** The "thin JNI read" turned out to need
a small new core method, not just a wrapper: `Node::prekey_health(now)` (new,
`src/node/identity.rs`) returns `(count, oldest secret's age, seconds to next rotation)` —
`None` for the age when the oldest entry is an unstamped bootstrap key whose true age is
unknowable (§7), and `0` for "next rotation" when it's already due, never an underflowed
giant number. A new `android/jni` export (`nativePrekeyHealth`, additive over the frozen
core, not the `bindings/` C/Python/Go/JS surface) serialises it as
`"count:oldestAge:nextMintIn"`; `NodeController` parses it into a `RingHealth` each
housekeeping tick, same cadence as peers/storeCount. The Advanced screen shows it plainly
("held: N · oldest: Xh", "next rotation: due" / "in Xh"). Export reuses the existing
`ConfirmDialog` (the same component B3 added for the PUBLIC-send confirm) warning that a
copy defeats the 7-day window, and only then reveals the ring hex — read through a new
`NodeController.prekeyRingHex()` accessor that mirrors `seedHex()` exactly (reads the
persisted ring out of the same encrypted `secretPrefs`, so there's no second path into that
store to drift from it, the S-015-class mistake this asked to avoid). No lifetime sliders
or other dead-code UI (PR0b gate respected).

Verified: `cargo test --lib` (175 passed, including a new
`prekey_health_reports_unknowable_age_and_due_rotation_honestly` covering the fresh-node,
just-rotated, and overdue cases), `cargo clippy -- -D warnings` clean on both the core crate
and `android/jni`, `python3 bindings/generate.py` produces no diff (this is additive JNI,
not the frozen C surface). The Kotlin UI itself is **not** build-verified: this environment
has no Android SDK (network-restricted — `sdkmanager` can't reach `dl.google.com`), so as
with B4, CI's `apk` job is the only place it actually compiles; reviewed by hand against the
real `RingHealth`/`Caption`/`ConfirmDialog` signatures instead.

## B6 — Bridges: status enum + permission recovery (no fake toggle) — 🟢 in review (#68)
**Why:** the LED uses brittle substring matching on status strings; a denied permission
dead-ends. **Files:** `NodeScreens` (BridgeRow), bridge classes, manifest flows.
**Steps:** map status → `up/connecting/down/error`; deep-link to app settings when
mic/BT/nearby is denied; **Remove stays the control** — no Start/Stop `Switch` until
start/stop is real for that row. **Acceptance:** no fake switch; denied permission has a
recovery path.

**Shipped (`feat/android-b6-bridge-status-permission-recovery`).** A new
`classifyBridgeStatus` (private, `NodeScreens.kt`) maps every bridge's free-text status to
a `BridgeStatus` (`Up/Connecting/Down/Error`) enum by **exact match** over the small, known
vocabulary every source actually emits (`NodeController`'s own literals, `BleBridge`,
`WifiDirectBridge`, `WebBridgeHost`'s JS-side events) — not the blind substring `in` checks
it replaces. Those checks were real, live bugs, not hypothetical: `"disconnected"` contains
`"connect"`, so a genuinely dropped BLE link read as *connecting*; `"unsupported"` contains
`"up"`, so an unsupported Wi-Fi Direct device read as *up* — both found by tracing every
status string each bridge class actually emits, not by trusting the old substring list.
`Error` now gets its own treatment (pink + a `⚠` icon, never colour alone per
VISUALDESIGN) instead of fading into the same "down" kevlar as an idle bridge.

Permission recovery: `BridgesList`'s `withPerms` now takes a label, and a denial (rather
than dead-ending silently, as before) opens a `ConfirmDialog` naming what needs the
permission with an "Open settings" action that deep-links to
`ACTION_APPLICATION_DETAILS_SETTINGS` for this app. Covers Audio modem (RECORD_AUDIO),
Meshtastic/RNode BLE (BLUETOOTH_CONNECT ≥31), and Wi-Fi Direct (NEARBY_WIFI_DEVICES ≥33 /
ACCESS_FINE_LOCATION below).

**Remove stays the control** — confirmed unchanged: no Start/Stop `Switch` was added or
exists; `canStop` still gates the one real control per row.

Not build-verified in this environment (no Android SDK, network-restricted — same
limitation as B4/B5); CI's `apk` job is the compile check. Reviewed by hand: traced every
status string each bridge source (`BleBridges.kt`, `WifiDirectBridge.kt`,
`WebBridgeHost.kt`, `NodeController.kt`) can actually produce against
`classifyBridgeStatus`'s cases, rather than assuming the vocabulary.

## B7 — Accessibility + density pass — 🟢 in review (#69) (depends B1–B6)
**Why:** icon-only 📎, LEDs/chips without descriptions, sub-48 dp targets, no way back to
the bottom of a scrolled chat. **Files:** `Chrome.kt`, `ChatScreens`, `FeedScreens`,
`MainActivity` (bottom nav). **Steps:** `contentDescription` on LEDs/chips/Baud/nav/📎; 48
dp targets; focus order → composer; jump-to-bottom FAB when scrolled up; verify
reduced-motion is fully static. **Acceptance:** TalkBack usable for send/attach/open; no
pink-on-olive.

**Shipped (`feat/android-b7-accessibility-density`).**
- **Icon-only controls now announce a real name, not the raw glyph.** `CrateButton` gained
  an optional `contentDescription` param (default `null`, so every word-labelled button —
  the overwhelming majority — is unchanged); when set, it overrides the merged accessible
  name and clears the label `Text`'s own semantics so TalkBack doesn't read both. Applied to
  every icon/symbol-only button traced across the app: Attach file (📎) and Remove staged
  attachment (✕) in `ChatScreens`, and Bold/Italic/Code/Insert link/Add image in the Feed
  composer's formatting row (`B`/`i`/`</>`/`🔗`/`🖼`) — TalkBack previously read these as
  literally "B" or "slash" or the bare emoji. `MainActivity`'s TopBar icons (←/👋/⚙, via a
  new small `IconTap` helper) get the same treatment.
- **Chips and bottom-nav tabs now announce selection, not just colour.** `TopicChip` and
  `BottomNav`'s three tabs switched from `.clickable` to `.selectable` (`role = Role.Tab`),
  which reports selected/unselected to TalkBack — the pink-vs-amber colour swap alone never
  did. LEDs (`BridgeRow`'s dot) were checked too: already invisible to the accessibility tree
  (no semantics of their own) and always paired with adjacent status text, so nothing was
  silently unlabelled there to begin with. "Baud" (the roadmap's original mascot name) is now
  the 🍄 emoji prefixed onto the screen-title heading — decorative flourish on real heading
  text, not an isolated control, so left as-is.
- **48dp touch targets.** Every `CrateButton` was under the floor (~36dp tall from its own
  padding); it now carries `Modifier.sizeIn(minWidth = 48.dp, minHeight = 48.dp)` directly —
  a real bigger button, not an invisible hitbox extending past a small-looking one. Bottom-nav
  tabs (~33dp) got `heightIn(min = 48.dp)`.
- **Focus order → composer.** A chat's petname-edit field sat above the thread and would
  otherwise be a keyboard/TalkBack user's first stop on entering a conversation, ahead of the
  actual reason they're there. `ChatDetail` now requests focus for the message composer on
  entry via `FocusRequester`.
- **Jump-to-bottom when scrolled up.** The B1 auto-scroll unconditionally yanked the thread
  back to its newest message on every arrival, even for a reader who had deliberately scrolled
  into history — the exact gap B1's own PR flagged and deferred to B7. It now only
  auto-follows when the reader is already at the bottom (tracked via `derivedStateOf` over
  `LazyListState.layoutInfo`); otherwise a "↓ new" button appears to jump back manually,
  animated normally or instantly under reduced motion.
- **Reduced-motion re-verified, not just re-declared.** Grepped every animation API
  (`animate*`, `Animatable`, `AnimatedVisibility`, `rememberInfiniteTransition`) across the
  Android sources: only the mascot sparkle and the CRT scanlines use motion, and both were
  already gated on `reducedMotion()`. No new gap found; the new jump-to-bottom button also
  respects it (`scrollToItem` instead of `animateScrollToItem`).
- **No pink-on-olive** — checked every new/changed element; none put pink on kevlar.

Not build-verified in this environment (no Android SDK, network-restricted — same
limitation as B4–B6); CI's `apk` job is the compile check. Reviewed by hand instead,
including a brace/paren balance pass on every changed file.

## B8 — Feed polish — ⬜ todo
**Why:** follow-topic fails quietly; image decode can blank; links are untappable by
design. **Files:** `FeedScreens.kt`, `Markdown.kt`. **Steps:** snackbar on follow
success/fail; placeholder on decode error; long-press **copy** URL without auto-opening
(keep the security posture). **Acceptance:** errors visible; no drive-by link open.

## B9 — Optional later (still full template if proposed)
Merge public/unsealed attachment into one bubble (needs magnet correlation, careful
native id without a frozen-ABI break); multi-file attach; reactions; ExoPlayer (a
dependency decision); Lite mode disabling WebView bridges for battery (measure first).

### Android engineering carried forward (from the retired production audit)
Distinct from the visual UX above: received-file `FileProvider`/`ACTION_VIEW` (cached
attachments open today; an arbitrary received file doesn't); a **JNI local-reference**
audit of the poll loops (`DeleteLocalRef`/frames) + a soak run, since that aborts under
load rather than degrading; **WebView battery** (measure with Battery Historian; cap/pool/
`destroy()`; Lite mode — per-process isolation needs an IPC layer the in-process `jlong`
node doesn't have, not a manifest attribute); foreground-loop lifecycle gating +
`MulticastLock` release on Wi-Fi drop; **permissions at point of use** (request at
bridge-enable with rationale; `BLUETOOTH_SCAN neverForLocation`; don't fight Doze — say
so in the UI).

---

# Track C — visual / palette

Normative tokens ([`VISUALDESIGN.md`](VISUALDESIGN.md) §1): `void #0a0a0c` (page),
`asphalt #1a1c20` (panels), `kevlar #4b5320` (crate fill/disabled), `amber #ffb000`
(primary text), `phosphor #39ff14` (success/live), `pink #ff2a85` (accent CTA, also "bad"
with an icon), `cyan #00ffff` (focus/selection), `edge #2a2f1c` (borders), `dim #8a7a4a`
(de-emphasised). Semantic: bg=void, panel=asphalt, ink=amber, accent=pink, accent2=cyan,
ok=phosphor, warn=amber, bad=pink+icon.

## C1 — Token parity + forbidden-pair audit — ✅ merged (#73)

**Why:** the design language rots when the four surfaces drift (VISUALDESIGN,
`site/style.css`, the web-standalone tokens, Android `Chrome.kt` Palette).

**Finding.** (1), the token table, turned up one real bug, not just drift:
`web/spore-standalone.html` (generated by `web/build-standalone.mjs`) was never
on the palette at all — a leftover generic dark-blue/green scheme from before
VISUALDESIGN existed, despite the doc's own Implementation-status table
claiming it "inherits the stylesheet." An S-022-shaped claim with no
implementation behind it. `site/style.css` and Android's `Chrome.kt` Palette
were already correct, hex-for-hex.

| Token role | `site/style.css` | `web/spore-standalone.html` (before → after) | Android `Chrome.kt` |
|---|---|---|---|
| bg / void | `#0a0a0c` | `#0e1116` → `#0a0a0c` | `#0A0A0C` |
| panel / asphalt | `#1a1c20` | `#171b22` → `#1a1c20` | `#1A1C20` |
| kevlar (disabled face) | `#4b5320` | *(absent)* → `#4b5320` | `#4B5320` |
| edge | `#2a2f1c` | `#262c36` → `#2a2f1c` | `#2A2F1C` |
| ink / amber | `#ffb000` | `#e6edf3` → `#ffb000` | `#FFB000` |
| dim | `#8a7a4a` | `#8b98a9` → `#8a7a4a` | `#8A7A4A` |
| accent / pink | `#ff2a85` | `#57c785` → `#ff2a85` | `#FF2A85` |
| accent2 / cyan | `#00ffff` | `#4aa3ff` → `#00ffff` | `#00FFFF` |
| ok / phosphor | `#39ff14` | *(absent)* → `#39ff14` | `#39FF14` |
| mono stack | `ui-monospace, "Cascadia Mono", "Fira Code", …` | `ui-monospace, SFMono-Regular, …` → matches site | *(Compose `FontFamily.Monospace)`* |

Light mode ("Field Notes") re-derived the same way from `site/style.css`'s
`prefers-color-scheme: light` block.

**Steps (2)–(6):**
- (2) Grepped every `Palette.Pink` use in the Android app — all five
  pink-faced `CrateButton`s already pair `face = Pink` with `ink = Void`
  (established by earlier B-series work); `StickerBadge`'s pink-ink uses
  default `bg = Void`, never `Kevlar`. No violations found.
- (3) `Send`/`New post`/invite-accept/confirm-dialog buttons: confirmed pink
  face + void ink on Android (pre-existing); added the same pairing to the
  standalone's `button` rule (was pink-on-hardcoded-near-black, now
  `var(--bg)`, i.e. void, and adapts correctly in light mode too).
- (4) The standalone had **no** `:focus-visible` styling at all — every
  interactive element (`a`, `button`, `input`, `select`, `textarea`) now
  gets the 2px cyan ring, matching Toughbook/site convention. Android already
  had this via Compose's Toughbook field + button focus states.
- (5) The standalone had **no** `:disabled` styling either — once its button
  face became pink (this PR), an unstyled `:disabled` would have rendered
  exactly the forbidden "translucent pink." Added `button:disabled` /
  `button.ghost:disabled` using the new `--kevlar` token + dim label. Android
  already did this correctly (disabled face is `Asphalt`, not `Kevlar` —
  deliberately, since `CrateButton`'s own *default enabled* face is already
  Kevlar, so a Kevlar-for-disabled rule would make default enabled/disabled
  buttons indistinguishable; recorded here rather than mechanically forcing a
  literal reading of "kevlar" that would regress the working Android case).
- (6) Reduced-motion gating on Android (scanlines, bloom, mascot sparkle) was
  already re-verified by B7 ("no new animation gaps found"); nothing to redo.
  The standalone has no ambient VFX to gate (none implemented).

Also fixed as part of (1): the standalone's `.log .rx` (received) and
`.badge.open` (bridge up) used `--accent`, which happened to be a
success-reading green in the old palette but is now the primary-action pink —
remapped to the new `--ok` (phosphor) token so "it worked" stays green, not
pink, matching the semantic mapping in §1.

**Verified:** rebuilt the wasm + regenerated `web/spore-standalone.html`,
screenshotted with Playwright/Chromium in both dark and light
`prefers-color-scheme`, and drove a real send→loopback-bridge→receive round
trip to confirm the restyle didn't break function — log coloring, the cyan
focus ring (real `Tab` keypresses, not just programmatic focus), and the
pink/void button all render as intended in both themes.

**Acceptance:** parity table above; no forbidden pairs (verified, not just
asserted); VISUALDESIGN §7 checklist re-walked.

## C2 — Conservative palette refinements (only if WCAG-safe) — ⬜ todo
Document in VISUALDESIGN **before** code: nudge `--dim`/`--edge` only if measured contrast
fails on real panels; an optional distinct `--bad` hue **only** if still paired with
icon/text; tone large phosphor success text for OLED fatigue but keep LED segments hot.
**Do not** add an Android light theme (bunker is dark; the site has Field Notes), webfonts,
or CDN images. **Non-goals:** rebrand, new mascot, Impact-font download, Material-3 dynamic
colour.

---

# Track: site — readability + less generic + more fun UI

**Why (your ask).** The docs site (`site/`, rendered to GitHub Pages by `build.mjs`) reads
generic and the colours are hard on the eyes; you want it to feel more like the fun,
characterful UI of **gitingest.com** — more visual identity and UI elements, without
betraying the Neo-Tokyo-Tactical-Wasteland design language or the hard rules (self-hosted
only, zero external requests, reduced-motion static, WCAG contrast).

**Files:** `site/build.mjs` (the generator + inlined `style.css`), `site/home.md`, the
per-page front matter. Any token change is coordinated with **C1** so the four surfaces
stay in parity.

**Steps (proposed; refine into a PR):**
1. **Readability first (contrast).** Audit body text against the panel colours on a real
   monitor — amber `#ffb000` on void is fine for headings but tiring for long body copy;
   consider a slightly desaturated ink for paragraphs and reserve full amber/phosphor for
   headings, code, and LEDs. Larger base font + line-height; comfortable measure
   (`max-width` on prose). Every change re-measured to keep §1 contrast (no regressions,
   no pink-on-olive).
2. **Less generic — visual identity.** A distinctive home hero (the mascot 🍄, the crate/LED
   motif, a tactical-wasteland masthead), section dividers with character, and a consistent
   "crate" card component for doc index tiles rather than a plain vertical list — the
   equivalent of gitingest's playful single-purpose landing.
3. **More UI elements.** Card grid for the docs index; copy-to-clipboard on code blocks; a
   sticky in-page table of contents for long docs; token/color chips; status/🧪 badges as
   real components. All pure HTML/CSS/inline-JS — **no external fonts, images, or scripts**
   (CI greps the standalone; keep the site to the same bar).
4. **Keep the constraints.** Self-contained (inline CSS/JS, data-URI assets); theme-aware
   (the site already does light "Field Notes" + dark); reduced-motion fully static; every
   internal link still resolves (`build.mjs` link check).

**Non-goals:** a web framework or bundler; external CDNs/fonts/images; animation that isn't
static under reduced motion; changing the design tokens unilaterally (that's C1's parity
job); touching the zero-network `spore-standalone.html` guarantee.

**Acceptance:** measured contrast passes on real panels for body copy; the home page reads
as *SPORE*, not a generic template; a card-grid docs index + at least one new reusable UI
element (e.g. copy-code) shipped; `node site/build.mjs` → "links OK"; no external request
added (grep clean).

## First increment — illustrated story cards on Home, Apps, Continuity

**Shipped (this PR, `feat/site-story-cards-user-pages`).** Step 2 above, scoped to the three
pages a first-time visitor actually lands on. `site/home.md`, `docs/APPS.md`, and
`docs/CONTINUITY.md` now open with a grid of `<figure class="story-card">` — a small
self-hosted inline-SVG illustration (CSS/tokens only, no rasters), a one-line caption, and
the full prose moved into `<details>` so the page reads as a scan-then-dive layout instead
of a wall of text. Reference/dense pages (Spec, Design, Bridges, Direct, Rebuild, Security
Findings, Hardware, Testing, VisualDesign, Roadmap itself, Changelog, Contributing,
Bindings, Reference, Web guide) are deliberately untouched — a story-card treatment would
work against their job.

Two real bugs found by actually building and screenshotting the pages (Chromium via
Playwright, not just reading the CSS) rather than trusting the source as given:
- Two CTA links pointed at `demo/spore-standalone.html` / `demo/spore-seedsheet.html`,
  which never exist at those paths — the Pages workflow (`pages.yml`) copies the built
  files to `spore-standalone.html`, `demo/` (index), and `spore-seedsheet.html` as
  siblings, never nested under `demo/`. Fixed to the real paths.
- The art column's `grid-row` span (a fixed guess) didn't match every card's actual row
  count, so Continuity's 4-`<details>` "cold-start" card had its last row fall out from
  beside the art. Fixed to the true max row count across all cards (5), after an
  intermediate `1 / -1` attempt turned out to resolve against the *explicit* grid (which
  this layout has none of) and silently reshuffled sibling placement instead — caught by
  re-screenshotting, not assumed fixed.

Verified: `node site/build.mjs` → "links OK"; reduced-motion actually disables the SVG
keyframe animations (checked via computed style, not just the media-query source); light
and dark themes both render coherently; no pink-on-olive; decorative art is `aria-hidden`.
Contrast pass on body copy (step 1) and the docs-index card grid (rest of step 3) remain
open for a follow-up increment.

## Second increment — a calmer body-copy tone (step 1: readability)

**Shipped (`feat/site-readability-contrast`).** Step 1 above. `--amber` on `--void` already
clears 10.80:1 — nowhere near a WCAG failure — but a fully saturated, high-luminance colour
glowing on near-black is tiring across paragraphs in a way a bare contrast ratio doesn't
capture. New semantic token **`--prose`** (`#d6af5c`, the same hue at ~60% saturation,
still 9.56:1 on void / 8.24:1 on asphalt — comfortably past 7:1/AAA) now colours every
long-form `<p>`, `<li>`, and `<td>` inside `main.doc` — every doc page, not just the three
story-card ones, so the dense reference pages (Bridges, Spec, Security Findings, …) get the
same relief. Headings, code, buttons, LEDs, and badges are untouched (still full `--ink`) —
short bursts of text don't cause the fatigue this addresses, so they keep their punch. Light
mode aliases `--prose` straight back to `--ink`: dark ink on cream paper is already the
comfortable pairing, so there's nothing to desaturate there. Documented in
`docs/VISUALDESIGN.md` §1 before the CSS changed, per this project's own convention.
Android's `Chrome.kt` Palette is **not** touched here — noted as a candidate for the C1
token-parity pass, not assumed done.

Verified by rendering, not just reading the CSS: candidate desaturation levels were
computed (WCAG relative-luminance formula) and rendered side by side before picking one,
then the actual built pages (Home, Apps, Continuity, Bridges) were screenshotted in both
themes to confirm headings stayed vivid while body copy visibly calmed down, and that light
mode was untouched.

## Third increment — copy-to-clipboard on every code block

**Shipped (`feat/site-copy-code-buttons`).** First of step 3's "more UI elements" list. A
small "Copy" button now sits top-right on every `<pre>` across every doc page (not just
Home/Apps/Continuity), injected by a page-wide script in `build.mjs`'s shared `page()`
template — unlike the existing share bar, which is `index.html`-only and stayed that way,
this needed every page with a code block. Styled with existing tokens only (`--void` face,
`--dim` label, `--edge` border, `--ink` hover, `--accent2` focus ring); click copies via
`navigator.clipboard.writeText`, flashes "Copied ✓" for 1.6s, and falls back to a
"Select + copy" message rather than throwing if the Clipboard API is unavailable. Hidden
under `@media print`, same as the share bar. Documented in `docs/VISUALDESIGN.md` §3 before
shipping, per convention.

Verified functionally, not just visually: a genuine Playwright clipboard test (granted
`clipboard-read`/`clipboard-write` permissions) confirmed `navigator.clipboard.readText()`
matches the code block's text for both an always-visible button and one nested inside a
collapsed `<details>` (opened first, as a real user would), and that the label flashes and
reverts on schedule. Both themes screenshotted, including the hover state. Remaining step-3
items (docs-index card grid, sticky table of contents, status/🧪 badges) are open for a
follow-up increment.

---

# Track E — web (lighter; still a full PR if proposed)

Standalone empty-state + visible bridge errors; a one-liner on identity-in-localStorage vs
theft (continuity-honest, not fear-mongering); **do not** break the zero-network CI; Direct
in the browser only after the mesh glue (PR8c) exists; keep `web/README`'s transport table
aligned with `BRIDGES.md` honesty.

---

## Suggested calendar

```
Week 1 — ship blockers
  d1–3  PR0  ratchet TTL + zeroize + offline lifetime knobs/UI
  d2–4  PR1  attachments                 || PR0
  d4–5  PR2  unregister + bridge UI

Week 2 — harden phone node
  d1–2  PR3  lifecycle (after PR2)
  d2–3  PR4  profile local
  d3    PR5  store spill verify          || anytime
  d4–7  PR6  device matrix + HARDWARE honesty

Later
  PR7   polish (ring health, key_id badge, boot, a11y, …)

Parallel anytime (non-blocking)
  PR8   spore-direct library + docs/DIRECT.md + UDP/TCP example
  PR8b  Codec2 / ESP-NOW / call UI (after PR8)
  PR9   bridge-iroh (feature-gated; desktop/daemon first)
```

## Branch naming

```
fix/ratchet-skip-ttl          # includes offline lifetime knobs/UI
feat/android-attachments
feat/hub-unregister-bridges
fix/android-lifecycle
feat/android-profile-local
fix/store-spill-verify
docs/device-hardware-matrix
feat/spore-direct-pipe
feat/bridge-iroh
```

## Definition of done — credible phone node

- [x] PR0 merged — FS claim matches code **and** offline window disclosed + configurable
- [ ] PR1 merged — attachments usable end-to-end
- [x] PR2 merged — bridges stoppable/removable (real for every kind as of #75:
      Audio, BLE, Wi-Fi Direct, Web, and now the core-owned UDP/TCP bridges too)
- [ ] >=1 device-matrix pass (backup exclusion + migration)
- [ ] One radio path checked in HARDWARE.md **or** marketing demoted

---

## Audit ID index

| PR | IDs |
|----|-----|
| PR0 | S-024a, C-R1, C-R2 + FS/DTN knobs (ex-PR9) |
| PR1 | A-NS3, Part VII §62, Patch F |
| PR2 | A-N2, C-H4, A-NS2, Patch B |
| PR3 | A-S1, A-A2, A-B1/B2/B6, A-W1, C-B1, Patch D/E |
| PR4 | Part VII §63 |
| PR5 | C-ST4, Patch C |
| PR6 | Still open field gaps, D-1 |
| PR7 | UX-1/2, A-M3, A-NC3, A-J3, C-D1 |
| PR8 | Design discussion (Direct plane); no prior S-nnn — new optional profile |
| PR9 | Iroh bridge; BRIDGES.md 🧪 pattern (tor/i2p class) |

---

## Out of scope

- Wire / C ABI changes
- Group membership consensus protocol
- Multi-file attach, in-app video, post-send edit
- Claiming 🧪 radios production-ready without HARDWARE results
- Full release-pipeline fixture automation (note only)
- Routing Direct records through store-and-forward relays (Direct is non-routed by definition)

---

## Web node as a full daily-driver peer (W-series)

**Steering:** `MISSION.md` pillar 3 (Façades) + pillar 4 (Nodes people can run —
"Android, desktop, browser/wasm, daemon, ESP/home router") — the browser is a
first-class node, not a demo toy. Source: an implementer plan
(`SPORE_WEB_NODE_FULL_PLAN.md`) reviewed against the current tree; findings and
scope below are from that review, not the plan verbatim.

In the vocabulary of [`DESIGN.md`](DESIGN.md)'s "The spore and the soil": the
browser is one **vessel** among several, and the communicator this track builds
is an **extension of that runtime**, not part of the core. Thin soil in one
respect that shapes the scope below — no disk to spill the store to, and the node
stops when the last tab closes.

### Confirmed: standalone HTML, "web node," and the site demo are one file

`web/build-standalone.mjs`'s own header says it plainly: the single generated
`spore-standalone.html` "is also the live demo served on the site." Verified in
the pipeline itself — `.github/workflows/pages.yml` copies the exact same file
to both `_site/spore-standalone.html` and `_site/demo/index.html` (its comment:
*"The live demo IS the standalone: one self-contained file... Serve the same
file at /demo/"*). One artifact, three names. There is no separate "demo" to
diverge from the "real" node — improving one is improving all of it, and CI's
zero-external-network check (`ci.yml`, greps the file for `import`/`export`/
`https?://`) already guards that it stays a real offline node, not a
CDN-backed page.

### Confirmed: today it's a transport demo, not a daily-use node

Reading `web/build-standalone.mjs`'s actual UI (not just its stated intent):
three panels — **This node** (address, a raw hex-dest + payload compose box,
subscribe-to-topic), **Bridges** (add/remove/inspect transports), **Seed &
memory** (export/forget). `spore_node_send`/`spore_node_recv` in `src/wasm.rs`
are the only send/receive exports, and `Node::send` is the raw
unsealed/unsigned path — the same call a protocol implementer would use to
verify a transport works, not what a user sends a friend. There is no thread
view, no file library, no topic-room concept, no feed. This matches the source
plan's own diagnosis (its phase **W0** is literally "audit wasm API vs Node
capabilities, gap list, no fake buttons") — the audit is confirmed accurate:
**yes**, it is currently explanatory/demo-shaped, and **yes**, the goal below
(daily-use functional node) is the right direction, not scope creep.

### The real gap is `src/wasm.rs`, not just UI

The Rust core already has everything the plan's feature table needs —
`Node::send_direct`/`open_dm` (sealed + §7 ratchet, shipped in #70), `topic.rs`
(encrypted topic membership/rotation), `feed.rs` (topic-scoped microblog), and
`file.rs`/`bundle.rs`/`node/files.rs` (`publish_file`, `publish_file_sealed`,
`fetch`, `open_file`, content-addressed manifests — all present, all tested).
None of it is reachable from the browser: `src/wasm.rs` exports exactly 15
functions today, covering node lifecycle, raw send/recv, and the prekey ring —
no DM, no topic, no file, no feed call among them. So **W1 (encrypted DM)
cannot start with UI work** — it starts with new `wasm.rs` exports, the same
shape as `android/jni`'s existing pattern (an additive C-ABI-style surface
over the frozen core, itself not part of the frozen contract — confirmed
against `CONTRIBUTING.md`'s frozen-file list, which names `bindings/spore.h`,
`reference/vectors.json`, and `tests/api_freeze.rs`, not `wasm.rs`). Freeze
impact: **None** — additive exports, no wire change, no `allow-frozen-change`
needed.

### Guardrail: this stays a reference client, not "the" SPORE app

MISSION.md is explicit: *"Chat UI is one client, not the product definition."*
The plan's Mail/Rooms/Feed/Files/Folder IA is a good **reference**
implementation — proof the façade pattern works end-to-end in a browser — but
it must not become the implied canonical SPORE experience the way a
company-app chat UI would. Concretely: no feature here should require the
standalone HTML specifically (a bridge, a Python script, or the daemon CLI
must remain equally capable), and copy should read "a SPORE node in your
browser," not "the SPORE app."

### Feature scope (from the source plan, unchanged)

| Capability | Mechanism | Honesty note |
|---|---|---|
| Encrypted DM | `send_direct`/`open_dm` (ratchet, already shipped) | New wasm exports only |
| Open group | Unencrypted topic | Name + join UX |
| Encrypted group | Sealed topic / shared key | **No roster** — UI must say "anyone with the key can write," never "members" |
| Public shout | Public/flood topic | — |
| Feed / personal blog | Topic posts signed by addr, chronological | Follow = subscribe, not a server relationship |
| Authorized-only feed | Sealed topic key via invite/QR, **not** an allowlist server | Label as a **capability key** |
| Files | Manifests/magnets via existing `node/files.rs` | Library UI only, no new wire |
| Public folder (`spore://<addr>/path`) | App-level path-index convention (JSON, published as a magnet/topic) | World-readable like a public topic — never call it a private homepage; sandbox any rendered foreign HTML (XSS) |

### Phased delivery

W0 audit is done (above). Remaining, one PR per phase, same density as the
B-series:

- **W1** — Encrypted DM: `wasm.rs` exports for `send_direct`/`open_dm`, thread
  list, compose, delivery honesty (no read receipts).
- **W2** — Topics: open group join/create + public shout, clearly labeled public.
- **W3** — Sealed group: shared-key/invite-blob room, "anyone with the key can
  post" banner.
- **W4** — Feed/microblog: compose to `feed::<addr>`, follow = subscribe,
  optional link-out to long-form in the public folder.
- **W5** — Files: publish → magnet, fetch by magnet, progress UI, local search.
- **W6** — Public folder + `spore://` resolver: index publish/refresh, MIME
  present-or-download, sandboxed foreign HTML, `spore://` scheme registration
  where the browser allows it.
- **W7** — Authorized feed polish: invite flow, documented revoke-by-rotation
  limitation.
- **W8** — Continuity polish: export seed from the new UI, `web/README.md` +
  `docs/APPS.md` updates, mobile-browser limits documented.

### Non-goals (unchanged from the source plan)

iOS; a real group roster/admin-kick (still `P-Group-Roster`, not this track);
a global filename DHT; transparent clearnet browsing; describing sealed groups
as Signal groups.

### Acceptance (overall)

- [ ] Two browsers on a LAN: DM, open room, sealed room (with key), shout,
      feed post, publish a file, fetch a magnet — all through the standalone
      HTML, no daemon involved.
- [ ] Publish `about.html` to a public folder; a second node resolves
      `spore://<addr>/about.html` and renders it by MIME.
- [ ] Every `wasm.rs` addition has a matching `web/test.mjs` case, same bar as
      the existing node-ABI tests.
- [ ] UI never claims path/delivery/membership/anonymity the core doesn't
      provide (same rule as every other surface in this roadmap).
- [ ] Still a full node — store, bridges, same envelopes — not a view-only
      client wrapping a relay.

### Priority

Not yet placed on the "Priority compass" above — that list is locked human
direction from the 2026-08-01 audit tour and this track postdates it. Proposed
slot: after **P-Direct-NAT** (which several of these transports — WebRTC,
iroh-in-browser later — benefit from) and independent of the anonymity/iOS
items, since it touches neither. Flag for the next priority-compass review
rather than self-ranking here.

---

## Product decisions (2026-08-01 audit tour) — locked, not proposals

Explicit human direction, recorded here per this doc's own rule that status
lives in exactly one place. Full source reasoning: the audit-tour handoff and
its addendum (not committed to the repo — these are the durable conclusions).

### Non-goals (locked)

| Item | Decision |
|---|---|
| **iOS** | Not a target, ever, for this project's roadmap. Android, desktop, browser, and ESP/daemon only. State this on `docs/APPS.md` / the Uses material so the expectation dies early rather than getting asked repeatedly. |
| **Instant delivery with no path** | Impossible under a store-and-forward substrate by definition. The UI must say "no path yet" / fail closed for anything live-media-shaped, never spin as if it's an online chat app. An async fallback (e.g. a voice-note file) is fine; a live call promise is not. |
| **Group membership consensus protocol** | Already out of scope above — restated here because it's the crux of §"Family / group" below: a *shared-key sealed topic* is real and shippable now; a Signal-style roster (signed member list, add/remove epochs, sender binding, split-brain-proof transcripts) is a deliberate future protocol project, not a UI feature to fake. |

### Anonymity — an explicit, non-default toggle

Not implicit, not silent, not claimed to be Tor:

| Mode | Behaviour |
|---|---|
| Normal (default) | Seal/ratchet content as today; Direct allowed; underlay metadata as today |
| Mix-preferred | Prefer `mix` onions when mixes are known; warn if none are; discourage Direct for that send (a Direct locator leaks to the peer by design) |
| Mix-only | Refuse to send unless an onion path is available |

The primitives exist (`src/mix.rs`: onion wrap/peel, size-class padding,
`Batch` min-size + delay) and are already opt-in per-send, not a blanket
"anonymous mode." Defeating a *global passive adversary* the way Tor aims to
needs cover traffic and multiple operational mixes — SPORE's mix layer makes
who-talked-to-whom harder for *some* observers, and every surface that
mentions it must say that plainly, never "like Tor" or "anonymous." Tracked as
**P-Mix-Runner** (an example mix operator, so the anonymity path is actually
operable by someone besides the person who wrote it) + the app-level toggle
above. Clearnet exit (a peer relaying your traffic to the open internet,
tracked separately) is not part of this and stays off by default — it is a
convenience feature, not an anonymity one, and must never be described as such.

### Direct NAT traversal — staged plan (revised after deep review, 2026-08-01)

`docs/DIRECT.md` and the carried-forward notes elsewhere already say Direct
has no ICE/STUN/TURN/punch state machine yet — connecting assumes a usable
locator exists after ANSWER. A deep review before starting implementation
(reading `docs/DIRECT.md`, `Node::send_direct`, the `bridge-iroh` module, and
`docs/BRIDGES.md`'s WebRTC writeup) turned up findings that reshape this plan
from the version first sketched; the revisions below are agreed, not
open — the plan built on the research is the one to implement, no need to
re-litigate the "should we" questions this section used to hedge on.

**Finding 1 — Direct isn't reachable from the app yet.** `docs/DIRECT.md`'s own
Status section: the SPDR codec, key schedule, and UDP/TCP socket adapters are
built and unit-tested, but the mesh signalling glue that actually calls
`Pipe::offer`/`send_direct` from the app — previously tracked separately as
**PR8c** — was never merged. Nobody hits NAT-traversal pain today because
nobody can start a Direct pipe from the app at all. **Agreed: PR8c is now
Phase 0 of this track, not a separately carried-forward item** — wiring
OFFER/ANSWER into the daemon and Android app comes first, everything below is
built and field-tested against a Direct that actually exists in the app.

**Finding 2 — signaling latency and NAT-binding lifetime are in tension.**
`Node::send_direct` is an ordinary sealed unicast DM through the normal
mesh path-learning/flood mechanism — no fast path — and `docs/DIRECT.md`'s own
table says mesh delivery is "seconds to days." A NAT UDP binding typically
dies in 30s–5min of inactivity. So a coordinated punch **must not** be
attempted blindly off of however long OFFER→ANSWER happened to take over the
general mesh — by the time an ANSWER arrives after a slow mesh hop, the
candidates in it are presumptively stale. **Design consequence:** the punch
attempt (step 3 below) triggers off freshly re-exchanged locators once both
peers are demonstrably live and closely-timed (e.g. a fast round trip was just
observed, or both are already reachable over some existing low-latency
bridge), not off the original OFFER/ANSWER timestamps.

**Finding 3 — iroh already does hole punching and relay fallback.**
`src/bridge/iroh.rs`'s own doc comment: iroh "gives QUIC connections between
endpoints identified by a public key, with hole punching and relay fallback
when a direct path can't be found" — already an integrated dependency (PR9,
merged). That is most of what step 5 below set out to build from scratch.

**Finding 4 — this project already declined to hand-roll this class of thing
once.** `docs/BRIDGES.md`'s WebRTC section: a native Rust ICE/DTLS/SCTP stack
was "the largest dependency this project would have taken... spending it here
was considered and declined," with Tor/I2P (and now iroh) recorded as the
answer for "NAT traversal between two daemons with no reachable address." A
minimal STUN *client* — one UDP binding request/response, no ICE state
machine — is meaningfully smaller than what was declined, so reflexive-locator
discovery stays in scope; a **new SPORE-native relay protocol** does not.

**Agreed revised plan**, signaling still entirely inside SPORE envelopes
(SPDR offer/answer on the mesh), media still on whatever underlay was
negotiated:

0. **PR8c — wire Direct into the app** (daemon + Android): `send_direct`
   actually carries OFFER/ANSWER, an adapter opens on ANSWER. Prerequisite for
   everything below; nothing here is real until this lands.
1. Candidates in SPDR — already shipped.
2. Reflexive locators via a minimal, dependency-free STUN client (binding
   request/response only) — new, small, in-budget per Finding 4.
3. Coordinated UDP hole-punch: simultaneous probes once both sides have
   freshly re-confirmed reachability close together in time (Finding 2) —
   never timed off a possibly-stale OFFER/ANSWER round trip.
4. Optional UPnP/NAT-PMP/PCP on desktop/daemon, where a device actually offers it.
5. **iroh as the relay/NAT-fallback candidate** (Finding 3) instead of a new
   SPORE-native relay protocol — offered as a real, visible candidate, never a
   silent fallback the user didn't choose, and only where `bridge-iroh` is
   compiled in. iroh already supports a self-hosted relay, not only n0's
   public one, so "run your own relay" stays available without SPORE
   reinventing it; using the default public one is a real third-party-trust
   disclosure (`bridge-iroh`'s own doc comment already notes a relay sees
   ciphertext + metadata + timing) that belongs in `docs/DIRECT.md`'s threat
   model once this ships.
6. Preference order when several candidates work: LAN → reflexive punch →
   iroh (relay/fallback) → fail honestly (say there's no path, per the
   non-goals above).

**Should a daemon help other people's NAT traversal by default?** Only the
STUN-shaped echo is SPORE's own to decide — relay is now iroh's concern
(its own relay operators, or a self-hosted one, choose that separately):

- **Reflexive-locator echo (step 2) — default on.** Stateless, one packet in
  and one out, carries no payload, costs nothing to keep running — and it
  keeps SPORE from quietly depending on a third party's STUN server for
  something the protocol can trivially do for itself.
- **Relay** is no longer a SPORE-native opt-in question — see step 5. A
  daemon operator who wants to offer a relay does so by running (or pointing
  at) an iroh relay, a decision that already lives in iroh's own config, not
  a new `spore.example.yaml` flag.

Engineering pattern (unchanged): one shared helper (something like
`spore_direct_nat`) used identically by the daemon, desktop, and Android —
façades only ever call the Direct API, never reimplement punch logic per
platform. Tracked as **P-Direct-NAT**, now inclusive of PR8c per Finding 1.

**Hard limit to document, not paper over:** CGNAT-to-CGNAT with no working
relay candidate still fails sometimes. A relay is the permanent escape hatch
here, not a cleverer punch algorithm — don't claim arbitrary NAT traversal
once this ships; claim exactly what steps 0–6 cover.

### Priority compass

In order, per explicit human direction — later roadmap grooming should read
top-to-bottom here before picking up new discretionary work:

1. **P-Direct-NAT** — ends the single most repeated pain (Direct usually not
   actually connecting on a real WAN).
2. **Family** — sealed-topic + honest UX is buildable now; full roster only if
   the product genuinely demands it (see non-goals above).
3. **Anonymity toggle** — mix-preferred + a runnable **P-Mix-Runner** example;
   do not block this on being Tor-complete, which is explicitly not the goal.
4. **iOS** — non-goal, documented, not revisited absent new direction.

### North star — what these tracks add up to

One line: **personal offline infrastructure.** Devices you control carry
signed messages, files, and — when a path exists — live media, through apps
people already use, with honest limits: no iOS, no pretend anonymity, no
pretend "online."

What that means once PR6, P-Direct-NAT, P-Mix-Runner, and the family/roster
line above are actually done, not just planned:

- **A credible multi-path network.** Phones (Android), browsers, desktops,
  daemons, and small ESP/home nodes speak the same postcards over Wi-Fi, USB,
  radio, relays — backed by PR6's device evidence, not only CI passing.
- **Two planes, taught clearly, not blurred.** Mesh is store-and-forward:
  works hours or days later, sneakernet-native. Direct is the multi-transport
  low-latency pipe: signaling rides in ordinary envelopes, and P-Direct-NAT is
  what stops a WAN call or stream from being a permanent special case.
- **Familiar doors, not a walled garden.** One node on a device or home
  server, reached through a browser, `spore://`, a mail client pointed at
  `@spore.local`, the OS share sheet, plain folders and magnets, and
  optionally a SIP softphone or XMPP client — not a requirement to live inside
  one custom chat UI.
- **Family that matches what the protocol actually offers.** A household
  board and a shared sealed channel, now, with honest copy: holding the key
  means being able to write, not "verified membership." Not a fake Signal
  group until P-Group-Roster (or an app-level equivalent) does the real
  membership/roster work.
- **Anonymity as an opt-in, not a default story.** Confidentiality is the
  default and it's strong. The mix-preferred/mix-only toggle is there when
  wanted — never presented as "anonymous internet," never confused with Tor
  or with the separate, off-by-default clearnet exit.
- **Live vs. async, told honestly.** No path means no instant delivery — the
  UI says so and offers an async fallback (a voice note, not a spinner
  pretending to dial). Users learn the real model instead of a chat illusion
  that occasionally breaks.
- **A findable project.** SPORE stays the protocol name; a public
  subtitle/name (still open — see `docs/ROADMAP.md`'s open product choices)
  keeps search results from being only the EA game, and the Uses material
  explains the patterns above in plain language.
- **What this deliberately does not carry:** iOS, instant messaging with zero
  connectivity, Tor-level anonymity as the default story, or magic groups with
  no membership cryptography behind them. Each of these is a *choice*,
  recorded above, not a gap nobody noticed.

A normal day, once this is real: someone installs the Android app or opens the
standalone HTML, pairs with a QR, and leaves a home node or ESP device up.
Notes and photos move sealed or as magnets; a mail client works against
`@spore.local`; a call rides Direct when NAT and an available relay allow it;
when the mesh is genuinely slow, the UI says so instead of spinning. No
account, no vendor inbox, no iOS-roadmap guilt.

| Today | With these tracks done |
|---|---|
| Strong core, thin proof | A believable daily-driver path |
| Chat-shaped entry point | A platform with many familiar clients |
| Direct mostly LAN-shaped | WAN-capable Direct (punch + explicit relay) |
| Anonymity easy to over-claim | An explicit toggle with an honest ceiling |
| Group-feature pressure | A clear now (sealed topic) vs. later (roster) line |

---

## Plan health check (review)

### Structure
| Check | Status |
|-------|--------|
| PR0–PR10 each have Why / files or deliverables / acceptance / CHANGELOG / branch | OK (PR7 is a batch table by design) |
| Dependency edges (PR3→PR2, PR0 includes lifetime knobs) | OK |
| Freeze surface called out | OK — no PR requires `allow-frozen-change` |
| Parallel tracks labeled | OK — Direct, Iroh, FS/DTN honesty |
| Branch list matches PR map | OK after cleanup |

### Coverage vs audit P0–P2
| Audit theme | Plan |
|-------------|------|
| Ratchet TTL + zeroize | PR0 |
| Device matrix / backup / 7d field verify | PR6 (PR0 copy) |
| Hardware 🧪 honesty | PR6 |
| Bridge stop/unregister | PR2 |
| Attachments / FileProvider | PR1 |
| Lifecycle BLE/audio/service | PR3 |
| Store spill verify | PR5 |
| FS vs long offline honesty | PR0 (Part B) |
| Release pipeline dry-run | Explicitly out of scope |
| Group roster consensus | Out of scope (key_id badge in PR7 only) |

### Intentionally small / deferred (not missing blockers)
| Item | Where it lives |
|------|----------------|
| S-024b `mark_seen` vs ingest | Optional one-liner under PR0 or PR7 — dead code align |
| Ring health + export FS warning | PR7 |
| Group `key_id` divergence badge | PR7 |
| Boot receiver, sound, Baud | PR7 |
| Web/wasm iroh or Direct | Follow-up; daemon/Android first |
| Android iroh UI | After PR10 desktop proves path |
| Codec2 / POTS / ESP32 | PR8b / out-of-repo app — not core milestone |
| `with_node` reentrancy guard | Still open; low — optional polish |
| Beacon duty-cycle measurement | PR6 / HARDWARE.md |
| CI release dry-run fixtures | Out of scope (note only) |

### Renumbering note
Former standalone “offline lifetime knobs” PR is **Part B of PR0**. Iroh is **PR9** (was PR10).

### No obvious *blocker* milestone missing
Ship order PR0 → PR1 → PR2 still matches “credible phone node.”  
PR8/PR9 (iroh) are growth tracks.  

**Optional future PR11** (only if you want it tracked): `chore: mark_seen align + SECURITY_FINDINGS Still-open pass` after PR0 — process hygiene, not user-facing.

### Risks to watch
1. **PR0 policy object** — one defaulted policy for prekey + skip TTL used by crypto and UI.
2. **Iroh MSRV** — may force feature-gate + newer toolchain; keep default CI on 1.75.
3. **PR1 marker format** — document as app convention in UX-ISSUES so Feed/chat stay consistent.
4. **PR2 iface holes** — never renumber; document for all bridges including iroh stop.

---

*Actionable plan derived from static audit of the 0.6.0 tree (2026-07-28), plus Direct-pipe, offline-lifetime knobs, and iroh bridge tracks. Update when PRs land or hardware results arrive.*
