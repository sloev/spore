# SPORE roadmap — the single living plan

**Project:** `sloev/spore` · **Version:** 0.6.0 (`Cargo.toml`) · **Wire:** v1, frozen.

This is the one forward-looking plan. "What shipped" lives in exactly two places —
[`../CHANGELOG.md`](../CHANGELOG.md) `## Unreleased` and the **Status** column below —
so no third progress table can drift out of sync. Detailed bodies for the original
PR0–PR9 series are preserved in git history (see the note in the retired
`SPORE_DEEP_AUDIT.md` stub); this file carries the map, the principles, the still-open
work, and the new tracks.

## Principles

1. **One concern per PR** — reviewable, revertible, CI-green on its own.
2. **No fake UI** — never ship a control whose backend is missing.
3. **Honesty preserved** — 🧪 markers, "Still open", served-vs-fetching language,
   [`VISUALDESIGN.md`](VISUALDESIGN.md) is normative.
4. **Security is orthogonal to UX** — a P0 fix does not block a UX PR when independent.
5. **Distinguish** Verified (code/CHANGELOG) vs Reasoned vs Needs-device-run.
6. Micro-commits inside a PR are fine; the **merge unit is the PR**.

## PR map

| PR | Title | Urgency | Status |
|----|-------|---------|--------|
| PR0 | Ratchet TTL + zeroize **and** offline lifetime knobs | P0 security | 🟡 Part A merged (#40); **Part B (PR0b) carried** |
| PR1 | Chat stage/attach/preview/FileProvider | Critical UX | 🟡 merged (#41); polish carried |
| PR2 | Hub unregister + bridge stop/remove | High UX | 🟡 merged (#42); polish carried |
| PR3 | Service / Audio / BLE lifecycle | High reliability | ✅ merged (#44) |
| PR4 | Name others see + local avatar + mesh profile | Medium | ✅ 4a (#45), 4b (#46) |
| PR5 | `Store::wire` verifies spilled envelope id (C-ST4) | P0 security | ✅ merged |
| PR8 | SPORE Direct — protocol core + UDP/TCP adapters | Feature | ✅ merged (#50–#52) |
| PR9 | iroh QUIC bridge (`bridge-iroh`); MSRV → 1.85 | Feature | ✅ merged (#53) |
| **Docs-1** | Introduce this file; migrate plan; stub deep-audit | Process | 🟢 **in progress** |
| **Docs-2** | Retire `ANDROID_AUDIT.md`; fix Progress drift | Process | 🟢 in progress |
| **Docs-3** | Absorb `android/PLAN.md` + `UX-ISSUES.md` | Process | todo |
| **B1** | Chat nav: Back + scroll-to-latest + IME insets | Critical UX | todo |
| **B2** | Send/error feedback (no silent no-op) | Critical UX | todo |
| **B3** | Empty states + PUBLIC/broadcast confirm | High UX | todo |
| **B4** | Notifications + transfers overflow | High UX | todo |
| **B5** | Advanced: ring health + cautious export | Medium | todo |
| **B6** | Bridges: status enum + permission recovery | High UX | todo |
| **B7** | Accessibility + density pass | High UX | todo |
| **C1** | Token parity + forbidden-pair audit | High UX | todo |

## Shipped (see CHANGELOG for the wire-status line on each)

PR0 Part A (S-024a ratchet skip-key TTL + zeroize), PR1 (chat attachments, one bubble,
FileProvider), PR2 (`Hub::unregister` + bridge Remove), PR3 (service/audio/BLE
lifecycle), PR4a/b (announced name + local avatar + mesh profile pull), PR5 (spilled-id
re-verify), the lib.rs protocol-layer split, PR8 (Direct core + real UDP/TCP
`DatagramPort` adapters, two-process UDP round-trip), and PR9 (iroh QUIC bridge behind
`bridge-iroh`, MSRV raised 1.75→1.85, dedicated `iroh` CI job).

## Carried forward — still open

- **PR0b — offline crypto lifetime knobs.** Configurable `prekey_lifetime_secs` /
  `ratchet_skip_ttl_secs` + Android presets + "raise above default" theft warning +
  decrypt-failure UI. **Blocked for an honest reason:** Double-Ratchet decrypt is not
  yet on a production DM send/receive path (test-only), so lifetime sliders would
  configure dead code — a no-fake-UI violation. Prekey-disclosure-only copy is OK if
  carefully worded. Revisit after the ratchet is integrated.
- **PR6 — on-device QA.** Run [`../android/TESTING.md`](../android/TESTING.md) on real
  hardware and append History rows. Every 🧪 Android item below depends on this.
- **PR7 — polish batch.** Reactions, ring-health UI, sound toggle (default **off**),
  optional boot receiver.
- **PR8c — Direct mesh glue.** Carry SPDR OFFER/ANSWER over `send_direct`; add
  `CLOSE`/`REKEY`. **PR8d — ports:** BLE / ESP-NOW `DatagramPort`; Android call UI on
  Direct.
- **PR9 follow-up.** Two-machine (direct + relay) runbook in `HARDWARE.md`; exercise the
  iroh bridge on a real NAT path (the localhost QUIC round-trip is the CI-testable
  slice).
- **S-024b** — `mark_seen` vs ingest 30-day floor mismatch (low chore). **Zeroize** pass
  beyond the ratchet (chore). **Group roster** — out of scope (a membership-consensus
  protocol change).

Findings still open are tracked in
[`SECURITY_FINDINGS.md`](SECURITY_FINDINGS.md) ("Still open"): field-verify the 7-day
forward-secrecy window; `with_node` reentrancy; beacon duty cycle on radio; and every
🧪 bridge until [`HARDWARE.md`](HARDWARE.md) records a run.

## Track: docs consolidation

Cull overlapping status/plan docs down to canonical roles. Target `docs/` footprint:
`SPEC`, `DESIGN`, `BRIDGES`, `SECURITY_FINDINGS`, `VISUALDESIGN`, `APPS`, `CONTINUITY`,
`HARDWARE`, `DIRECT`, **`ROADMAP`**. Retired: `SPORE_DEEP_AUDIT.md` (Docs-1) and
`ANDROID_AUDIT.md` (Docs-2 — its still-open items are in the Android sections above, its
device checks in `TESTING.md`). Still to do: absorb `android/PLAN.md` (→ README "shipped
milestones") and `android/UX-ISSUES.md` (→ `VISUALDESIGN.md` appendix) in Docs-3 —
**preserving the attachment-marker regex verbatim**:
`(?m)^📎 (.+) \| spore:([0-9a-fA-F]{16,}) \| (\S+)$`.

## Track: Android UX (all 🧪 until PR6 runs on a device)

**B1** hierarchical Back + pin-to-latest + `imePadding`. **B2** send helpers return a
result; snackbar on failure; clear composer only on success. **B3** plain-language empty
states + PUBLIC/broadcast confirm (no fake unread badges). **B4** dynamic FGS
notification (peers/relaying) + transfers `+N more`. **B5** ring-health readout + export
gated behind a forward-secrecy warning (single accessor, no second seed copy). **B6**
bridge status enum (up/connecting/down/error) + permission deep-link recovery — Remove
stays the control, **no fake on/off switch**. **B7** content descriptions, 48 dp
targets, jump-to-bottom FAB, reduced-motion honored.

### Android engineering — carried forward from the retired production audit

The Android production audit (formerly `ANDROID_AUDIT.md`, retired into this file) left
these still-open items, distinct from the B-track's visual UX. Device-run verification
of everything already shipped lives in
[`../android/TESTING.md`](../android/TESTING.md); the shipped fixes themselves are in
the CHANGELOG.

- **`FileProvider` + `ACTION_VIEW` for received files.** Open-via-chooser works for
  attachments the app has cached (PR1), but an *arbitrary received file* (e.g. a PDF a
  peer sent) still can't be opened. Feed images render inline — a different path. (This
  is the one Progress-table row that read "open" and stays honestly open.)
- **Reactions**, then audio/video previews — the latter needs an ExoPlayer dependency
  decision, not just UI (PR7).
- **JNI local-reference audit + soak.** `nativePollForward`/`nativePollDelivery` allocate
  a byte array per iteration; a loop *inside* a native call needs `DeleteLocalRef` or an
  explicit frame, or it aborts under sustained load rather than degrading. Reasoned, not
  measured — needs a soak run (PR6/§7). `DirectByteBuffer` for large payloads is a
  measure-first optimization, not a given.
- **WebView battery.** Prime suspect for warmth: each headless WebView is a full renderer
  that doesn't suspend under Doze. Per-process isolation is an *architecture* change (the
  node lives behind a `jlong` in-process; a separate process needs an IPC layer that
  doesn't exist), not a manifest attribute. Cheap wins: cap concurrent instances, pool +
  reuse, explicit `destroy()`, and a **Lite mode** that disables WebView bridges. Measure
  with Battery Historian (12 h, all bridges) before optimizing.
- **Foreground-loop lifecycle gating** (don't recompute `peers`/`storeLen` every 30 s
  while backgrounded) and **release `MulticastLock`** when Wi-Fi drops.
- **Permissions at point of use.** Request Bluetooth/audio/location the moment a bridge is
  enabled with one line of rationale, never at launch; `BLUETOOTH_SCAN` with
  `neverForLocation` on API 31+ to avoid the location grant; `RECORD_AUDIO` only when the
  audio modem starts. **Don't fight Doze** — SPORE is store-and-forward, missing a
  fifteen-minute window is normal; say so in the UI rather than burning battery. (Overlaps
  B6 permission recovery.)

## Track: visual / palette

**C1** token parity across the four surfaces (`VISUALDESIGN.md`, `site/style.css`, the
web-standalone tokens, Android `Chrome.kt` Palette) + a forbidden-pair audit: no
pink-on-kevlar (measured 2.32:1), `Send` = pink face + void ink, cyan 2 px focus,
disabled = kevlar + dim label, and never signal failure by colour alone (pink is both
accent and "bad"). Non-goals: rebrand, new hues, webfonts/CDN, Android light theme.

## Non-goals (repo-wide)

Wire / C-ABI / `reference/vectors.json` / `tests/api_freeze.rs` changes; group-membership
consensus; claiming a 🧪 radio production-ready without a `HARDWARE.md` History row;
lifetime-slider UI before the ratchet is on a real DM path; new long `*_AUDIT.md` files.
