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
| **C3** | Generate the design tokens from one source (kills C1's manual re-audit) | Medium — maintenance | ✅ done — `design/tokens.json` → `design/generate.py`, CI job "design tokens in sync" | C1 (did it by hand) | W-series |
| **Site** | Readability + less generic + more fun UI (à la gitingest.com) | Medium UX | ✅ story cards (#63); contrast pass (#64); copy-code buttons (#65) | — | C1 |
| **P-Runtime** | Storage (then scheduling) as declared runtime nutrients, not assumptions | Foundation | ✅ **shipped** — P-Runtime-1 storage backend (#87); P-Runtime-2 scheduling contract (#90). See "Runtime nutrients" below | — | unblocked W-series and any thin runtime |
| **P-Direct-NAT** | Direct NAT traversal: STUN reflexive locators + coordinated hole-punch + relay candidate | Feature / networking | ✅ **shipped** — the whole ladder: LAN, global IPv6, declared overlay, reflexive + punch (#114 fixed the ordering that kept the punch from ever landing), iroh as last resort. Staged plan under "Product decisions" below | PR8 | — |
| **P-Mix-Runner** | Example mix operator + app-level anonymity toggle (mix-preferred / mix-only) | Feature — anonymity path operable | ⬜ todo | `src/mix.rs` (have) | — |
| **P-Group-Roster** | Signal-style membership (signed roster, add/remove epochs, sender binding) | Feature / protocol — **not v1, do not fake in UI** | ⬜ future — sealed-topic + honest UX is the shippable answer today | — | — |
| **W-series** | Web node: browser as a daily-driver peer (Mail/Rooms/Feed/Files/Folder), not a transport demo | Feature / product | ⬜ todo — phased W0–W8, see "Web node as a full daily-driver peer" below | — | — |

**Shipped work keeps its row here and loses its spec.** A PR marked ✅ has its
implementation plan deleted once merged: the code is the truth, `CHANGELOG.md`
records what changed, and anything left over is in *Carried forward* below. Git
history has the original if you need it. This file is what is *planned*, not a
museum of what was.

**Minimum credible phone node:** PR0 + PR1 + PR2 + one device-matrix pass.

**Direct-pipe track (orthogonal):** PR8 can start anytime; does not block phone-node definition of done.

**Iroh track:** PR9 is a normal bridge (like tor/i2p/tcp): carry SPORE envelopes over iroh QUIC; 🧪 until exercised.

**Note:** Former “PR9 offline lifetime knobs” is **folded into PR0** so crypto default and user-facing policy ship together.

---

## Conformance gaps — SPEC rules with no implementation

Found by checking [`SPEC.md`](SPEC.md) against the crate rather than against the
other docs: rules this repo *specifies* and does not execute. Recorded rather than
deleted from SPEC, because the rule is right and the code is what is behind — the
opposite of the usual drift.

| SPEC | Rule | State |
|---|---|---|
| §4 | Paths `purge 7 d` | ✅ **shipped** (#113). `Paths::purge` on the sweep, `Paths::trim(MAX_PEERS)` as the backstop. It was the one peer-keyed map `enforce_bounds` did not trim, so every signed envelope from a new source added a row that outlived the node. |
| §2 | "stores clamp horizon to 30 d" | ✅ **shipped** (#113). One `min` in `Node::store_put`, the single choke point into the store, plus the matching clamp on the dedup retain so an id is never held longer than its bytes. |
| Page 2 | Native nodes run WebRTC **ice-lite** with static ufrag/pwd/fingerprint | ⬜ open. No native WebRTC bridge exists; WebRTC is browser-only (`web/transports/webrtc.mjs`), so the 90-byte descriptor story has no native half. Planned, not a defect — but the page reads as though it ships, and now says otherwise. |

**Why this section exists.** Two of the three were live defects — unbounded growth
and store pinning — that no test covered and no doc admitted, because every doc had
only ever been checked against the other docs. Check new normative text against the
crate, not against the page that inspired it.

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
| `android/UX-ISSUES.md` | Was **new**; the convention was later absorbed into [`VISUALDESIGN.md`](VISUALDESIGN.md) Appendix A and the file retired |

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
Was: create `android/UX-ISSUES.md`. Shipped instead as [`VISUALDESIGN.md`](VISUALDESIGN.md) Appendix A — same content, one fewer file to keep in sync.

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

# PR6 — Field verification + docs honesty

## Why
Still open: no field-verified 7-day FS; all radios 🧪; HARDWARE.md procedure only; Android backup exclusion untested on device.

## Deliverables

### 1. Device matrix (`android/TESTING.md`)

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
- [x] Checklist exists (`android/TESTING.md`, 7 rows + History)
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

## Files — as shipped

The layout this track proposed and what it actually became. Kept accurate rather
than as-proposed, because the proposal is now the thing people would go looking
for.

| Path | State |
|------|-------|
| `src/direct.rs` | ✅ offer/answer, BLAKE2b key schedule, record AEAD, `DatagramPort`, `Pipe`, `Signalling` — the trait lives here, not in a `port.rs` |
| `src/direct/udp.rs` | ✅ UDP adapter (1 datagram = 1 record) |
| `src/direct/tcp.rs` | ✅ TCP adapter — **`u32be len ‖ record`**, not the `u16be` this table originally proposed |
| `src/direct/{stun,punch,iroh}.rs` | ✅ the P-Direct-NAT ladder; not foreseen here |
| `src/direct/runner.rs` | ✅ the shared runner both native runtimes drive |
| ESP-NOW adapter | ⬜ not built; would be `src/direct/espnow.rs` behind a `cfg` |
| `docs/DIRECT.md` | ✅ profile, threat model, candidate table |
| `examples/direct_loopback.rs` | ✅ the two-peer smoke test (named `direct_udp.rs` in the proposal) |
| Signalling only | ✅ `Node::send_direct` with opaque `SPDR` payloads; the envelope never changed |

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

Shipped, so [`DIRECT.md`](DIRECT.md) is the living copy — check it, not this, before
implementing against the record.

```text
offset  size  field
0       1     ver = direct::VERSION (3 today)
1       1     type  (0=MEDIA 1=KEEPALIVE 2=CONTROL 3=DATA 4=STREAM …)
2       2     seq   u16 BE
4       4     pipe_id trunc
8       n     AEAD ciphertext + tag
```

- **UDP / ESP-NOW:** one packet = one record  
- **TCP / serial / BLE:** `u32be length ‖ record` (BLE may chunk further)

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

## Runtime nutrients: storage and scheduling (P-Runtime)

**Steering:** [`DESIGN.md`](DESIGN.md)'s "The spore and the soil" — the core asks
its host for four things (randomness, time, storage, scheduling) and supplies
everything else. Two of the four are already contracts rather than assumptions.
This track closes the remaining two, so "what does this runtime provide?" becomes
something the code answers instead of a convention each runtime re-derives.

**Why it leads the compass:** every platform track is downstream of it. The
W-series browser node stays memory-only until storage is a contract, a thin
ESP-class runtime cannot spill to flash, and the honest capability report a UI
reads ("this runtime has no disk") has nothing to read until a runtime can declare
what it supplies. Doing this first means those tracks build on one real seam
instead of each inventing its own.

### P-Runtime-1 — storage as a nutrient  ✅ shipped (#87)

**Current shape.** `Store` holds envelopes in memory up to `set_mem_budget` and
spills the coldest wires past it. The only spill backend that exists is a
filesystem directory, bound through `Node::set_spill_dir(&Path, now)`. A runtime
whose storage is not a filesystem — a browser with IndexedDB/OPFS, an MCU with
flash — has no way to offer it and silently runs memory-only. Note the precise
shape of the gap: the core does **not** assume a disk (a node with no spill dir
works fine, by design), it offers exactly *one* shape of storage.

**Change.** A backend trait behind the existing spill machinery — put / get /
remove / enumerate-on-adopt — with today's filesystem code as its first
implementation. `set_spill_dir` stays as the filesystem convenience constructor;
a sibling entry point accepts any backend.

**Freeze impact: None.** Verified against `tests/api_freeze.rs`: the frozen
surface pins `Node::set_store_budget`, and neither `Store` itself nor
`set_spill_dir` appears there. This is additive — no `allow-frozen-change`, and
the PR guard's refusal to modify `api_freeze.rs` is never triggered.

**Acceptance**
- [ ] `Store` spills through the trait; the filesystem backend passes every
      existing spill test unchanged, including
      `the_store_spills_to_disk_and_still_serves_what_it_spilled` and the
      corrupt / truncated-spill cases.
- [ ] Adopt-time verification still discards content whose id does not match, so
      a tampered spill directory cannot inject — the property `set_spill_dir`
      already guarantees must survive the refactor.
- [ ] A memory-backed test backend proves a second implementation is possible
      without touching `Store`.
- [ ] `set_spill_dir` behaviour is byte-identical; no existing caller changes.

### P-Runtime-2 — scheduling as a nutrient  ✅ shipped (#90)

Ticking sync, expiry and resend is driven by `bridge::hub` and `cli::run` by
convention rather than through an interface, so each new runtime re-derives what
"tick" means and how often. Smaller and less urgent than P-Runtime-1 — no feature is
blocked on it — but it is the last of the four that is a habit rather than a
contract. After P-Runtime-1, or folded in if the same PR makes it natural.

### Explicitly not in this track

Compile-time `max_core` gating (`C0…C8` as cargo features). Recorded as declined
rather than forgotten: the ratchet session map lives inline on `Node`, so
cfg-gating protocol layers out reaches into `lib.rs` instead of excluding files,
and no shipping runtime needs the binary-size saving yet. Revisit only if a real
MCU target proves it necessary. The cheaper version of the same idea — gating
*supplies* rather than protocol layers — is the existing "Feature-gate optional
bridges" item.

**Branch:** `feat/runtime-storage-backend`

---

## Web node as a full daily-driver peer (W-series)

**Steering:** `MISSION.md` pillar 3 (Façades) + pillar 4 (Nodes people can run —
"Android, desktop, browser/wasm, daemon, ESP/home router") — the browser is a
first-class node, not a demo toy. Source: an implementer plan (since retired,
along with the other pre-ROADMAP planning docs) reviewed against the current tree;
findings and scope below are from that review, not the plan verbatim.

In the vocabulary of [`DESIGN.md`](DESIGN.md)'s "The spore and the soil": the
browser is one **runtime** among several, and the communicator this track builds
is a **façade on that runtime**, not part of the core. A thin runtime in one
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

### UI across runtimes — locked decision (2026-08-01)

Explicit human direction, recorded here rather than rediscovered per platform.
**Two UI implementations, not three**, over three shared layers.

| Runtime | UI | Reaches the node via |
|---|---|---|
| Browser / standalone | the web UI | in-process (wasm) |
| Desktop | **the same** web UI | localhost HTTP to the daemon |
| Android | **Compose** (stays native) | JNI |

**Shared underneath all of them:** the op-set (one declared surface, generated
adapters — the `bindings/spec.json` → `generate.py` pattern), the communicator
domain logic (threads, rooms, feed, library live in the core, not in each UI's
state), and the design tokens (see C3 below).

**Desktop is `daemon + web UI`, and ships in two steps.** Step 1 needs no wrapper
at all: the daemon serves the UI on `127.0.0.1` and the user opens their own
browser — zero new dependencies, works the day the HTTP surface exists, and it is
already a named target ("cli daemon with browser-accessed UI"). Step 2, only if
app-feel is wanted, adds a **Wry** window pointed at that port.

**Wry, not Tauri** — the reason is architectural, not size. Tauri's headline
feature is its JS↔Rust IPC bridge, which is precisely what must not be used here:
the UI has to speak HTTP to localhost so the *same bundle* runs in a browser, in a
desktop window, and against the API from Android. Tauri would put a tempting
shortcut (`invoke()` the Rust command directly) next to the rule that forbids
forking the UI — paying a large tree for the one feature that breaks the
one-bundle guarantee. Wry is window + webview and nothing else; same maintainers,
so escalating later stays in-ecosystem.

Before adopting Wry, check what bit this repo once already: the tree is at ~404
crates and the offline bundle at ~91 MB, and **an optional feature still moves the
core MSRV** — that is exactly how `bridge-iroh` pushed the floor 1.75 → 1.85,
because Cargo resolves optional deps too. Resolve Wry against 1.85 *before* adding
it. Linux additionally needs WebKitGTK as a system dependency, which is the one
platform where step 1 is arguably the permanent answer rather than a stopgap. Gate
it exactly like `bridge-iroh`: off by default, off the default and MSRV CI
matrices, its own job.

**Android stays Compose.** `WebBridgeHost.kt` already shows where the line belongs
— it runs the repo's real web transport modules in a *headless* WebView, so
WebSocket/Nostr/WebTorrent are not reimplemented in Kotlin. That is sharing web
code where there is no UI surface. The UI surface is the opposite case: B1–B7
fixed BackHandler, scroll-to-latest, IME insets, notifications, transfers
overflow, permission recovery, and the accessibility/density pass — precisely what
a WebView reintroduces. Swapping ~5,300 lines of Compose for a WebView would
discard that work and reopen those bugs.

**Trap to avoid:** do not build the desktop app by wrapping
`spore-standalone.html` in a window. That is the *browser* runtime in a desktop
costume — no disk spill, no background life once the window closes, and none of
the native bridges (UDP, folder, serial, Tor). Desktop is daemon + UI.

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

Placed third on the "Priority compass" below at the 2026-08-01 second-pass
review — after **P-Runtime** (whose storage seam this track's browser node is the
first real consumer of) and **P-Direct-NAT**, and independent of the
anonymity/iOS items, since it touches neither.

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
   **Core seam landed** — `direct::Signalling` turns a delivered DM's plaintext
   into a `Signal` saying what to open, and `accept`/`Pipe::answer_with` split
   deciding from opening (`Pipe::answer` took the port before it chose, so a
   responder willing to use more than one medium could not use it). A whole
   negotiation is tested over the real `send_direct`/`on_rx` path. **Daemon wired** — `src/cli/direct.rs`, with `direct:`/`direct-to:` config
   keys; two real daemon processes negotiate over the mesh and bring a UDP pipe
   up. LAN only, and the daemon says so: reflexive discovery is this track's own
   step 2 and is not built. **Android wired** — the JNI layer drives the *same* `direct::UdpRunner`, so the
   daemon and the phone cannot drift into two negotiations of one protocol.
   Compile-checked only: no SDK here, so the symbols are verified symmetric and
   the crate builds, but no device has run it. **PR8c is complete**; Direct is
   startable from both native runtimes, LAN-scoped until this track's own step 2.
1. Candidates in SPDR — already shipped.
2. ~~Reflexive locators via a minimal, dependency-free STUN client (binding
   request/response only)~~ — **done.** `src/direct/stun.rs`: binding
   request/response, XOR-MAPPED-ADDRESS for v4 and v6, unknown attributes
   skipped rather than rejected so a full STUN server interoperates. No
   dependency, no ICE. The **echo** half ships with it and is what the
   default-on decision below asked for — a daemon's `stun:` port answers
   statelessly, so one SPORE node is a reflexive server for another and the
   network need not lean on a third party. A discovered locator is offered as a
   second candidate ranked *below* the LAN one. Verified with two daemons, one
   asking the other. **It does not make NAT work** — see step 3.
3. **Wire fix landed, wiring landed.** `Answer::Ok` now carries `from`, the
   responder's own locator, and the initiator dials *that* instead of a candidate
   from its own offer; `UdpRunner::open` punches on the socket the pipe then runs
   on. SPDR is `VERSION = 3`.

   **Ordering fix landed — the punch now lands.** Wiring it proved the rung and
   then failed it: the responder opened *inside* `on_plaintext`, and opening a
   punched candidate blocks for `punch::WINDOW`. The ANSWER could not leave until
   that window had closed, and the initiator does not start punching until the
   ANSWER arrives — so the two windows were disjoint **by construction** and no
   punch could land however long either side waited. Both ends timed out into a
   plain connect, which still carries records on a LAN, which is precisely why it
   survived a test that only checked that bytes moved.

   The responder now answers first and opens second. `Accepted::answer` builds the
   ANSWER with **no port** — the key schedule binds the pipe id, both addresses and
   the medium, and never the socket, so nothing forced them together — and
   `Answering::over` attaches the port afterwards via the new `UdpRunner::settle`,
   which a runtime calls the instant the ANSWER is on the mesh. `poll` settles too,
   so a runtime that forgets is late rather than broken.

   The two-runner test now runs both punches on separate threads, because two
   blocking windows on one thread are disjoint by definition — two processes get
   that for free and a test has to ask for it. Both ends report `Via::Punched`, and
   it finishes in **0.24s where it used to take 4s**: those four seconds were two
   punch windows timing out, in a test that passed.

   *(historical)* The punch itself is built and tested (`src/direct/punch.rs`):
   two sides probing at once meet in the middle, a punch nobody answers fails
   inside its bound, and the socket that punched is the one handed to the pipe —
   a mapping belongs to a source port, so punching on any other socket is
   worthless. It is **not yet wired**, because doing so surfaced a wire gap and a
   bug that have to be fixed first (below).

   **Bug found while wiring it — the ANSWER never says where the responder is.**
   `Answer::Ok` carries `{ pipe_id, eph_pub, chosen }`, and `chosen` is a
   candidate from the *initiator's* own OFFER. So on receiving an ANSWER the
   initiator calls `open(&chosen, ..)` and connects its socket **to its own
   advertised address**. The responder dials the initiator from an ephemeral
   port, and the initiator's connected socket rejects it. Both ends still derive
   matching keys and report the pipe up, so it looks like it worked — records
   simply never arrive initiator-ward. The two-daemon verification in #95 missed
   it because it asserted key agreement and never that a record flowed.

   Fixing it means the ANSWER must carry the responder's own locator, which is an
   SPDR encoding change (`VERSION` 2 → 3), which is acceptable: the project is
   explicitly unstable before 1.0, so SPDR may be re-cut without a migration
   story. That freedom stops at 1.0, and it never extended to the v1 *envelope*
   wire, which is frozen independently of any release number.
   The punch then supplies the other half, since it learns the peer's *actual*
   source address from the probe that arrives rather than trusting a predicted
   one — which is also what makes it work through a symmetric NAT that rewrote
   the port.

4. Coordinated UDP hole-punch: simultaneous probes once both sides have
   freshly re-confirmed reachability close together in time (Finding 2) —
   never timed off a possibly-stale OFFER/ANSWER round trip.
4. Optional UPnP/NAT-PMP/PCP on desktop/daemon, where a device actually offers it.
5. **Core landed.** `direct::iroh::IrohPort` wraps an established iroh connection
   as a `DatagramPort`, and `UdpRunner` offers `iroh` as a candidate and dials it —
   but **only when a runtime supplies an endpoint** via `set_iroh`. Without one the
   medium is absent from both the offer and the willing set, so a peer offering it
   is declined with a reason rather than accepted and found unopenable; there is a
   test for exactly that. Ranked worst of the routable candidates, because it may
   punch but may also relay.

   To make this possible `Pipe`'s port is now boxed (`direct::AnyPort`): a UDP pipe
   and an iroh pipe are different types and could not share one map. That is the
   same bargain as `Box<dyn SpillBackend>`, and it is what medium-by-convention
   already implied — an open medium list cannot be held by a closed type.

   **Daemon wired.** `direct-iroh: direct-only | n0` stands up an endpoint and
   offers the medium. The relay posture is **never defaulted** — an unrecognised
   value is a config error, because inheriting a third party is exactly the kind
   of silent choice the honesty contract forbids. `direct-only` runs with no relay
   and no discovery; `n0` opts into n0's public relay and the daemon prints, at the
   moment it takes effect, that this is a third party which sees ciphertext, volume
   and timing when a path is relayed. iroh supports self-hosted relays, so needing
   a relay never has to mean needing n0's.

   A build without `bridge-iroh` says `direct-iroh:` was ignored rather than
   silently doing nothing.

   *(original plan)* **iroh as the relay/NAT-fallback candidate** (Finding 3) instead of a new
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

**Settled 2026-08-01 (fourth pass): iroh is the NAT answer; the hand-rolled punch
is demoted to an optimisation.** Steps 3–4 stop being load-bearing and step 5
moves ahead of them.

This is not a reversal — it is Findings 3 and 4 being taken at their word. iroh
already does hole punching and relay fallback, is already a merged dependency, and
this project already declined to hand-roll ICE because it was the largest
dependency it would ever take. Two integration bugs in two consecutive PRs on the
*easy* part of a hand-rolled punch (#101, #103) are evidence for that original
judgement, not against it.

Scope, stated narrowly on purpose. iroh is reached only when **both peers are on
the public internet, both are behind NATs, and no LAN or overlay path exists.**
That is the tail, not the trunk, of what SPORE is for:

- **LAN** — both ends already reachable; nothing to traverse.
- **ESP-NOW / BLE** — no IP, so no NAT concept; a link-layer locator.
- **Overlays** (Reticulum, Yggdrasil, cjdns, WireGuard, Tor, I2P) — SPEC page 2
  already treats these as "underlays with their own routing = one interface". An
  overlay hands both peers a routable address, so a Direct candidate over one is
  an ordinary `udp` candidate with the overlay's address as its locator. **An
  overlay does not sit on the ladder; it removes the need for it.**

Because that scope is narrow, leaning on a dependency there is cheap, and the
third-party-trust disclosure stays bounded: iroh supports a **self-hosted relay**,
so "a relay is needed" never means "n0's relay is needed". Native-only, so a
browser's ladder still ends before it (per the runtime-membership decision below).

**Answered, and half-done: nothing emitted anything but IPv4.** `UdpRunner::candidates()`
was the only producer and offered exactly two locators, both `udp` over v4 — the
configured LAN address and the reflexive one.

**A global IPv6 is now offered, and it is the WAN path that needs nothing built.**
Most ISPs hand out global v6, and a global v6 address has **no NAT in front of
it** — unlike the reflexive locator it is already the address a peer dials, with
no discovery, no punch and no relay. It is ranked between LAN and reflexive, so
`choose` prefers it over a path that does not exist yet. A path firewall may still
drop unsolicited inbound, but that is a pinhole rather than a mapping that must be
discovered first: better odds, not a promise, and the daemon prints exactly which
locators it is offering so "why did it not connect" is answerable without a packet
capture.

**Overlay addresses: done for the ones it can be done for, and the split is the
finding.** `direct-also:` declares extra locators, offered as ordinary candidates
ranked between IPv6 and reflexive — an overlay already routes, so it needs no
punch, but it is several hops of someone else's network so it should not outrank a
direct v6 path.

Declared rather than discovered, deliberately: a routing probe follows the default
route so it never picks an overlay's source address, and cjdns sits in `fc00::/8`
which a public-internet check rightly rejects. Auto-discovery here would be
guessing.

**Only IP-layer overlays are covered — Yggdrasil, cjdns, WireGuard, a VPN.** They
hand out real, UDP-dialable addresses. **Tor and I2P are not, and cannot be**: a
`.onion` or `.b32` is a stream rendezvous name with no UDP beneath it, so reaching
Direct over them needs its own medium and adapter, not a locator. That is a
separate piece of work and is not scoped here — but after the medium-by-convention
change it is additive, with no core edit and no allocation from anyone.

**Prerequisite: make the fallback loud — done.** `Event::PipeUp` now carries
`Via::Punched` or `Via::FellBack`, surfaced in the daemon log and in Android's
status line. Two daemons on one host today print *"no punch, plain connect"* on
both ends, so the disjoint-window bug is a runtime observable rather than a note
in this file. `Via::FellBack` is not automatically a failure — for a candidate
that already routes (LAN, IPv6, overlay) there was nothing to punch — but on a
reflexive candidate it means there is no path, however healthy the pipe looks.

The carriage test now asserts `Via::FellBack` explicitly, pinning today's
behaviour rather than the intent: when the ordering is fixed that assertion fails
loudly and in the right place, which is the point of writing it that way.

**Design constraints for `IrohPort`, so they are not rediscovered the hard way.**
The iroh connection *is* the medium — its punched path cannot be extracted and
reused, for the same reason a punch must happen on the pipe's own socket, so it is
wrapped as a `DatagramPort` and handed to `Pipe` like any other:

- **QUIC datagrams, not streams.** [`DIRECT.md`](DIRECT.md) avoids ordered
  delivery even on TCP so a lost media frame never head-of-line-blocks; running
  records over a QUIC stream reintroduces exactly that. iroh's max datagram size
  becomes `DatagramPort::mtu()`.
- **Keep the record AEAD — the double encryption is the point.** The SPDR key
  schedule binds both addresses, the pipe id *and the medium name*. Dropping our
  own sealing would make a pipe's security depend on its medium, and the threat
  model says every link is hostile.
- **The iroh NodeId is a second identity; the signalling authenticates it.** Do
  not reuse the SPORE signing key as iroh's static key — cross-protocol reuse.
  Nothing is needed: the candidate rides inside a sealed, signed OFFER, so the
  NodeId is attested by the SPORE identity exactly as an `ip:port` already is. No
  new trust root and no new pairing step.
- **A relayed iroh path is not "Direct" as this repo defines it.** `DIRECT.md`'s
  two-planes table says one hop, straight over an underlay; a relay is multi-hop
  and sees ciphertext, metadata and timing. iroh reports which it got, so surface
  it — `Event::PipeUp { via: Direct | Relay(..) }`, in the daemon log and the
  Android status. That is the "visible candidate, never a silent fallback"
  requirement, and here the signal comes free instead of having to be invented.

**Verification rule adopted with this decision:** a change claiming a network
capability must carry a test that fails when that capability is absent. "Pipe up"
could not fail. "A record arrives" could not fail either, because the fallback
satisfied it. "A record arrives *and* the punch landed" would have caught both.
If the native punch is ever revived, it needs a real NAT to test against — two
Linux network namespaces with an `nftables` masquerade, which runs on
`ubuntu-latest` — because without one, steps 3, 4 and 6 land blind.

**Runtime-dependent by construction** (added at the 2026-08-01 second-pass review,
in the [`DESIGN.md`](DESIGN.md) runtime vocabulary). Steps 2–5 are *supplies*, and
not every runtime has them: iroh (step 5) is a native-only bridge, so a browser
runtime's ladder ends at step 3 and it reaches a relay only through a peer that
has one; UPnP/NAT-PMP (step 4) is desktop/daemon-only, as already noted. Each
runtime therefore walks only the subset it can actually supply, and says so rather
than offering a candidate it has no way to try.

**Settled 2026-08-01 (third pass): the order is one shared table, and membership
in it is a per-runtime capability query.** The question was posed as a choice
between those two; the answer is that they are different layers, and keeping them
apart is what makes the ladder honest without making the core aware of hosts.

- **The order is global.** LAN → reflexive punch → UPnP-assisted → iroh → fail
  honestly is a property of the protocol, not of any host. A per-runtime *order*
  would let two runtimes disagree about what "best path" means — and a Direct pipe
  has two ends, so they would be negotiating against different tables.
- **Membership is queried.** A static per-runtime table would require the core to
  know which runtimes exist, which is exactly what "the core holds no OS" forbids.
  The runtime declares which rungs it can supply; the core walks the shared order
  and skips the rest.

**The handshake — including the capability exchange — is core; only the contents
are the runtime's.** This is the load-bearing reason, and it is not a new idea: it
is how Direct already works. `direct::choose(offer, willing)` (`src/direct.rs`) is
a capability handshake sitting in the core today — two peers agree on a medium, and
the runtime's whole role is to pass in `willing`, the set it can actually serve.
The core never touches the socket or the bytes; `DatagramPort` is the runtime's.
[`DIRECT.md`](DIRECT.md) states the property directly: everything in
`src/direct.rs` is pure — no sockets, no `Node` — so it compiles wherever the core
does, wasm included.

The ladder is that same negotiation with more rungs, so it belongs in the same
place: a pure function in `src/direct.rs` alongside `choose`, taking the local
capability set as a parameter. It is specifically **not** a filter each façade
applies before calling the core. A capability set is an input to an agreement
between two peers, so a façade that pre-filtered would be settling half of a
negotiation on its own, and the two ends could settle it differently.

Two smaller supports point the same way. `SpillBackend` is the same bargain for
storage — the runtime supplies an implementation or it does not, and the core
degrades honestly instead of pretending. And `MISSION.md`'s honesty clause ("a
runtime declares what it cannot do … never a control with nothing behind it") is
what makes the declaration user-visible rather than merely internal: a browser
runtime says it has no relay of its own, instead of showing a candidate that can
never connect.

**Adding a rung no longer breaks older peers — resolved.** This was recorded as a
trap: `direct::Medium` was a `#[repr(u8)]` wire enum and `Offer::decode` parsed
candidates with `Medium::from_u8(r.u8()?)?`, so the `?` propagated and an unknown
medium failed *the entire OFFER* — an older peer dropped the whole offer including
the candidates it could have used. A medium is now a length-prefixed **name**, so
an unrecognised one decodes cleanly and is simply a candidate nobody is willing to
open. Step 5's relay candidate can therefore be added without stranding anything.
The SPDR profile went to `VERSION = 2` for the encoding change, which was cheap to
do while no deployed build could start a pipe at all.

**This does not add a nutrient.** Transport is not one — it is the boundary the
four are stated across — and a punch rung is a bridge-side way of reaching that
boundary, exactly as a bridge is (see [`DESIGN.md`](DESIGN.md)'s legend). What is
new is only that the reachable set becomes something a runtime states, rather than
something each façade infers.

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

Engineering pattern (**revised at the third pass**; it previously read "one shared
helper — something like `spore_direct_nat` — used identically by the daemon,
desktop, and Android"): the shared thing is the **core**, not a helper standing
beside the runtimes. The walk lives in `src/direct.rs` next to `choose`, and each
runtime passes in what it can supply. The original intent is unchanged and now has
a stronger home than a helper crate — façades only ever call the Direct API, and
never reimplement punch logic per platform. Tracked as **P-Direct-NAT**, now
inclusive of PR8c per Finding 1.

**Hard limit to document, not paper over:** CGNAT-to-CGNAT with no working
relay candidate still fails sometimes. A relay is the permanent escape hatch
here, not a cleverer punch algorithm — don't claim arbitrary NAT traversal
once this ships; claim exactly what steps 0–6 cover.

### Priority compass

In order, per explicit human direction — later roadmap grooming should read
top-to-bottom here before picking up new discretionary work. **Revised
2026-08-01 (second pass)**, on explicit direction, to lead with the runtime work and
to place the W-series, which the first pass predated.

1. **P-Runtime** — storage, then scheduling, as declared nutrients. First because
   everything below is downstream of it: the browser node stays memory-only, a
   thin runtime cannot spill to flash, and no runtime can honestly declare what it
   lacks until it can declare what it supplies.
2. **P-Direct-NAT** — ends the single most repeated pain (Direct usually not
   actually connecting on a real WAN).
3. **W-series** — the browser as a daily-driver peer; the first runtime to consume
   P-Runtime's storage seam and the communicator-as-façade pattern.
4. **Family** — sealed-topic + honest UX is buildable now; full roster only if
   the product genuinely demands it (see non-goals above).
5. **Anonymity toggle** — mix-preferred + a runnable **P-Mix-Runner** example;
   do not block this on being Tor-complete, which is explicitly not the goal.
6. **iOS** — non-goal, documented, not revisited absent new direction.

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

## D1 — Editorial and architectural review of the documentation — ✅ shipped (#111, #112)

**Why.** The docs have grown by accretion — this session alone touched ROADMAP,
DESIGN, DIRECT, CHANGELOG, DEV_GUIDE, MISSION and VISUALDESIGN — and nobody has
audited them as a whole. Suspected symptoms, unverified: the same concepts
explained in several places (transport independence, the endpoint-only rule, the
runtime contract), onboarding duplicated across README/MISSION/DEV_GUIDE, and
prose that argues where a table would state.

**Change.** A full editorial pass over every markdown file, judged against "every
sentence should justify its existence": per-document purpose/audience/category and
whether it should exist at all; cross-document duplication with a
define-once-reference-everywhere table; information architecture and read order;
tone (target: RFC, kernel docs, Rust, SQLite, Go proposals — not philosophy);
redundancy/verbosity/precision scores; and a prioritised delete/merge/rewrite plan
with target word counts.

**`BRIDGES.md` is the deliberate exception** — it earns its length as an
encyclopedia and stays comprehensive.

**Non-goals.** Adding documents. The bar for a new one is that it *reduces* future
maintenance; prefer one definition referenced everywhere over five explanations.

**Acceptance**
- [x] Every markdown file classified, with a keep/delete/merge recommendation.
      `RESILIENCE.md` was the one deletion; its unique table moved into CONTINUITY.
- [x] Duplication table: concept → where it appears → where it should live. The
      live outcome rather than a table: ROADMAP dropped 757 lines of specs for
      shipped PRs, "not a protocol change" is said once in DIRECT, and
      VISUALDESIGN says "this document is normative" once rather than twice.
- [x] Recommended structure and read order for a first-time contributor —
      `DEV_GUIDE.md` is now almost entirely lookup tables, starting with a
      by-goal index.
- [x] A ranked plan — the first ten commits, by impact. Executed rather than
      filed: eleven commits across #111.

**What the pass did not anticipate.** It was scoped as an *editorial* review —
duplication, tone, read order — and assumed the docs were mutually consistent and
merely verbose. They were not. Checking them against the crate instead of against
each other (#112) found ten claims the tree contradicts, including two record-format
values in `DIRECT.md` that would have produced an incompatible implementation, and
a `develop` branch that does not exist being the first instruction a contributor
reads. Two of the SPEC rules it surfaced were live defects, fixed in #113.

The lesson worth keeping: an editorial pass makes the docs *consistent*, which is
not the same as *true*, and consistency is the more comfortable of the two to
achieve. The **Conformance gaps** section at the top of this file is where the
difference gets recorded from now on.

**Note on doing it well:** this needs the whole docs tree read in one pass, so it
wants a session with the context budget to actually read them rather than sample
them. A partial pass would produce confident-sounding claims about files nobody
opened, which is worse than no review.

**Branch:** `docs/editorial-review` · **Freeze impact:** None.

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
3. **PR1 marker format** — document as app convention in [`VISUALDESIGN.md`](VISUALDESIGN.md) Appendix A so Feed/chat stay consistent.
4. **PR2 iface holes** — never renumber; document for all bridges including iroh stop.

---

*Actionable plan derived from static audit of the 0.6.0 tree (2026-07-28), plus Direct-pipe, offline-lifetime knobs, and iroh bridge tracks. Update when PRs land or hardware results arrive.*
