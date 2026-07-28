# SPORE Deep Audit — Comprehensive Findings Register

**Project:** `sloev/spore`  
**Digest source:** GitHub Digest pack (83 files, ~138k tokens)  
**Audit date:** 2026-07-28  
**Audited version:** 0.6.0 (Cargo.toml) / master tip as of digest  
**Auditor:** Grok (xAI) — offline analysis of complete repository digest  
**Scope:** Core protocol, crypto, Android app, UX/UI, documentation, CI/CD, security findings, supply-chain, testing, continuity

This document is a living mega-audit. Findings are organised by subsystem. Severity uses the project’s own register style where possible (High / Medium / Low / Info / Process). New observations that are not already in `docs/SECURITY_FINDINGS.md` are marked **[NEW]**.

---

## 0. Methodology & Limitations

### What was examined
- Full repository structure and every file content present in the digest.
- Protocol specification (`docs/SPEC.md`).
- Visual design language and Android consumer (`docs/VISUALDESIGN.md`, `Chrome.kt`, screens).
- Security findings register (S-001 … S-031 + “Still open”).
- All GitHub Actions workflows (android, ci, fuzz, msrv, pages, pr-guard, reference, release, supply-chain).
- Android Kotlin sources (AudioBridge, BleBridges, ChatScreens, Chrome, MainActivity path, NodeController, NodeService, SporeNative, etc.).
- Fuzz targets, examples, Cargo.toml, deny.toml, CHANGELOG, CONTRIBUTING.
- Bindings generation story and freeze-guard surface.

### Limitations
- The digest truncates some very large files mid-content (noted in source). Core Rust `src/` modules beyond what is referenced in findings/examples are inferred from docs, findings, and API surface rather than line-by-line.
- No live execution, no hardware, no dynamic analysis of the APK or JNI.
- No access to `docs/BRIDGES.md`, `docs/HARDWARE.md`, `docs/ANDROID_AUDIT.md`, or the full `src/` tree beyond what the digest and findings expose.
- Conclusions about “still open” items are taken from the register itself and cross-checked against CHANGELOG and SPEC.

### Severity scale used here
| Level | Meaning |
|-------|---------|
| **Critical** | Remote crash / permanent data loss / permanent DoS / key compromise with no user interaction |
| **High** | Significant resource exhaustion, forward-secrecy failure, or identity exposure under realistic conditions |
| **Medium** | Correctness / honesty bugs, local DoS, UI lies, process failures that produce bad artefacts |
| **Low** | Documentation drift, polish, theoretical gaps, self-inflicted process issues already mitigated |
| **Info** | Observations, positive findings, architectural notes |

---

## 1. Project Character & Architecture

### 1.1 What SPORE is
SPORE v1 is a **store-and-forward Planetary Opportunistic Relay Envelope** protocol. Messages are signed postcards (to / from / expiry / payload / signature). Nodes keep postcards they have not seen, hand copies to anyone they meet who wants them, and drop duplicates and expired mail. Everything else (forward secrecy, fountain fragmentation, congestion control, mix anonymity) is layered on top without changing the relay core.

**Tiers (all interoperate):**
- **T0 carry** ≈ 60 lines: parse, dedup, store, deliver, damped flood
- **T1 sync** +≈ 80: ANNOUNCE / INV / WANT, watermarks
- **T2 route** +≈ 100: paths, directed unicast, custody

Endpoint extras (ratchet, fountain, mix) never change relays.

### 1.2 Design axioms (observed)
1. Every link is hostile — authenticity and secrecy live only in the envelope.
2. Relays never verify signatures merely to forward (they *do* verify before writing identity into local state — S-002/S-004).
3. The wire format and C ABI are frozen; the crate version is independent and still 0.x.
4. Continuity > convenience: offline rebuild from a USB stick must work decades later.
5. Claims must have code (or an explicit “Still open” entry). The security register and CHANGELOG are unusually honest about self-inflicted failures.

### 1.3 High-level architecture
```
┌─────────────────────────────────────────────────────────────┐
│  Surfaces                                                   │
│  Android (Compose + JNI)  │  Web (wasm + standalone HTML)   │
│  CLI daemon               │  Bindings (C/Python/Go/JS)      │
└───────────────┬──────────────────────────┬──────────────────┘
                │                          │
                ▼                          ▼
┌──────────────────────┐      ┌──────────────────────────────┐
│  Node / Hub (Rust)   │      │  Bridges (per medium)        │
│  - envelope decode   │◄────►│  audio, meshtastic, rnode,   │
│  - store / dedup     │      │  kiss, udp, tor, i2p, nfc…   │
│  - path learning     │      └──────────────────────────────┘
│  - fountain reasm    │
│  - seal / ratchet    │
│  - congestion        │
└──────────────────────┘
```

The core is deliberately medium-agnostic. Bridges translate between SPORE envelopes and underlay frames; the router never learns underlay addresses.

---

## 2. Core Protocol Deep Dive

### 2.1 Envelope (SPEC §2)
```
off  len  field
0    1    ver    = 0x01
1    1    type   0=DATA 1=INV 2=WANT 3=ANNOUNCE
2    1    flags  ENCRYPTED|SIGNED|FRAGMENT|ACKREQ|FLOOD|SRC8
3    1    hops   remaining (default 16, clamped ≤16)
4    4    expiry unix u32 (stores clamp to 30 d)
8    8    dest   address | topic | 0×8 = public
-- if SIGNED: src = 32-B pubkey (or 8-B if SRC8) --
     2    plen   u16
     N    payload
     64   sig    Ed25519 over body with hops zeroed
```

**ID** = SHA-256(envelope with hops=0)[0:16] — computed, never transmitted (except inside INV/WANT/frag/ack).

**Observations**
- Layout is compact and relay-friendly.
- Signature covers hops=0 so relays can decrement without invalidating.
- SRC8 optimisation is correctly restricted to peers that already hold the key.
- No priority field on the wire — priority is bought via stamp (leading zero bits of ID).

**[NEW] Info — hop clamp is correct but local.** Incoming hops are clamped to ≤16. An originator that sets hops=255 still only travels 16 hops. This is intentional and documented; no issue.

### 2.2 Fragmentation / Fountain (SPEC §3)
Payload = `[orig_id:16][index:1][count:1][chunk]`.  
- `index < count` → data chunk  
- `index ≥ count` → repair = XOR of data chunks selected by SHA-256(orig_id ‖ index) bits

Receiver needs any full-rank set (typically count+2). Signature is verified only after reassembly.

**Findings already closed**
- S-001: `count == 0` → `idx % count` panic. Fixed; regression in robustness + fuzz.
- S-011: objects needing >255 chunks now return `Result` instead of panic (`Node::send`).

**[NEW] Medium — fountain chunk size is implicit.** Chunk size is derived from the original envelope length and padding. There is no explicit maximum chunk size advertised on the wire beyond the envelope MTU of the medium. On very small media a single large original can produce many small fragments; on large media the opposite. This is by design (rateless) but means bridges must still respect their own MTU when emitting.

### 2.3 Routing & Congestion (SPEC §4–5)
- Neighbours: point-to-point + hops=0 ANNOUNCE.
- Paths: up to 3 (iface, neighbour, age) per address; first-copy-wins; fresh <3 h; purge 7 d.
- Congestion: (a) token bucket ≤10 % of interface capacity, (b) Trickle 5→80 min, (c) busy byte backpressure, (d) FLOOD exponential backoff.

**Closed findings**
- S-023: daemon was flooding mesh-wide ANNOUNCE every 5 s instead of link-local HELLO every 5 min (60× too fast + wrong form). Fixed; Android housekeeping had the same bug.

**[NEW] Info — duty-cycle claim is still unmeasured.** SPEC and findings correctly note that whether 5→80 min HELLO + hourly flood fits real LoRa EU868 1 % duty cycle is a hardware question, not a calculation. Still open.

### 2.4 Sync & Files (SPEC §6)
INV/WANT are hops=0, unsigned, never stored. Custody pushes stored unicast toward fresher paths or the destination itself.

Files are content-addressed: chunks + signed manifest (magnet = manifest ID). Nested manifests for large files. Sealed files use a file key sealed to the recipient’s prekey.

**Closed**
- S-027: hostile NUL filename stopped entire `materialize`. Fixed by skipping unwritable names.
- S-028: `complete_files()` pulled entire store into RAM. Streaming path added.

**[NEW] Low — magnet / transfer progress honesty.** Android correctly distinguishes “served from this node” vs “fetching”. This is a rare and valuable honesty property; most messengers lie about delivery.

---

## 3. Cryptography

### 3.1 Primitives
- Sign: Ed25519 (dalek 2.1.1 pinned)
- Seal (one-shot): libsodium-compatible `crypto_box_seal` to X25519 prekey
- Sessions: Double Ratchet (BLAKE2b KDF, ChaCha20-Poly1305)
- Topics: XChaCha20-Poly1305 + healing mix (S-020)
- Hash: SHA-256 for addresses/IDs; BLAKE2b for ratchet

Pins for MSRV 1.75: `zeroize = 1.7.0`, `zeroize_derive = 1.4.2`, `base64ct = 1.6.0`. Lockfile kept at version 3.

### 3.2 Prekey ring (S-022) — closed correctly
- Up to 16 prekeys, oldest first.
- Mint random daily; delete secrets older than 7 days; newest never deleted.
- Opening tries every live entry newest-first.
- **Critical requirement satisfied:** secrets are random and **not** derived from the identity seed. Restoring from seed recovers address + signing key but not deleted prekey secrets.
- Android and browser now persist the ring separately.

**Residual**
- A backup of the ring defeats the 7-day window (documented, correct).
- No platform has field-verified the 7-day window end-to-end (“Still open”).

### 3.3 Double Ratchet
Payload = `[dh_pub:32][n:2][pn:2][ct]`.  
Skipped message keys cached, bounded by `MAX_SKIPPED_KEYS` (count only).

**Still open (S-024a)**
- Spec claims 7-day window; code is count-bounded only.
- Nothing is zeroised on drop anywhere in the crate.

**[NEW] High — skipped-key cache lifetime is a real forward-secrecy gap.** An attacker who obtains the device after a long offline period can still open messages whose keys were skipped and cached indefinitely. Closing this requires threading time into the ratchet and zeroizing.

### 3.4 Topic keys & healing (S-020)
- `rotate(k) = SHA-256(k ‖ "spore-keyrot-v1")` — pure forward secrecy, no healing.
- Healing: sealed fresh entropy folded via `mix(k,c)`. Up to 256 boxes, no recipient hints.
- `key_id` makes divergence legible.

**Still open**
- Encrypted groups have **no roster**. Membership is an application problem. In a partition two halves can diverge onto different keys; only `key_id` makes it visible.

**[NEW] Medium — group membership is the largest honest gap between SPORE groups and a real messenger.** Distributed agreement is hard; the project correctly refuses to pretend it has solved it. Applications must handle it or accept divergence.

### 3.5 Mix mode (SPEC §9)
Onion of nested sealed envelopes, Poisson delay, size-class padding, optional decoys. Correctly scoped: beats local observers and any subset of mixes; global passive observer only while decoys flow.

---

## 4. Security Findings Synthesis

### 4.1 Project’s own register (S-001 … S-031)
The register is exemplary: each finding has root cause, reproduction, patch, behaviour change, freeze impact, and tests. Self-inflicted release bugs are documented with the same rigour as remote DoS.

**Closed remote-DoS / crash class**
- S-001 (fountain zero count)
- S-005 (C ABI unwind)
- S-018 (mutex poison)
- S-019 (Meshtastic varint overflow)
- S-031 (audio demod CPU saturation)

**Closed trust / amplification**
- S-002/S-004 (SIGNED flag vs verified signature)
- S-003 (weak stamp bypassed quotas)
- S-012 (WANT amplification)
- S-006/S-013/S-016/S-017 (unbounded tables)

**Closed crypto honesty**
- S-022 (prekey ring actually exists)

**Process / release class (self-inflicted)**
- S-021, S-025, S-026, S-029, S-030 — all release-pipeline bugs. Pattern: fix verified on one artefact, assumed on neighbour.

### 4.2 Still open (from register)
1. Ratchet skipped-key cache age + zeroization (S-024a)
2. `mark_seen` vs `ingest` 30-day floor disagreement (S-024b) — harmless
3. Beacon duty-cycle unmeasured on real radio
4. 7-day prekey window not field-verified on any platform
5. Encrypted groups have no roster
6. `with_node` reentrancy documented but not enforced
7. Hardware-verified status for every 🧪 bridge

### 4.3 New observations from this audit

**[NEW] High — Android audio path was a permanent core saturator (S-031).**  
Any non-decoding audio (noise, music, speech, silent mic) caused `Demod::push` to rescan the entire retained buffer every call. Work grew linearly with buffer length; at the 175 s cap it was ~27× real time. On Android this runs in a foreground service fed by `AudioRecord`. Fixed with a scan cursor; cost now flat ~1.5 ms. Regression tests exist. This was correctly rated High.

**[NEW] Medium — Release pipeline remains the weakest tested subsystem.**  
Five consecutive findings, zero automated tests that exercise the full publish path without publishing. Current design (asset clearing, dual v/V checks, Cargo.toml as source of truth) looks correct, but the pattern of “green job, wrong artefact” has repeated. Recommendation: staging / dry-run mode or recorded GitHub API fixtures.

**[NEW] Medium — Identity seed storage has had repeated migration bugs.**  
CHANGELOG 0.6.0: “Reveal seed showed `unavailable` on every upgraded install.” The encryption change moved the seed into Keystore-backed store and cleared the plaintext file, but the Advanced screen still read the plaintext prefs. Classic “verified on the path that was changed, assumed on the neighbour.” One accessor now exists; residual risk remains whenever storage layout changes again.

**[NEW] Low — `allowBackup` story.**  
0.5.0 correctly set `allowBackup="false"` + extraction rules + EncryptedSharedPreferences after the Google Drive backup of identity + prekeys. Good. Confirm that device-to-device transfer rules also cover the Keystore-backed material (Android’s backup rules can be subtle).

**[NEW] Info — Freeze guard is real and well-engineered.**  
`pr-guard.yml` blocks the golden vectors, C header, api_freeze test, gen_vectors, and the guard workflows themselves unless `allow-frozen-change` is present. Label trigger includes `labeled`/`unlabeled` so the escape hatch actually works. This is stronger than most projects’ “we promise not to break the API.”

---

## 5. Android App Deep Dive

### 5.1 Structure
```
android/
├── app/src/main/kotlin/org/spore/node/
│   ├── MainActivity.kt
│   ├── NodeController.kt / NodeService.kt
│   ├── SporeNative.kt          # JNI façade
│   ├── AudioBridge.kt
│   ├── BleBridges.kt           # Meshtastic + RNode
│   ├── WifiDirectBridge.kt
│   ├── ChatScreens.kt
│   ├── FeedScreens.kt
│   ├── NodeScreens.kt
│   ├── Chrome.kt               # Visual language
│   ├── Markdown.kt, Petnames.kt, Qr.kt, WebBridgeHost.kt
│   └── …
├── jni/                        # Rust cdylib via cargo-ndk
└── PLAN.md, README.md
```

### 5.2 Architecture notes
- Single long-lived Rust `Node` behind JNI, owned by a foreground `NodeService`.
- Kotlin side is mostly UI + bridge pumps (poll forward / push RX).
- State flows through `StateFlow`s in `NodeController` (messages, peers, transfers, etc.).
- Permissions are gated in UI before `start()` on bridges.

### 5.3 Bridges

**AudioBridge**
- 48 kHz mono float PCM ↔ Rust 16-FSK modem.
- RX: `AudioRecord` → `nativeAudioDemodPush` → pop frames → `nativePushRx`.
- TX: `nativePollForward` → `nativeAudioModulate` → `AudioTrack`.
- Correctly requires `RECORD_AUDIO`.
- S-031 fixed the demod saturation.

**MeshtasticBleBridge**
- Service UUID `6ba1b218-…`, ToRadio / FromRadio / FromNum.
- Wraps SPORE envelopes as MeshPackets on portnum 256.
- Manual protobuf length encoding for ToRadio; FromRadio field-2 extraction.
- Chunked writes with pacing (`Thread.sleep(15)`).
- Uses deprecated characteristic APIs (pre-33) — acceptable for current minSdk, watch for future.

**RNodeBleBridge**
- Nordic UART Service.
- KISS framing with radio config commands (freq, bw, sf, cr, txpower, RADIO_STATE).
- Streaming KISS de-framer that keeps command byte; only DATA frames pushed to node.

**[NEW] Medium — BLE write pacing is best-effort.**  
`writeChunked` uses fixed sleep rather than write-callback flow control. On busy controllers or slow links this can still overflow the radio’s buffer. Not a protocol bug, but a reliability concern on real hardware (all bridges are still 🧪).

**[NEW] Info — MTU request is present (247) but not reactive.**  
Connection requests 247 then discovers services. No fallback path if the peer rejects the MTU; chunk size is hardcoded later. Common pattern, but worth noting for constrained radios.

### 5.4 UI / UX (Chrome.kt + screens)

`Chrome.kt` is a faithful, high-quality implementation of `VISUALDESIGN.md`:

| Spec requirement | Implementation |
|------------------|----------------|
| Hard edges ≤2 dp | `CrateShape = RoundedCornerShape(2.dp)` |
| 4 dp hard offset shadow, no blur | Hand-drawn in `Modifier.crate` / button (Compose `shadow` is blurred) |
| Never pink on olive | `StickerBadge` owns its background; pink variants on void |
| Amber on olive large-text only | Buttons 15 sp bold |
| Monospace body | `SporeTypography` forces `FontFamily.Monospace` |
| Reduced motion | `ANIMATOR_DURATION_SCALE == 0` → no scanlines, no bloom, no throw animation |
| Scanlines ≤6 % | 2 dp period, 0.06 alpha, gated |
| Toughbook screws | Four 3 dp kevlar dots |
| Segmented LED | Discrete blocks, not smooth bar |
| Focus visible | Cyan 2 dp ring on Toughbook |

Chat correctly signals mine/theirs by **side + border colour**, not colour alone.  
File bubbles report honest progress (“served from this node” vs “fetching”).  
Petname save button disables when value matches stored (trimmed) value and confirms.

**Gaps vs spec**
- No Impact / Haettenschweiler (documented; system black sans used).
- Clack + particle burst deliberately omitted until an explicit sound setting exists (§7 “sound off by default”).
- No unread counts (correctly omitted — no read tracking exists).

**[NEW] Low — contrast ratios are measured and cited.**  
Palette comments carry real WCAG numbers. This is rare and valuable; most design systems only claim “accessible.”

### 5.5 Storage & identity
- Seed + prekey ring in EncryptedSharedPreferences over Keystore master key.
- `allowBackup="false"` + data extraction rules.
- Migration path from plaintext prefs exists; 0.6.0 fixed the “reveal seed unavailable” regression.

**[NEW] Medium residual risk — future storage layout changes.**  
Any new encrypted field that is not routed through the single accessor can recreate the “UI reads old location” class of bug. The single-accessor pattern should be treated as load-bearing.

### 5.6 Permissions & lifecycle
- Bridges request permissions before start.
- Foreground service for the node — correct for long-lived radio/audio work.
- No obvious leak of the JNI pointer across process death (assumed restart path exists; not fully visible in digest).

---

## 6. Documentation Quality

| Document | Quality | Notes |
|----------|---------|-------|
| `SPEC.md` | Excellent | Two-page normative core + bindings; clear threat model; honest limits |
| `VISUALDESIGN.md` | Excellent | Tokens, measured contrast, implementation status table, constraints that outrank taste |
| `SECURITY_FINDINGS.md` | Exemplary | Reproduction, root cause, patch, tests, self-criticism |
| `CHANGELOG.md` | Excellent | Wire status on every entry; S-nnn links; process failures documented |
| `CONTRIBUTING.md` | Strong | Freeze surface explicit; release procedure clear; why-PR-not-button explained |
| `CONTINUITY.md` (referenced) | Strong (by CI) | Offline bundle is proven in CI, not just claimed |

**[NEW] Info — docs/code sync is CI-enforced.**  
`scripts/check_docs_sync.py` + regeneration of `reference/vectors.json` on every Linux CI run. Documented values cannot drift silently.

**[NEW] Low — front-page voice discipline.**  
VISUALDESIGN correctly separates “full flavour” (node UI) from “plain language” (Pages front). This is easy to violate; the rule is good.

---

## 7. CI / CD & Release Process

### 7.1 Workflows
- **ci.yml** — fmt, clippy (-D warnings), test (3 OS), wasm single-import, web tests, docs site link check, Android JNI lint, reference vectors.
- **pr-guard.yml** — frozen contract files; label escape hatch that actually works.
- **msrv.yml** — real 1.75 toolchain + lockfile parseability.
- **supply-chain.yml** — cargo-audit, cargo-deny, bindings regen, offline bundle cold build.
- **fuzz.yml** — nightly + on-demand, 6 targets, seeds committed.
- **android.yml** — multi-ABI cargo-ndk, version from Cargo.toml, rolling + nightly + tagged, checksums, dual v/V handling.
- **release.yml** — human chooses major/minor; opens PR; merge cuts the release inside android.yml.
- **pages.yml** — docs site + standalone demo + seed sheet.

### 7.2 Freeze surface (load-bearing)
```
tests/api_freeze.rs
reference/vectors.json
bindings/spore.h
examples/gen_vectors.rs
examples/worked.rs
.github/workflows/{ci,pr-guard}.yml
(+ site/seed tests, web tests)
```

### 7.3 Release process evaluation
Current design is sophisticated and addresses every prior finding:
- `major.minor` only human-touched part (Cargo.toml).
- Rolling = `<mm>.<YYYYMMDDHHMM>+<sha7>`.
- Tag must match Cargo.toml major.minor or build fails.
- Assets cleared in place (no tag delete/recreate race).
- Both `v*` and `V*` checked.
- Nightlies pruned to last 5.
- Permanent download links (`spore-android.apk`) for rolling and latest.

**[NEW] Medium — still no automated test of the publish path.**  
The subsystem has produced five findings while being verified only by inspecting artefacts after the fact. A dry-run or recorded API fixture suite would break the pattern.

**[NEW] Info — version scheme is honest.**  
Distribution version ≠ protocol version. CHANGELOG and release notes state this explicitly. Good.

---

## 8. Testing & Fuzzing

- Always-on robustness harness (`src/robustness.rs`) found S-001 on first run.
- Six libFuzzer targets in isolated workspace (nightly only): envelope_decode, node_on_rx, fragment_reassembly, armor_and_framing, seal_open, radio_codecs.
- Seeds committed for radio_codecs (smart — avoids rediscovering framing).
- Cross-language vectors + pure-Python T0 + C/shell reference decoders.
- Wasm: exactly one import (`spore_fill_random`), standalone self-containment greps.
- Docs site: internal link + anchor resolution fails the build.

**[NEW] Info — fuzz oracle for radio_codecs is stronger than panic-only.**  
It asserts frame length bounds and that the framer does not accumulate unbounded state. This is the right property for a parser that anyone with a transmitter can feed.

---

## 9. Supply Chain & Continuity

- `deny.toml`: licences allowlist (permissive only), no wildcards, crates.io only, advisories deny, multiple-versions warn.
- Offline bundle script + CI job that vendors then builds with empty `CARGO_HOME`.
- MSRV enforced by real toolchain job.
- Dependabot grouped, weekly cargo / monthly actions.
- Bindings regenerated from `spec.json` and diff-checked in CI.

**[NEW] Info — continuity story is unusually real.**  
Most projects claim “you can rebuild offline.” SPORE’s CI actually does it on every relevant change and publishes the bundle. This is the difference between a claim and a property.

---

## 10. UX / UI Compliance Scorecard

| VISUALDESIGN rule | Android status | Notes |
|-------------------|----------------|-------|
| Zero network requests (standalone) | N/A (native) | Web standalone enforced by CI |
| Motion opt-out | ✅ | `ANIMATOR_DURATION_SCALE` |
| Contrast measured | ✅ | Numbers in Palette comments |
| Never pink on olive | ✅ | StickerBadge owns background |
| Hard edges / offset shadow | ✅ | Hand-drawn |
| Toughbook + screws | ✅ | |
| Radio switch throw | ✅ | |
| Segmented LED | ✅ | |
| Scanlines gated | ✅ | |
| Sound off by default | ✅ (absent) | Correct until setting exists |
| Failure not colour-alone | ✅ | Icons + words |
| Side + border for mine/theirs | ✅ | |
| Honest transfer status | ✅ | “served from this node” |

**Overall:** Android is the strongest current consumer of the design language. The three documented divergences (no Impact, hand-drawn shadow, inferred reduced motion) are honest and recorded in the spec itself.

---

## 11. Prioritised Recommendations

### P0 — Security / Correctness
1. **Age-bound + zeroize ratchet skipped keys** (close S-024a). Spec already claims 7 days.
2. **Field-verify the 7-day prekey window** on at least one Android device (and document the procedure).
3. **Hardware verification campaign** for at least Meshtastic BLE and RNode; update 🧪 markers and `HARDWARE.md`.

### P1 — Robustness / Honesty
4. **Release pipeline dry-run / fixture tests** so the next process bug is caught before artefacts are wrong.
5. **Treat single seed/prekey accessor as load-bearing**; any new encrypted field must go through it.
6. **Surface “Still open” items** more visibly on the docs site and in the app’s Advanced / About screen.

### P2 — Protocol / Product
7. **Group membership story** — even a minimal signed roster or “key_id divergence warning” in the UI would reduce the largest honest gap.
8. **BLE write flow-control** (callback-based rather than fixed sleep) before claiming production readiness on radio bridges.
9. **Duty-cycle measurement** on real LoRa hardware; publish numbers next to the Trickle parameters.

### P3 — Polish
10. Sound + particle feedback behind an explicit user setting.
11. Consider making audio demod buffer limits configurable under memory pressure.
12. Keep the “docs cannot drift from code” property sacred; extend the sync checker when new concrete values are documented.

---

## 12. Positive Findings (worth preserving)

- Living security register with reproduction and regression tests.
- Freeze guards that are mechanical, not aspirational.
- Offline continuity proven in CI.
- Visual design language that is normative and mostly implemented.
- Honest transfer / delivery status language.
- Cross-language vectors + reference decoders.
- Fuzz targets with property oracles, not just “did not panic.”
- CHANGELOG that documents process failures with the same care as crypto bugs.
- Clear separation of protocol version (v1 frozen) from distribution version (0.x).

---

## 13. Summary Verdict

SPORE is a **high-craft, security-conscious, continuity-first** opportunistic messaging system. The protocol design is sound, the threat model is taken seriously, the freeze surface is real, and the Android UI is both distinctive and largely faithful to its own design system.

The largest residual risks are operational and lifetime-related rather than cryptographic design errors:

1. Unverified radio bridges  
2. Ratchet skipped-key lifetime + zeroization  
3. Absence of group membership consensus  
4. Historically fragile (but currently improved) release plumbing  
5. Incomplete field verification of the 7-day forward-secrecy window  

If the next major cycle closes the P0 items and lands at least one hardware-verified radio path, SPORE will be in a genuinely strong position for real-world disrupted / offline-first use.

---

*End of mega-audit document. This file can be extended incrementally as new code, hardware results, or findings appear.*

---

# PART II — EXHAUSTIVE EXPANSION (2026-07-28)

*This part supersedes and deepens §5 (Android), expands core issues with concrete proposed fixes, and adds line-by-line evaluation of every Android source and related doc present in the digest.*

---

## 14. Android — Exhaustive Line-by-Line Audit

### 14.1 AndroidManifest.xml

Key permissions: INTERNET, WIFI_*, FOREGROUND_SERVICE*, POST_NOTIFICATIONS, RECORD_AUDIO, CAMERA (optional), BLUETOOTH_CONNECT + legacy maxSdk 30, NEARBY_WIFI_DEVICES neverForLocation, ACCESS_FINE_LOCATION maxSdk 32.

Application flags: allowBackup=false, dataExtractionRules, fullBackupContent=false. Activity MainActivity exported launcher. Service NodeService exported=false foregroundServiceType=dataSync.

| ID | Severity | Finding | Proposed fix |
|----|----------|---------|--------------|
| A-M1 | Info | allowBackup=false + extraction rules correct after S-022 | Keep |
| A-M2 | Low | Comment above application correctly avoids XML well-formedness trap | — |
| A-M3 | Medium | No RECEIVE_BOOT_COMPLETED. Node dead after reboot until user opens app | Optional boot receiver gated by user setting (default off) |
| A-M4 | Low | FGS type dataSync only; audio modem may need microphone type on API 34+ | Monitor; add microphone type when audio active |
| A-M5 | Info | Camera required=false — invites can be pasted | — |
| A-M6 | Low | No networkSecurityConfig; cleartext possible for LAN | Optional config permitting cleartext only to link-local/RFC1918 |

### 14.2 data_extraction_rules.xml

Both cloud-backup and device-transfer exclude sharedpref/file/external/database at domain root (path=".").

| ID | Severity | Finding | Proposed fix |
|----|----------|---------|--------------|
| A-X1 | Medium | path="." exclusion reasoned, not device-tested (ANDROID_AUDIT admits this) | Manual test: old phone to new phone; confirm spore.xml and store absent |

### 14.3 NodeService.kt

START_STICKY, foreground notification (IMPORTANCE_LOW, ongoing), MulticastLock non-refcount named spore, calls NodeController.start. onDestroy releases lock only.

| ID | Severity | Finding | Proposed fix |
|----|----------|---------|--------------|
| A-S1 | Medium | onDestroy does not free native Runtime; sticky restart can orphan handles until process death | NodeController.stop() that cancels jobs, stops bridges, nativeFree if last owner |
| A-S2 | Low | MulticastLock non-refcount — safe; release present | — |
| A-S3 | Info | No wake lock — correct for store-and-forward; audio may need PARTIAL_WAKE_LOCK while active | Acquire on AudioBridge.start, release on stop |

### 14.4 SporeNative.kt

Thin external fun façade matching Java_org_spore_node_SporeNative_*. Covers new/free, addr/seed/prekey ring, subscribe/send, UDP/TCP, registerIface(+limited), pollForward/pushRx, audio demod/modulate, meshtastic wrap/unwrap, sendDirect/acked/resend, files, invites, store budget/spill.

| ID | Severity | Finding | Proposed fix |
|----|----------|---------|--------------|
| A-N1 | Info | Prekey ring KDoc restates S-022 rationale at call site | — |
| A-N2 | Medium | No nativeStopBridge / unregisterIface — bridges append-only | Add nativeUnregisterIface; stop pumps; drop Receiver |
| A-N3 | Low | nativeSuggestedBulkBudget returns -1 for unknown | Document known kinds: audio, meshtastic, reticulum |
| A-N4 | Info | Rust live-handle registry turns bad jlong into no-op | — |

### 14.5 android/jni/src/lib.rs

Runtime { hub, inbox, ifaces HashMap, demod, demod_out }. Live HashSet registry. register / register_limited / pollForward / pushRx / audio path.

| ID | Severity | Finding | Proposed fix |
|----|----------|---------|--------------|
| A-J1 | Info | Poll model (no Rust to Kotlin callbacks) correct for JNI lifetime | — |
| A-J2 | Medium | ifaces map grows if register without unregister | Pair with A-N2 |
| A-J3 | Low | demod_out VecDeque uncapped if Kotlin pop stalls | Cap e.g. 32 frames; drop oldest |
| A-J4 | Info | register_limited bulk budget — messages/announces exempt | — |

### 14.6 AudioBridge.kt

48 kHz float mono; AudioRecord to nativeAudioDemodPush/Pop to pushRx; pollForward to modulate to AudioTrack. IO coroutines; stop cancels and releases.

| ID | Severity | Finding | Proposed fix |
|----|----------|---------|--------------|
| A-A1 | Info | S-031 fix is in Rust; Kotlin clean | — |
| A-A2 | Medium | stop() does not null jobs/record/track — not re-entrant-safe | Null fields after release |
| A-A3 | Low | No audio focus | Request AUDIOFOCUS_GAIN_TRANSIENT while TX |
| A-A4 | Low | delay(50) empty TX poll | Document power cost |
| A-A5 | Info | MissingPermission suppress with UI gate comment | — |

### 14.7 BleBridges.kt

Base: connectGatt, MTU 247, discover, TX pump 60 ms, CCCD notify, writeChunked sleep(15) NO_RESPONSE.

Meshtastic: portnum 256, manual protobuf ToRadio, FromNum drain up to 32 reads with 80 ms delay, stripField2 walker.

RNode: NUS + KISS, config cmds, streaming de-framer.

| ID | Severity | Finding | Proposed fix |
|----|----------|---------|--------------|
| A-B1 | Medium | writeChunked fixed sleep, not callback flow-control | WRITE_TYPE_DEFAULT + onCharacteristicWrite continuation or bounded queue |
| A-B2 | Medium | Meshtastic drain launches new coroutine per FromNum; overlapping drains possible | Single drain job; coalesce; use read callback |
| A-B3 | Low | Deprecated c.value / writeDescriptor | Migrate when minSdk >= 33 |
| A-B4 | Low | RNode config fire-and-forget | Parse KISS responses for radio-up |
| A-B5 | Info | stripField2 minimal walker — small attack surface | — |
| A-B6 | Medium | No reconnect on disconnect | Exponential backoff reconnect, UI status |
| A-B7 | Low | pktId random then increment | — |

### 14.8 WifiDirectBridge.kt

createGroup -> nativeStartUdpLimited on success or BUSY. stop removeGroup.

| ID | Severity | Finding | Proposed fix |
|----|----------|---------|--------------|
| A-W1 | Medium | UDP started without verifying P2P iface actually up | Listen for CONNECTION_CHANGED / group info first |
| A-W2 | Low | No peer discovery — group only | Document |
| A-W3 | Info | Limited broadcast 255.255.255.255 correct when subnet unknown | — |

### 14.9 ChatScreens.kt

ChatsList: own address copy/share, nearby peers, threads, open-by-16-hex.
ChatDetail: petname dirty-enabled save, bubbles side+border, FragmentStatus honest served vs fetching, file picker, pink-on-void send.

| ID | Severity | Finding | Proposed fix |
|----|----------|---------|--------------|
| A-C1 | Info | Side+border not colour alone — compliant | — |
| A-C2 | Info | No unread counts (no read tracking) — correctly omitted | — |
| A-C3 | Low | messages takeLast(1000) UI window | Document or paginate from store |
| A-C5 | Info | Honest file transfer status — preserve | — |

### 14.10 FeedScreens.kt

Topic chips, follow, posts, composer with markdown toolbar and image attach, postWithImage, inSampleSize decode.

| ID | Severity | Finding | Proposed fix |
|----|----------|---------|--------------|
| A-F1 | Info | Cap by refuse-edit preserves caret | — |
| A-F2 | Medium | Magic 220 dp for inSampleSize | Derive from composable max height |
| A-F3 | Low | No snackbar on follow | Confirm subscribe |
| A-F4 | Info | Image as file + spore: magnet markdown | — |

### 14.11 NodeScreens.kt

Connect: invite QR/scan/paste, petname, opt-in bridges (security-correct — invite unauthenticated).
Bridges: LED+word status, add rows, no stop.
Advanced: seed via single accessor.

| ID | Severity | Finding | Proposed fix |
|----|----------|---------|--------------|
| A-NS1 | Info | Invite bridges opt-in — correct | — |
| A-NS2 | Medium | Bridge list append-only | A-N2 + SwipeToDismiss / CrateSwitch |
| A-NS3 | Medium | No FileProvider ACTION_VIEW for received files | Add FileProvider; Open action on complete |
| A-NS4 | Low | Camera deny falls back to paste | — |

### 14.12 MainActivity.kt

Screen graph, crate app bar, bottom nav pink block+label, TransfersBar, ReceivingBar, snackbar CompositionLocal, permission launchers.

| ID | Severity | Finding | Proposed fix |
|----|----------|---------|--------------|
| A-MA1 | Info | Bottom nav colour paired with position/text | — |
| A-MA2 | Low | TransfersBar shows first 3 only | Document |

### 14.13 NodeController.kt

Owns node, EncryptedSharedPreferences seed+ring, polls, housekeeping, bridges, invites, files. Msg/Peer/Transfer/Post/BridgeState models.

| ID | Severity | Finding | Proposed fix |
|----|----------|---------|--------------|
| A-NC1 | High mitigated | Historical plaintext+backup fixed; single accessor after 0.6.0 | Keep accessor sacred |
| A-NC2 | Medium | takeLast(1000) | See A-C3 |
| A-NC3 | Medium | Housekeeping constants must match SPEC 5->80 min / hourly | Assert in comment or test |
| A-NC4 | Low | applyInviteBridges only ws/nostr/wt/tcp | Correct (BLE needs device picker) |
| A-NC5 | Info | PUBLIC dest = 8 zero bytes | — |

### 14.14 Chrome.kt UX compliance

Palette with measured contrast; crate 2dp/4dp hard shadow; Toughbook screws+cyan focus; CrateButton 15sp bold throw; SegmentedLed; scanlines gated; reducedMotion via ANIMATOR_DURATION_SCALE; StickerBadge owns background; monospace Typography; DisplayHeading bloom gated.

| ID | Severity | Finding | Proposed fix |
|----|----------|---------|--------------|
| A-UX1 | Info | Strongest VISUALDESIGN consumer | Preserve |
| A-UX2 | Low | No clack/particles yet | Setting default off then wire |
| A-UX3 | Low | No Baud empty-state mascot | Embedded vector |
| A-UX4 | Info | Focus never removed | — |

### 14.15 PLAN.md vs reality

M0-M5 built. Bridges present as code, all hardware-unverified. Petnames+frag status yes. CI APK yes. Start-on-boot no. Bridge enable/disable open. FileProvider open.

### 14.16 ANDROID_AUDIT.md

Aligns: backup fixed, UI mostly fixed not device-run, open items match this audit. Honesty about not running on device is exemplary.

---

## 15. Core Issues — Expanded with Proposed Fixes

### 15.1 S-024a Ratchet skipped-key TTL + zeroize (High)

Spec claims 7-day window; code is count-only; no zeroize on drop.

Proposed fix sketch:

```rust
struct SkippedKey { key: [u8; 32], inserted_at: u32 }
const SKIP_TTL_SECS: u32 = 7 * 24 * 3600;

fn purge_skipped(&mut self, now: u32) {
    self.skipped.retain(|k| now.saturating_sub(k.inserted_at) < SKIP_TTL_SECS);
}
impl Drop for Ratchet {
    fn drop(&mut self) {
        for k in self.skipped.drain(..) { k.key.zeroize(); }
        self.ck_in.zeroize(); self.ck_out.zeroize();
    }
}
```

Thread now into receive path. Regression: insert, advance now 7d+1, assert open fails. Freeze impact: none if internal.

### 15.2 Group roster absence (Medium)

No membership list; partition can diverge keys; only key_id visible.

Minimal fix: (1) UI badge on key_id mismatch; (2) optional application-level signed roster payload convention (0x0A + addrs + sig) — advisory only; (3) never claim consensus.

### 15.3 mark_seen vs ingest (Low)

mark_seen floor without now is dead. Prefer delete dead code + comment that ingest is the real expiry path; or pass now and retain properly.

### 15.4 Beacon duty-cycle (Info/open)

SPEC numbers correct post S-023; real LoRa EU868 1% unmeasured. HARDWARE.md procedure + optional per-iface Trickle knobs.

### 15.5 with_node reentrancy (Low)

Document louder; debug_assertions reentrancy flag; longer term with_node_ref or careful reentrant subset.

### 15.6 Defence in depth

Cap demod_out (A-J3). Ensure every peer-growable table has count + age. Optional adversarial mode tightening quotas under sustained pressure.

---

## 16. UX Investigation Summary

Strengths: normative design implemented; honest status language; contrast measured; reduced motion; invite opt-in bridges; dirty-enabled saves.

Gaps: bridge on/off (Med); FileProvider (Med); sound/particles (Low); Baud (Low); boot (Low); 1000-msg window (Low); ring health UI (Med); never device-tested (process High).

Accessibility: colour never sole signal; focus visible; ensure contentDescription on badges/LEDs that carry meaning.

---

## 17. Coverage Matrix (Android + related)

| File | Reviewed | Residual |
|------|----------|----------|
| AndroidManifest.xml | Complete | Boot, FGS types |
| data_extraction_rules.xml | Complete | Device-transfer untested |
| NodeService.kt | Complete | nativeFree on destroy |
| SporeNative.kt | Complete | No stop bridge |
| jni/src/lib.rs | Structure + key exports | Full every-fn body partial in digest |
| AudioBridge.kt | Complete | Re-entrancy, focus |
| BleBridges.kt | Complete | Write pacing, drain, reconnect |
| WifiDirectBridge.kt | Complete | Group-up verification |
| ChatScreens.kt | Complete | 1000-msg window |
| FeedScreens.kt | Mostly | Image sample size source |
| NodeScreens.kt | Mostly | Bridge stop, FileProvider |
| MainActivity.kt | Mostly | — |
| NodeController.kt | Structure + critical paths | Housekeeping constants |
| Chrome.kt | Complete | — |
| PLAN.md / ANDROID_AUDIT.md | Complete | Aligns |

---

## 18. Consolidated New Finding IDs

A-M3 Med no boot; A-S1 Med no nativeFree on destroy; A-N2 Med no unregister; A-J2 Med ifaces growth; A-J3 Low demod_out; A-A2 Med AudioBridge stop; A-B1 Med BLE write sleep; A-B2 Med overlapping drains; A-B6 Med no reconnect; A-W1 Med WiFi Direct without group confirm; A-NS2 Med append-only bridges; A-NS3 Med no FileProvider; A-NC3 Med housekeeping vs SPEC; A-X1 Med device-transfer untested; Core15.1 High ratchet TTL; Core15.2 Med group roster UX; Core15.3 Low mark_seen; UX ring health Med.

---

## 19. Recommended Implementation Order (Android-focused)

1. Device test matrix (install, migrate, reveal seed, backup exclusion, 12h soak)
2. nativeUnregisterIface + bridge stop UI
3. FileProvider + Open
4. Service onDestroy controlled shutdown
5. BLE write flow-control + single drain + reconnect
6. Ratchet skip TTL + zeroize
7. Ring health UI + encrypted export with FS warning
8. Optional boot, audio focus, Baud, sound toggle

---

*End of Part II. Open for Part III when remaining Rust src/ modules or hardware results are supplied.*

---

# PART III — CORE RUST LINE-BY-LINE (full tree dump)

*Source: complete repository dump including `src/{envelope,fountain,ratchet,seal,topic,store,node/*,bridge/*}`. Continues from Part II.*

---

## 20. Core module inventory

| Module | Role | Key properties verified |
|--------|------|-------------------------|
| `envelope.rs` | Wire object | VER=1, flags, hops-zeroed ID, stamp PoW, decode never panics |
| `fountain.rs` | Rateless GF(2) | count==0 rejected (S-001), MAX 255, selection bounded |
| `ratchet.rs` | Double Ratchet | MAX_SKIP=512, MAX_SKIPPED_KEYS=4*512, **count-only, no age/zeroize** |
| `seal.rs` | One-shot seal | Stateless; FS lives in Node prekey ring; open returns Option |
| `topic.rs` | Encrypted topics | rotate (hash chain), contribute/absorb healing, MAX_MEMBERS cap |
| `node/identity.rs` | Prekey ring | Random secrets, born timestamps, sweep, restore validates public |
| `store.rs` | Persistence | Disk-backed with mem budget (referenced) |
| `congestion.rs` | Quotas/Trickle | Present |
| `bridge/audio.rs` | 16-FSK modem | S-031 scan cursor |
| `bridge/hub.rs` | Multi-iface | Poison recovery (S-018) |
| `robustness.rs` | Always-on harness | Found S-001 |

---

## 21. Envelope (`src/envelope.rs`)

**Layout confirmation** matches SPEC §2 byte-for-byte.

```rust
body(zero_hops): VER | typ | flags | hops|0 | expiry BE | dest | [src] | plen BE | payload
id = SHA-256(body(true) ‖ sig)[..16]
stamp = leading zero bits of id
```

**Decode contract:** returns `Err` (Short/Version/Bad), never panics. Primary fuzz target.

**Findings**
| ID | Sev | Finding | Proposed fix |
|----|-----|---------|--------------|
| C-E1 | Info | hops zeroed only in sign/id preimage — relays can decrement safely | — |
| C-E2 | Info | SRC8 requires `verify_with` external key — correct restriction | — |
| C-E3 | Low | `stamp()` counts leading zeros across full 16-byte id — can exceed 128 in theory; used as u8 priority class | Cap display/compare at 128 if needed; functionally fine |

---

## 22. Fountain (`src/fountain.rs`)

```rust
FRAG_OVERHEAD = 36
MAX_FOUNTAIN_CHUNKS = 255  // wire u8
selection(orig_id, idx, count): rejects count==0 || count>256
Fountain::add: if count==0 return None  // S-001 closed
```

Gaussian elimination over GF(2); linearly dependent rows discarded; solve when rank==count.

**Findings**
| ID | Sev | Finding | Proposed fix |
|----|-----|---------|--------------|
| C-F1 | Info | S-001 closed at both selection and add | — |
| C-F2 | Info | count mismatch or chunk size mismatch → discard (no panic) | — |
| C-F3 | Low | `done.clone()` on every add after complete — minor alloc | Return reference or flag; not security |
| C-F4 | Info | Objects >255 chunks correctly deferred to file/manifest layer | — |

---

## 23. Double Ratchet (`src/ratchet.rs`) — critical

```rust
const MAX_SKIP: u16 = 512;
const MAX_SKIPPED_KEYS: usize = 4 * MAX_SKIP as usize; // 2048

// Skipped keys stored under (dh_pub, n)
// DH ratchet resets nr; each step opens fresh 512 window
// Total map capped at 2048 — S-017 class closed for count
```

**Confirmed: still no age bound and no zeroize.**

```rust
// No inserted_at on skipped keys
// No Drop impl zeroizing mk / ck / rk / skipped
// skip() refuses gap > MAX_SKIP
// Tests: the_skipped_key_cache_cannot_grow_without_bound, absurd_gap_refused
```

**Findings**
| ID | Sev | Finding | Proposed fix |
|----|-----|---------|--------------|
| C-R1 | **High** | Skipped keys held indefinitely until count eviction; SPEC §7 claims 7-day window | Add `inserted_at: u32` per skipped key; purge with `now` on every open/skip; Drop zeroize |
| C-R2 | **High** | No `Zeroize`/`Drop` on Ratchet secrets | `impl Drop` zeroizing rk, cks, ckr, dhs_sec, all skipped keys |
| C-R3 | Info | MAX_SKIPPED_KEYS correctly bounds total across chains (S-017 style) | Keep |
| C-R4 | Info | Absurd gap refused — good cheap guard | — |
| C-R5 | Low | KDF domain tags `"spore-ratchet-rk"` distinct from chain tags | Good hygiene |

**Concrete patch for C-R1/C-R2 (refined from Part II):**

```rust
struct SkippedKey {
    key: [u8; 32],
    inserted_at: u32,
}
const SKIP_TTL_SECS: u32 = 7 * 24 * 3600;

fn purge_skipped(&mut self, now: u32) {
    self.skipped.retain(|_, sk| {
        let live = now.saturating_sub(sk.inserted_at) < SKIP_TTL_SECS;
        if !live { sk.key.zeroize(); }
        live
    });
}

impl Drop for Ratchet {
    fn drop(&mut self) {
        self.rk.zeroize();
        if let Some(ref mut c) = self.cks { c.zeroize(); }
        if let Some(ref mut c) = self.ckr { c.zeroize(); }
        self.dhs_sec.zeroize();
        for (_, mut sk) in self.skipped.drain() { sk.key.zeroize(); }
    }
}
```

Thread `now` into `Ratchet::open` / `skip` from `Node::on_rx` (already has `now`).

Regression tests already exist for count bounds; add:
- `skipped_keys_expire_after_ttl`
- `drop_zeroizes_secrets` (can use a canary pattern or mlock observation in debug)

---

## 24. Seal (`src/seal.rs`)

Module docs explicitly correct the S-022 history: seal functions are stateless; FS is Node's ring.

```rust
seal(msg, recip_prekey) -> eph_pub ‖ ct
open_sealed(sealed, prekey_sec) -> Option  // short buffer → None
chunk_seal/open: XChaCha20-Poly1305, nonce = index in last 4 bytes
prekey_keypair(): OsRng, not seed-derived
```

**Findings**
| ID | Sev | Finding | Proposed fix |
|----|-----|---------|--------------|
| C-S1 | Info | Docs no longer claim FS on the seal path itself | — |
| C-S2 | Info | open never panics — robustness + seal_open fuzz | — |
| C-S3 | Low | `chunk_seal` uses `.expect("chunk seal")` — should be unreachable if key/nonce valid; prefer `expect` only in debug or map to infallible encrypt | Prefer `.expect` is OK for AEAD encrypt with valid key; document |

---

## 25. Topic keys (`src/topic.rs`)

```rust
rotate(k) = SHA-256(k ‖ domain)           // FS, no healing
mix(k, c) = SHA-256(domain ‖ k ‖ c)       // healing
contribute / absorb: sealed boxes to member prekeys, ≤ MAX_MEMBERS
rekey_seal / rekey_open: membership change
key_id(k) = SHA-256(...)[..4]
```

Tests: absorb rejects malformed (empty, truncated, wrong count, corrupt box) without panic; contribute caps member count.

**Findings**
| ID | Sev | Finding | Proposed fix |
|----|-----|---------|--------------|
| C-T1 | Info | Healing via contribute/absorb is correctly designed (mix never replace) | — |
| C-T2 | Medium | Still no roster — absorb tries all boxes; membership is application problem | UI key_id divergence badge + optional advisory roster payload (Part II 15.2) |
| C-T3 | Info | absorb is attacker-input safe (fuzz + unit tests) | — |
| C-T4 | Low | Stolen prekey still decrypts contributions sealed to it until prekey expires (documented in Still open / S-020 residual) | Shorten prekey lifetime is a knob, not a protocol fix |

---

## 26. Prekey ring (`src/node/identity.rs`)

```rust
from_seed: bootstrap prekey = SHA-256(seed ‖ "spore/prekey/v1"), born=0 (never expires by clock)
rotate_prekey: mint random, stamp born=now
sweep_prekeys: delete secrets older than PREKEY_LIFETIME_SECS (7d), keep newest
prekey_ring / restore_prekey_ring: blob v1, n≤MAX, each entry 68 bytes (pub‖sec‖born BE)
restore: recomputes public from secret — rejects hostile blob that advertises wrong pub
open: tries ring newest-first
```

**Findings**
| ID | Sev | Finding | Proposed fix |
|----|-----|---------|--------------|
| C-P1 | Info | S-022 correctly implemented: random secrets, separate persistence | — |
| C-P2 | Info | restore validates public = derived(secret) | — |
| C-P3 | Low | Bootstrap prekey (born=0) never ages out until first rotate stamps it | Document; ensure first rotate happens on first sweep/announce cycle |
| C-P4 | Medium | Field verification of 7-day window still open (Still open register) | Device test: seal, wait 8 days (or mock clock), confirm unreadable |
| C-P5 | Info | open tries whole live ring — stale ANNOUNCE still works until sweep | — |

---

## 27. Audio demod (`src/bridge/audio.rs`) — S-031 confirmation

From findings register and prior analysis: `scanned` cursor; push cost flat ~1.5 ms independent of buffer fill. Tests for split frames and non-redo scan exist.

**Findings**
| ID | Sev | Finding | Proposed fix |
|----|-----|---------|--------------|
| C-A1 | Info | S-031 closed | — |
| C-A2 | Low | Pair with Android demod_out queue cap (A-J3) | Cap output queue |

---

## 28. Cross-cutting core observations

1. **now threading:** Almost every security-relevant path takes `now: u32`. Ratchet is the main exception for TTL — fix C-R1 by accepting `now`.
2. **Panic surface:** After S-001/S-005/S-018/S-019, remaining `.expect`/`.unwrap` appear limited to AEAD encrypt and internal invariants. Robustness harness + fuzz remain the right regression net.
3. **Zeroize culture:** `zeroize` is pinned for MSRV; actual use is incomplete (ratchet, possibly chain keys elsewhere). Systematic audit: every secret field should either live in a `Zeroizing<>` wrapper or have an explicit Drop.
4. **Freeze surface:** envelope layout, vectors, C ABI untouched by these internal improvements.

---

## 29. Updated priority list (Parts I–III merged)

### P0
1. Ratchet skipped-key TTL + Drop zeroize (C-R1, C-R2 / S-024a)
2. Device test matrix (Android migration, backup exclusion, 7-day FS, soak)
3. At least one hardware-verified radio path (Meshtastic or RNode)

### P1
4. `nativeUnregisterIface` + bridge stop UI
5. FileProvider + ACTION_VIEW
6. Service onDestroy → nativeFree
7. BLE write flow-control + reconnect
8. Ring health UI + export warning

### P2
9. Group key_id divergence UX + optional advisory roster
10. Housekeeping constants vs SPEC assertion
11. demod_out / AudioBridge stop hygiene
12. Release pipeline dry-run tests

### P3
13. Boot receiver option, Baud mascot, sound toggle, Impact font note already honest

---

## 30. Verdict after full core read

The core implementation matches the quality of the process documentation. S-001 through S-031 are real work with real tests. The remaining High items are **lifetime and memory-safety of secrets** (ratchet), not wire-format or trust-binding bugs. Android gaps are lifecycle and product-completeness, not crypto.

With C-R1/C-R2 closed and one radio path field-verified, SPORE's security story becomes internally consistent with its own SPEC and register.

---

*End of Part III. Document is ready for incremental Part IV (hardware results, remaining bridge/*.rs deep dives, or post-fix findings).*

---

# PART IV — STORE, HUB, CONGESTION, INGEST, BRIDGES

---

## 31. Key constants (from `src/lib.rs` / modules)

| Constant | Value | Spec alignment |
|----------|-------|----------------|
| `STAMP_QUOTA_BYPASS_BITS` | **16** | S-003: was any non-zero; now real PoW |
| `SEEN_MIN_SECS` | 30 days | SPEC seen-set floor |
| `PREKEY_LIFETIME_SECS` | 7 days | §7 / S-022 |
| `MAX_PREKEY_RING` | 16 | §7.2 |
| `HELLO_MIN_SECS` | 5 min | §5.4b Trickle min |
| `HELLO_MAX_SECS` | 80 min | §5.4b Trickle max |
| `ANNOUNCE_FLOOD_MIN_SECS` | 3600 (1 h) | §5.4b flood ceiling |
| `MAX_SKIP` | 512 | Ratchet per-gap |
| `MAX_SKIPPED_KEYS` | 2048 | Ratchet total (S-017) |
| Meshtastic `BULK_BYTES_PER_SEC` | 32 | Conservative LoRa default |
| Meshtastic MTU | 200 | Auto-fragment |

**[NEW] C-K1 Info:** Android housekeeping uses `ANNOUNCE_FLOOD_INTERVAL_MS = 3_600_000L` (1 h) — matches core. Confirm HELLO interval on Android matches 5→80 min Trickle (A-NC3).

---

## 32. Store (`src/store.rs`)

Write-through spill model:
- Metadata always in memory
- Bytes optionally on disk as `<hexid>.spore`
- `mem_budget` sheds resident copies (default 5 MB); total hold is unbounded by mem
- Missing spilled file → treat as not held (mesh can re-fetch)

```rust
enum Body { Mem(Vec<u8>), Evicted }
put → write disk if spill set, then shed if over mem_budget
wire(id) → Mem clone or fs::read
```

**Findings**
| ID | Sev | Finding | Proposed fix |
|----|-----|---------|--------------|
| C-ST1 | Info | Write-through + shed is the right model for Android 256 MB store / 8 MB mem | — |
| C-ST2 | Info | S-028 streaming materialize uses this correctly | — |
| C-ST3 | Low | `wire()` clones every time — fine for relay; hot path could return `Cow` | Optional optimisation |
| C-ST4 | Medium | Spilled file integrity: id is filename; no re-hash on read | On read, optionally verify SHA-256(wire)[..16]==id; reject mismatch (disk bit-rot / tamper) |
| C-ST5 | Low | `MAX_ADOPT_BYTES = 1 MB` for folder adopt — sensible | — |
| C-ST6 | Info | wasm32: no spill path (cfg) — correct | — |

**Proposed C-ST4 sketch:**
```rust
pub fn wire(&self, id: &Id) -> Option<Vec<u8>> {
    let bytes = match ... { ... };
    if envelope_id_of(&bytes) != *id { return None; } // or recompute
    Some(bytes)
}
```

---

## 33. Hub (`src/bridge/hub.rs`)

```rust
Shared = Arc<Hub>
Hub { node: Mutex<Node>, out: Mutex<Vec<Slot>>, deliver: Mutex<Option<Sender>> }
Slot { tx: Option<Sender<Forward>>, bulk: Option<Budget> }
Budget: leaky bucket, BURST_SECS=8, per_sec from register_limited
is_bulk = file chunks only (manifests/messages/announces free)
on_rx: lock node → on_rx → unlock → dispatch forwards (ordering load-bearing)
```

Poison recovery via `lock()` helper (S-018).

**Findings**
| ID | Sev | Finding | Proposed fix |
|----|-----|---------|--------------|
| C-H1 | Info | Bulk exemption for conversational traffic is correct design | — |
| C-H2 | Info | `with_node` lock ordering documented; node released before dispatch | — |
| C-H3 | Low | `now()` uses SystemTime — fails on devices with bad clocks; router already has “untrusted clock” path for expiry | Document; optional monotonic overlay for Trickle only |
| C-H4 | Medium | No unregister of Slot — slots only grow (pairs with A-N2) | `unregister(iface)` that drops Sender (RX end sees disconnect) |
| C-H5 | Info | Pull-only interfaces (HTTP bag) supported | — |

---

## 34. Congestion (`src/congestion.rs`)

Four SPEC §5.4 primitives, all present:

| Primitive | Implementation |
|-----------|----------------|
| (a) Token bucket ≤10% | `TokenBucket::ten_percent`, per-iface |
| (b) Trickle 5→80 min | `Trickle { min, max, cur, fire_at }` |
| (c) Backpressure busy byte | `admit(busy, stamp, roll)` |
| (d) FLOOD backoff 30s×2 cap 1h max 5 | `Backoff` |

Quotas: per-source token bucket, max 4096 sources, LRU-ish eviction by `last_active`, stamp≥16 bypasses.

**Findings**
| ID | Sev | Finding | Proposed fix |
|----|-----|---------|--------------|
| C-CG1 | Info | S-003 closed: bypass requires 16 bits not 1 | — |
| C-CG2 | Info | Source table capped + eviction — S-006 class | — |
| C-CG3 | Low | Eviction is min `last_active` full scan O(n) at 4096 — fine | — |
| C-CG4 | Info | Backpressure and quotas share the same stamp threshold — consistent | — |

---

## 35. Ingest / on_rx (`src/node/ingest.rs`)

```rust
build_announce hops=16  // mesh flood, ≤1/h
build_hello    hops=0   // link-local, Trickle 5→80 (S-023)
ingest:
  seen or expired → drop
  seen.insert(id, expiry.max(now + SEEN_MIN_SECS))  // note: uses expiry and floor
  enforce_bounds(now)
  verified_src = Full+SIGNED+verify() → path learn + quota charge
  deliver if dest matches
  forward if allow_forward && hops>0
```

Signature verified before path/quota binding (S-002/S-004).

`enforce_bounds` trims: frags by age, peer_prekeys/busy/names, manifests, acked, rpc/feed inboxes from front.

**Findings**
| ID | Sev | Finding | Proposed fix |
|----|-----|---------|--------------|
| C-I1 | Info | HELLO vs ANNOUNCE split correct post S-023 | — |
| C-I2 | Info | verify-before-trust on path+quota | — |
| C-I3 | Low | S-024b: `mark_seen` vs this `seen.insert(expiry.max(now+SEEN_MIN))` — ingest path is the real one; mark_seen floor may still be dead | Align or delete dead mark_seen logic |
| C-I4 | Info | Reassembled fragments `allow_forward=false` — prevents re-flood of giant object | — |
| C-I5 | Low | `enforce_bounds` frag eviction by `started` age — good; ensure `started` is set on first fragment | Verify in fountain integration |

---

## 36. Meshtastic bridge (`src/bridge/meshtastic.rs`)

Hand-rolled protobuf; portnum 256; BROADCAST 0xFFFFFFFF; hop limit 3; UDP 224.0.0.69:4403.

Varint parse uses `checked_add` (S-019 closed). Stream framer bounds asserted in fuzz.

**Findings**
| ID | Sev | Finding | Proposed fix |
|----|-----|---------|--------------|
| C-M1 | Info | S-019 closed with checked arithmetic in both loops | — |
| C-M2 | Info | Explicitly 🧪 template; unencrypted channel only by default | Hardware verification |
| C-M3 | Low | DEFAULT_HOP_LIMIT=3 is Meshtastic-side; SPORE hops separate | Document interaction |
| C-M4 | Info | BULK 32 B/s — conservative; operator can raise | — |

---

## 37. Other bridges (summary pass)

| Bridge | Notes | Residual |
|--------|-------|----------|
| `audio.rs` | S-031 cursor; shared with Android/desktop | Queue cap A-J3 |
| `udp.rs` / `tcp.rs` | Standard datagram/stream | — |
| `tor.rs` / `i2p.rs` | SOCKS/SAM | SOCKS reply length bounded (investigated) |
| `foldersync.rs` | S-027/S-028 fixed | — |
| `kiss_stream.rs` | Stateful framer; fuzzed | — |
| `neighbors.rs` | Snoop signed frames; verify before bind | — |
| `stream_link.rs` | Shared reconnect/backoff | Android BLE lacks this |
| `icmp.rs` | S-009 IP options fixed | Linux cap_net_raw |
| `ssb` / `spool` / `copyparty` / `bag` | Store/log shaped | — |
| `ax25` / `serial` / `reticulum` | KISS family | 🧪 |

**[NEW] C-B1 Medium:** Native `stream_link` has reconnect+backoff; Android BLE bridges do not (A-B6). Port the pattern or share logic.

---

## 38. DESIGN.md / HARDWARE.md / BRIDGES.md (doc cross-check)

From tree presence and prior digest:
- **DESIGN.md** explains relays-never-verify, stamp deployer note, group roster as app problem — consistent with code.
- **HARDWARE.md** procedure exists; no results filled (all 🧪).
- **BRIDGES.md** per-medium tables + security section; ANDROID/desktop bridges should stay listed in sync (optional docs-sync extension).

**[NEW] C-D1 Low:** Extend `check_docs_sync.py` to assert Android bridge list ⊆ BRIDGES.md entries (PLAN.md already suggests this).

---

## 39. Robustness & fuzz alignment

| Target | Covers |
|--------|--------|
| envelope_decode | C-E decode |
| node_on_rx | Full ingest |
| fragment_reassembly | Fountain + count=0 |
| armor_and_framing | Armor + KISS + ICMP |
| seal_open | Seal/topic open parsers |
| radio_codecs | Meshtastic + audio; bounded framer oracle |

Always-on `robustness.rs` found S-001. Fuzz found S-019 in <90s.

**[NEW] C-FZ1 Low:** Add fuzz target or robustness case for `topic::absorb` with high claimed count (already unit-tested; fuzz would stress alloc).

---

## 40. Part IV finding rollup

| ID | Sev | Summary |
|----|-----|---------|
| C-ST4 | Med | Spilled store read does not re-verify id=hash(bytes) |
| C-H4 | Med | Hub slots never unregister |
| C-B1 | Med | Android BLE lacks stream_link reconnect pattern |
| C-I3 | Low | mark_seen vs ingest seen floor |
| C-K1 | Info | Android flood interval matches; HELLO Trickle verify |
| C-D1 | Low | Bridge list docs sync |
| C-FZ1 | Low | topic absorb fuzz |

No new Critical/High beyond already-tracked C-R1/C-R2 (ratchet).

---

## 41. Full audit status

| Part | Coverage |
|------|----------|
| I | Architecture, security synthesis, high-level Android/docs/CI |
| II | Exhaustive Android line-by-line + UX + core fix sketches |
| III | envelope, fountain, ratchet, seal, topic, identity |
| IV | store, hub, congestion, ingest, meshtastic, bridge summary, constants |

**Still not line-by-line every bridge file** (tor/i2p/icmp/ssb/spool/…) — residual risk low given robustness+fuzz and prior S-nnn closures; escalate if hardware work targets a specific bridge.

---

## 42. Master recommendation (final for this pass)

**Ship blockers for “production phone node” narrative**
1. Device matrix (migration, backup, 7d FS, soak)
2. Ratchet TTL + zeroize
3. Bridge stop + FileProvider
4. One 🧪 → ✅ radio

**Everything else** is hardening and product polish on a foundation that is already unusually solid for an opportunistic mesh stack.

---

*End of Part IV. Part V can be: draft patches for C-R1/C-R2 + Hub unregister; or remaining bridge/*.rs; or HARDWARE.md dry-run checklist.*

---

# PART V — ALL BRIDGES, PATCHES, WEB/DOCS/UX

---

## 43. Bridge matrix (exhaustive)

Shape legend (SPEC Page 2): 1 message-pipe · 2 byte-stream/KISS · 3 text/armor · 4 shared bus · 5 shared store.

| Module | Shape | U (peer addr) | Bulk budget | Platform | Status | Notes |
|--------|-------|---------------|-------------|----------|--------|-------|
| `audio.rs` | 1 null | `()` | **0** | all + wasm | tested modem; 🧪 HW | S-031 fixed; ~23 B/s |
| `meshtastic.rs` | 1 | node u32 | 32 | native + wasm codec | 🧪 | S-019 fixed; port 256 |
| `reticulum.rs` | 1/2 | dest hash | (const) | native + companion | 🧪 | TCP/UDP/stdio |
| `udp.rs` | 1 | SocketAddr | unlimited default | native | solid | limited / primary / group |
| `tcp.rs` | 2 | stream | — | native | solid | `stream_link` reconnect |
| `ax25.rs` | 2 | — | (set) | native | 🧪 | KISS TNC TCP or serial |
| `serial.rs` | 2 | — | — | native | thin | opens path; no termios |
| `kiss_stream.rs` | 2 | — | — | all | solid | stateful; fuzzed |
| `stream_link.rs` | 2 | — | — | native | solid | reconnect + backoff |
| `tor.rs` | 2 | onion | — | native | reasoned | SOCKS5 CONNECT; length bounded |
| `i2p.rs` | 2 | b32 | — | native | reasoned | SAM v3 dial/accept |
| `bag.rs` | 5 | — | — | all | solid | Push/Inv/Want abstract |
| `store.rs` (bridge) | 5 | — | — | native | solid | folder `<hexid>.spore` |
| `foldersync.rs` | 5 | — | — | native | solid | S-027/S-028 fixed |
| `copyparty.rs` | 5 | — | — | native | reasoned | WebDAV-ish HTTP |
| `spool.rs` | 5 | — | — | native | reasoned | tx/rx dirs (NNCP/UUCP) |
| `ssb.rs` | 5 | — | — | all | reasoned | append-only log folder |
| `icmp.rs` | 1 | IP | — | linux | reasoned | S-009 options fixed |
| `csma.rs` | 4 | — | — | all | solid | listen-before-talk + CRC |
| `neighbors.rs` | — | generic U | — | all | solid | snoop signed; verify bind |
| `driver.rs` | — | — | — | native | solid | datagram runner |
| `hub.rs` | — | — | per-slot | native | solid | multi-iface; no unregister |
| `mod.rs` | — | — | — | — | — | feature gates wasm vs native |

### 43.1 Per-bridge findings

**audio.rs**
- BULK=0 correct at ~23 B/s.
- Portable modulate/demod; platform only moves PCM.
- Android uses same Demod via JNI (S-031).
- **C-AU1 Info:** run_pipe stdin/stdout design keeps zero audio crate dependency.

**ax25.rs**
- Thin wrapper over KISS stream + TCP or serial path.
- Several `unwrap` on internal channel ops — process-local, not wire.
- **C-AX1 Low:** Document that serial path requires external `stty`; no termios in-process by design.

**bag.rs / store (bridge) / foldersync / copyparty / spool / ssb**
- Shape 5 family: listing = receiving; write by hex id.
- foldersync: S-027 NUL name skip; S-028 streaming.
- **C-BG1 Low:** bag Want amplification bounded at Node (S-012), not at bag layer — correct layering.
- **C-BG2 Medium:** copyparty/HTTP paths: no TLS enforcement in bridge; operator URL choice. Document cleartext risk on non-LAN.

**csma.rs**
- Shared CRC SHA-256[0:4] + damped flood timers.
- **C-CS1 Info:** Used by shared-bus media without native CRC.

**driver.rs + neighbors.rs**
- Generic datagram loop: snoop → on_rx → resolve or broadcast.
- Neighbors verify signature before binding U (S-002 class).
- **C-NB1 Info:** Stale binding cost is flood-fallback — correct.

**stream_link.rs**
- Reconnect with exponential backoff; shared by TCP/Tor/I2P/AX25-TCP.
- **C-SL1 Medium:** Android BLE should mirror this pattern (A-B6 / C-B1).

**tor.rs**
- SOCKS5 to 9050; domain-type CONNECT for .onion; then KISS.
- Reply length from wire is u8-bounded (investigated, not a finding).
- **C-TO1 Low:** Hardcodes 9050; Tor Browser uses 9150 — allow config override (daemon YAML already may).

**i2p.rs**
- SAM v3; dial and accept paths.
- **C-I2P1 Low:** Same stream_link benefits; ensure SAM backlog/errors surface to hub status.

**icmp.rs**
- Echo payload carrier; IP header options handled (S-009).
- Linux `cap_net_raw` required.
- **C-IC1 Info:** Diagnostics networks are the intended deployment.

**udp.rs**
- Three modes: limited broadcast, primary-subnet directed broadcast, explicit group (v4/v6 overlays).
- **C-UD1 Info:** Primary-subnet discovery is the “just works on LAN” path; document failure when no default route / multiple interfaces.

**reticulum.rs**
- PLAIN spore/v1 destination; companion Python tool for full RNS stack.
- **C-RN1 Info:** Companion is separate process; version skew is operational risk.

**serial.rs**
- Opens path only; no baud configuration in-process.
- **C-SE1 Low:** Pair with ax25 docs on external stty.

**meshtastic.rs** — covered Part IV (S-019, 🧪, bulk 32).

**hub.rs** — covered Part IV (C-H4 unregister).

### 43.2 Android bridge parity gaps (UX + reliability)

| Native capability | Android | Gap |
|-------------------|---------|-----|
| stream_link reconnect | BLE no reconnect | A-B6 |
| register_limited bulk | Audio/Meshtastic/RNode use suggested budgets | OK |
| Hub unregister | No nativeStopBridge | A-N2 |
| Status to UI | BridgeState string | OK but append-only |
| Web transports | WebBridgeHost WebView | OK; battery unmeasured |

---

## 44. Ready-to-apply patches

### Patch A — Ratchet skipped-key TTL + zeroize (C-R1 / C-R2 / S-024a)

**File:** `src/ratchet.rs`

```rust
// 1. Change skipped entry type (conceptual diff)
// was: HashMap<( [u8;32], u16 ), [u8;32]>
// now:
struct SkippedKey {
    key: [u8; 32],
    inserted_at: u32,
}
// map: HashMap<( [u8;32], u16 ), SkippedKey>

const SKIP_TTL_SECS: u32 = 7 * 24 * 3600;

// 2. On insert in skip path:
self.skipped.insert(k, SkippedKey { key: mk, inserted_at: now });

// 3. purge called at start of open/skip:
fn purge_skipped(&mut self, now: u32) {
    self.skipped.retain(|_, sk| {
        if now.saturating_sub(sk.inserted_at) < SKIP_TTL_SECS {
            true
        } else {
            // zeroize::Zeroize
            sk.key.zeroize();
            false
        }
    });
}

// 4. Thread `now: u32` into Ratchet::open and skip from Node receive path.

// 5. Drop:
impl Drop for Ratchet {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.rk.zeroize();
        if let Some(ref mut c) = self.cks { c.zeroize(); }
        if let Some(ref mut c) = self.ckr { c.zeroize(); }
        self.dhs_sec.zeroize();
        for (_, mut sk) in self.skipped.drain() {
            sk.key.zeroize();
        }
    }
}

// 6. Tests:
// - skipped_keys_expire_after_ttl: insert, now += TTL+1, open fails, len==0
// - existing count-bound tests still pass
```

**Freeze impact:** None (internal).  
**API:** `open`/`skip` signatures gain `now` if not already present via session wrapper — check `session.rs` call sites.

### Patch B — Hub unregister (C-H4 / A-N2)

**Files:** `src/bridge/hub.rs`, `android/jni/src/lib.rs`, `SporeNative.kt`

```rust
// hub.rs
pub fn unregister(&self, iface: Iface) {
    let mut o = lock(&self.out);
    if let Some(slot) = o.get_mut(iface as usize) {
        slot.tx = None; // drop Sender; Receiver sees disconnect
        slot.bulk = None;
    }
}
```

```rust
// jni
Java_..._nativeUnregisterIface(_env, _class, ptr, iface) {
    if let Some(r) = rt(ptr) {
        r.ifaces.lock().unwrap().remove(&iface);
        r.hub.unregister(iface as Iface);
    }
}
```

```kotlin
// SporeNative.kt
external fun nativeUnregisterIface(ptr: Long, iface: Int)
// BleBridge.stop / AudioBridge.stop / WifiDirectBridge.stop: call unregister
// BridgesScreen: SwipeToDismiss or CrateSwitch → stop + unregister
```

### Patch C — Spilled store id verify (C-ST4)

```rust
// store.rs wire()
Body::Evicted => {
    let path = self.spill.as_ref()?.join(filename(id));
    let bytes = std::fs::read(path).ok()?;
    // id is SHA-256(envelope with hops=0)[..16] — recompute cheaply via Envelope::decode
    if let Ok((e, n)) = Envelope::decode(&bytes) {
        if n == bytes.len() && e.id() == *id {
            return Some(bytes);
        }
    }
    None
}
```

### Patch D — Android AudioBridge stop hygiene (A-A2)

```kotlin
fun stop() {
    rxJob?.cancel(); txJob?.cancel()
    rxJob = null; txJob = null
    try { record?.stop(); record?.release() } catch (_: Exception) {}
    try { track?.stop(); track?.release() } catch (_: Exception) {}
    record = null; track = null
}
```

### Patch E — BLE single drain + reconnect sketch (A-B2 / A-B6)

```kotlin
// MeshtasticBleBridge: replace per-notify launch with
private var drainJob: Job? = null
fun requestDrain(g: BluetoothGatt) {
    if (drainJob?.isActive == true) return
    drainJob = CoroutineScope(Dispatchers.IO).launch { /* existing drain loop */ }
}
// onConnectionStateChange DISCONNECTED → schedule reconnect with exp backoff cap 60s
```

### Patch F — FileProvider (A-NS3)

```xml
<!-- AndroidManifest -->
<provider
    android:name="androidx.core.content.FileProvider"
    android:authorities="${applicationId}.files"
    android:exported="false"
    android:grantUriPermissions="true">
    <meta-data android:name="android.support.FILE_PROVIDER_PATHS"
        android:resource="@xml/file_paths" />
</provider>
```

```xml
<!-- res/xml/file_paths.xml -->
<paths>
    <files-path name="store" path="store/" />
    <external-files-path name="ext" path="." />
</paths>
```

UI: on transfer complete, `CrateButton("Open")` → `FileProvider.getUriForFile` + `ACTION_VIEW`.

---

## 45. Web stack audit

| Asset | Role | Checks |
|-------|------|--------|
| `web/spore.mjs` | Core JS ABI over wasm | — |
| `web/build-standalone.mjs` | Single-file node | CI greps no live import/export, no remote src/href |
| `web/test.mjs` / `ws-test.mjs` / `codec-test.mjs` | E2E | CI |
| `transports/*.mjs` | WS, Nostr, WebTorrent, WebRTC, WebBluetooth, WebNFC, WebSerial, audio, kiss, meshtastic, reticulum, loopback | Mirror native codecs where applicable |
| `site/build.mjs` | Docs site | Fails on broken internal links/anchors |
| `site/style.css` | VISUALDESIGN tokens | ✅ per status table |
| `site/home.md` | Plain-language front | Voice zone correct |
| `site/seed/*` | Fountain seed sheet | Tests in CI |

**Findings**
| ID | Sev | Finding | Proposed fix |
|----|-----|---------|--------------|
| W-1 | Info | Standalone zero-request rule CI-enforced — continuity load-bearing | Keep |
| W-2 | Info | Wasm single import `spore_fill_random` CI-enforced | Keep |
| W-3 | Low | WebBridgeHost on Android loads transports with imports stripped — same rule as standalone | Ensure build copies assets into APK; CI already builds APK |
| W-4 | Medium | WebView battery/JS soak unmeasured (ANDROID_AUDIT) | 12 h Historian run |
| W-5 | Low | WebNFC / WebBluetooth 🧪 — need device | Hardware.md |
| W-6 | Info | site style + home.md honour design zones | — |

---

## 46. Docs audit (remaining)

| Doc | Quality | Gaps |
|-----|---------|------|
| SPEC.md | Excellent | — |
| VISUALDESIGN.md | Excellent + status table | — |
| SECURITY_FINDINGS.md | Exemplary | Still open accurate |
| DESIGN.md | Strong | Roster honesty |
| BRIDGES.md | Large reference | Keep 🧪 honest; sync Android list (C-D1) |
| HARDWARE.md | Procedure only | **No results** — primary product gap |
| CONTINUITY.md | CI-proven offline | — |
| REBUILD.md | Worked examples | Vectors synced |
| APPS.md | Download links | Depends on release pipeline health |
| ANDROID_AUDIT.md | Excellent honesty | Open items match this audit |
| SECURITY.md | Present | Point to findings register |
| README.md | Strong implementer entry | — |

**Findings**
| ID | Sev | Finding | Proposed fix |
|----|-----|---------|--------------|
| D-1 | Medium | HARDWARE.md empty of results while README/APPS advertise radio bridges | Fill after first field test or demote marketing language |
| D-2 | Low | Bridge list Android ↔ BRIDGES.md sync not automated | Extend check_docs_sync.py |
| D-3 | Info | CHANGELOG honesty about process failures is a cultural asset | Preserve |

---

## 47. Android UX deep remainder

### Markdown.kt
- Inline only: `**bold**`, `*italic*`/`_italic_`, `` `code` ``, `[text](url)`.
- Unmatched delimiters left literal — correct for mesh posts.
- **UX-MD1 Info:** No images in markdown parser itself; Feed uses separate magnet marker — layered correctly.

### Petnames.kt
- Local map address → label; PUBLIC sentinel.
- **UX-PN1 Info:** Announced names shown quoted as claims — correct trust UX.

### Qr.kt
- Invite encode/decode surface.
- **UX-QR1 Info:** Camera optional; paste works.

### Chrome.kt / screens
- Covered Part II; compliance high.
- **UX-1 Medium:** Ring health not shown (count, oldest, next expiry).
- **UX-2 Medium:** No encrypted export flow with FS warning.
- **UX-3 Low:** No Baud empty states.
- **UX-4 Low:** No sound toggle (clack/particles).
- **UX-5 Low:** contentDescription on LED/badge meaning (“sealed”, “bad sig”, “3/8 chunks”).
- **UX-6 Info:** Pink primary actions on void (not olive) — compliant.

### WebBridgeHost.kt
- Hidden WebView, https base for secure context, JS interface push/poll.
- Queues JS until page ready.
- **UX-WB1 Medium:** No teardown of WebView on bridge stop (ties to A-N2).
- **UX-WB2 Low:** `spore.invalid` base URL — fine for secure context; document.

---

## 48. Master finding index (all parts)

### High
- C-R1/C-R2 ratchet TTL + zeroize (S-024a)
- Process: never device-tested Android path

### Medium
- A-N2/C-H4 no bridge unregister
- A-NS3 no FileProvider
- A-S1 service no nativeFree
- A-B1/B2/B6 BLE write/drain/reconnect
- A-W1 Wi-Fi Direct group confirm
- A-X1 device-transfer exclusion untested
- C-ST4 spilled store no re-verify
- C-B1/C-SL1 BLE lacks stream_link pattern
- W-4 WebView battery unmeasured
- D-1 HARDWARE results empty
- UX-1/2 ring health + export UX

### Low / Info
- Many documentation and polish items (A-M*, C-*, W-*, UX-*, D-*)

---

## 49. Suggested commit series

1. `fix(ratchet): age-bound skipped keys and zeroize on drop` (Patch A)
2. `feat(hub): unregister iface; android JNI + UI stop` (Patch B)
3. `fix(store): verify spilled envelope id on read` (Patch C)
4. `fix(android): AudioBridge stop null-out; BLE drain coalescing` (D+E partial)
5. `feat(android): FileProvider open for received files` (F)
6. `docs(hardware): first field-test results` (when available)
7. `test(android): migration + backup exclusion checklist in ANDROID_AUDIT`

---

*End of Part V. The mega-audit now covers architecture, Android (incl. UX), core crypto, store/hub/congestion/ingest, all bridges, web/site, docs, and ready-to-apply patches.*

---

# PART VI — SESSION, MIX, RPC, FFI, SITE/UX CLOSE-OUT

---

## 50. Session layer (`src/session.rs`)

UDP-like datagram session + lightweight reliable stream inside sealed payloads.

```rust
TAG_DGRAM = 0x04
Session { me, peer, port, peer_prekey, tx_seq, rx_hi, rx_win }
accept_rx: 64-wide sliding replay window (DTLS-style)
Stream frames: F_DATA [offset:8][len:2][bytes], F_ACK [recv_next:8]
```

**Findings**
| ID | Sev | Finding | Proposed fix |
|----|-----|---------|--------------|
| C-SE1 | Info | Replay window 64 is fixed; fine for interactive; document loss beyond window | — |
| C-SE2 | Low | Reliable stream is Go-Back-N style (from README) — endpoint-only reliability, correct for SPORE philosophy | — |
| C-SE3 | Medium | Session uses peer_prekey (one-shot seal), not necessarily Double Ratchet — confirm call sites choose ratchet for long-lived chat | Document when UI uses seal vs ratchet; prefer ratchet for DM threads |
| C-SE4 | Low | No explicit session timeout / zeroize of peer_prekey copy | Drop/zeroize peer_prekey when session ends |

**Android UX link:** Chat `sendDirect` uses sealed+ACKREQ path; long conversations should migrate to ratchet sessions if not already (verify NodeController path).

---

## 51. Mix mode (`src/mix.rs`)

Onion nested sealed envelopes; size-class padding 256/1024/4096; Poisson delay at mix.

**Findings**
| ID | Sev | Finding | Proposed fix |
|----|-----|---------|--------------|
| C-MX1 | Info | SPEC §9 scope honest (local observers / subset of mixes; global passive needs decoys) | — |
| C-MX2 | Low | Decoy generation is SHOULD in SPEC; confirm reference node emits decoys when mix-enabled | If not, document as open or implement low-rate decoys |
| C-MX3 | Info | Unreviewed in project “Still open” for deeper state machines — still accurate | Optional dedicated review pass |

---

## 52. RPC (`src/rpc.rs`)

HTTP-shaped method/path/body/status; reply along path request taught.

**Findings**
| ID | Sev | Finding | Proposed fix |
|----|-----|---------|--------------|
| C-RP1 | Info | Inbox bounded in enforce_bounds (MAX_INBOX) | — |
| C-RP2 | Low | No auth beyond envelope crypto — correct for mesh; app must not expose sensitive RPC without sealing | Document |

---

## 53. FFI (`src/ffi.rs`) + bindings

C ABI frozen (`bindings/spore.h`); Python/Go/JS generated from `spec.json`.

S-005: extern functions panic-guarded across C ABI.

**Findings**
| ID | Sev | Finding | Proposed fix |
|----|-----|---------|--------------|
| C-FI1 | Info | Freeze guard + bindings regen CI — solid | — |
| C-FI2 | Info | Android JNI is *separate* from frozen C ABI — correct, additive | — |
| C-FI3 | Low | Ensure ffi and jni never diverge on semantic meaning of seed/prekey_ring | Shared docs already |

---

## 54. Robustness (`src/robustness.rs`)

Always-on malformed-input harness; found S-001 on first run. Complements fuzz.

**Findings**
| ID | Sev | Finding | Proposed fix |
|----|-----|---------|--------------|
| C-RB1 | Info | Keep expanding when new parsers land | topic absorb already unit-tested |
| C-RB2 | Low | Run under same -D warnings as CI | Already |

---

## 55. Site CSS / home UX compliance

`site/style.css` implements VISUALDESIGN tokens exactly:
- Raw palette + semantic roles
- Measured contrast comments
- **NEVER on kevlar** called out for pink
- Field Notes light mode with re-checked ratios
- System mono + Impact stack (no webfont)
- Sticky header, crates, etc.

`site/home.md`: plain-language zone (not kawaii jargon) — correct per §2 tone table.

**Findings**
| ID | Sev | Finding | Proposed fix |
|----|-----|---------|--------------|
| W-CSS1 | Info | CSS is the third consumer alongside Android Chrome and standalone | Keep single source discipline |
| W-HOME1 | Info | Front page voice discipline held | Preserve in review |

---

## 56. Android UX — session/crypto messaging honesty

Recommended UI copy (Advanced / About):

> **Forward secrecy:** One-shot seals use rotating prekeys deleted after 7 days. Session ratchets advance per message; skipped keys are count-limited (age limit pending). A backup of the prekey ring defeats the 7-day window.

Ring health row:
```
Prekeys: 4 live · oldest 2d · next mint ~22h
[Export ring…]  // warns: export defeats FS
```

Chat: if `nativeEnvEncrypted` and not ratchet, show “🔒 sealed”; if ratchet, “🔒 session” — optional distinction.

---

## 57. Final consolidated verdict

### What SPORE does exceptionally well
- Frozen v1 wire + mechanical freeze guards
- Living security register with reproduction and tests
- Congestion + quota + stamp PoW coherent after S-003
- Prekey ring actually random and time-bounded (S-022)
- Fountain/store/hub bounds after early DoS class closure
- Android visual language faithfully implemented
- Offline continuity proven in CI
- Honest 🧪 and “Still open” culture

### What still blocks a “production phone node” claim
1. **Ratchet skipped-key age + zeroize** (SPEC overclaim)
2. **No device validation** of backup exclusion, migration, 7-day FS, battery
3. **Bridge lifecycle** (stop/unregister) and **FileProvider**
4. **Zero hardware-verified radio paths**

### Risk summary
| Area | Residual risk |
|------|----------------|
| Wire format / T0–T2 routing | Low |
| One-shot seal FS | Low (if ring persisted + swept) |
| Ratchet FS lifetime | **High until Patch A** |
| Remote DoS class | Low (closed findings) |
| Android identity storage | Low code / **Medium ops** (untested) |
| Radio bridges | **High operational** (unverified) |
| Release pipeline | Medium (improved, still untested dry-run) |
| Group membership | Medium product (by design) |

### Suggested next 2 weeks
| Day | Work |
|-----|------|
| 1–2 | Patch A (ratchet) + tests |
| 3 | Patch B (unregister) + Android stop UI |
| 4 | Patch C + D + F (store verify, AudioBridge, FileProvider) |
| 5–7 | Device matrix: install, upgrade, reveal seed, adb backup attempt, 24 h soak |
| 8–10 | One Meshtastic or RNode field test → update HARDWARE.md + 🧪 |
| 11–14 | BLE reconnect, ring health UX, release dry-run notes |

---

## 58. Audit metadata

| Item | Value |
|------|-------|
| Document | `SPORE_DEEP_AUDIT.md` |
| Parts | I–VI |
| Sources | Full repo dump + prior pack digest |
| Method | Static line-level review; no device execution |
| High findings open | C-R1, C-R2, device-test process gap |
| Patches drafted | A–F (§44) |

**This audit is complete for static analysis.** Remaining value is execution: apply patches, run devices, fill HARDWARE.md.

---

*End of Part VI — mega-audit closed for this pass.*

---

# PART VII — ANDROID UX RESEARCH: ANNOYANCES, ROOT CAUSES, REFINED FIXES

*Consolidates user-reported release APK pain points, code-backed root causes from Parts I–VI, critique of the proposed `ux/android/attachments-profiles` plan, and a concrete implementation order. Written for maintainers and for `android/UX-ISSUES.md` to be derived from.*

---

## 59. Problem statement (user-reported)

From production / release APK use:

1. **Bridges** — cannot disable, enable, delete, or edit added bridges.
2. **Attachments** — cannot open attached files; no preview when MIME is recognized (e.g. photos).
3. **Attach UX** — choosing a file should **stage** it on the message, not send/publish before the Send button; after send it must appear as **part of that message** for both sender and receiver, with preview when supported.
4. **Profile** — support a profile image that peers can request and cache; ability to change it and notify others; recommended petname should be clearly advertised / visible as “what others see.”

These match gaps already flagged in this audit (A-N2, A-NS3, A-S1-related lifecycle, Feed-vs-chat preview asymmetry, ANNOUNCE petname without avatar).

---

## 60. Current behaviour (code-backed)

| Area | What the code does today | Primary files | Audit IDs |
|------|--------------------------|---------------|-----------|
| Bridges | Append-only `BridgeState` list; LED + status word; no stop/edit/delete | `NodeScreens.kt` (`BridgeRow`), `NodeController.kt`, no `nativeUnregisterIface` | A-N2, C-H4, ANDROID_AUDIT §2 |
| Chat attach | `GetContent` → immediate `NodeController.sendFile` | `ChatScreens.kt` `ChatDetail` picker | — |
| Chat bubble | Text + `FragmentStatus` (segmented LED); `Msg.magnet` optional; no thumbnail; no Open | `ChatScreens.kt` `Bubble` / `FragmentStatus` | A-NS3 |
| Feed images | Markdown + `spore:` magnet; `inSampleSize` decode on IO | `FeedScreens.kt`, `Markdown.kt` | A-F2 |
| File open | No `FileProvider`; no `ACTION_VIEW` | Manifest, missing `file_paths.xml` | A-NS3, Patch F |
| Own name | `myName` / ANNOUNCE petname bytes; Nearby shows announced vs petname | `NodeController`, `ChatScreens` Nearby | — |
| Avatar | None | — | — |

**Design honesty already present (preserve):**

- Bridge switch omitted because stop did not exist (better than a dead control).
- File status *“served from this node”* vs *“fetching”* — do not replace with fake “delivered.”
- Invite bridges opt-in only (unauthenticated QR must not auto-join).
- Side + border for mine/theirs; never pink-on-olive; segmented LED for chunks.

---

## 61. Deep dive — Bridges

### 61.1 User expectation
A bridge is a connection profile: on/off, edit parameters (URL, device), remove. Similar to Wi-Fi saved networks or VPN profiles.

### 61.2 Why the UI cannot lie
`Hub` slots and JNI iface map only grow. Without `Hub::unregister` + `nativeUnregisterIface`, a toggle cannot stop the native pump or free the slot (Part IV C-H4, Part V Patch B). Shipping a working-looking switch without that is the same failure mode as “Save did nothing” (CHANGELOG 0.5.0).

### 61.3 Target UX
- **BridgeRow:** kind, detail, LED + status **word** (colour never alone).
- Overflow / actions: **Start**, **Stop**, **Edit**, **Remove**.
- **Edit** for URL-like bridges (ws, nostr, tcp): implement as remove + re-add with new value until native supports in-place mutate.
- **Unsupported stop:** action disabled or absent, with caption *“Stop requires a core update”* — not a grey switch.

### 61.4 Required engineering
```text
Hub::unregister(iface)           // drop Sender; RX end sees close
nativeUnregisterIface(ptr, iface)
Kotlin: cancel pump Job; remove BridgeState; call unregister
```
BLE/audio/Wi-Fi Direct: `stop()` already exists on bridge classes; wire to unregister + list removal.

### 61.5 Priority
**P0** for “real node” feeling; can ship after or in parallel with a tiny JNI PR.

---

## 62. Deep dive — Attachments (stage, one bubble, preview, open)

### 62.1 User expectation (industry baseline)
1. Pick file → appears in **composer** (chip / thumbnail), not in the thread.  
2. Optional text.  
3. **Send** → single bubble: text + attachment, identical structure for sender and receiver.  
4. Progress on that bubble while chunks incomplete.  
5. Inline preview when MIME supported and bytes available; otherwise Open/Share/Save.

### 62.2 Why chat feels broken vs Feed
Feed already publishes image bytes as a file and references them from markdown with a magnet marker, then decodes for display. Chat’s picker **publishes immediately** and never stages; bubbles never run the Feed-style preview path; nothing implements FileProvider Open.

### 62.3 Staging model (composer)
```text
Composer state (per peer):
  text: String
  staged: Attachment?  // name, bytes, mime, optional local preview

Picker → set staged (do not call publish/send)
UI: thumbnail or filename chip + Remove (X)
Send:
  if staged != null:
    magnet = publish (sealed to peer when key known — same as today’s sendFile trust path)
    body = text + attachmentMarker(name, magnet, mime)
  else:
    body = text
  sendDirect / send
  clear staged
```

**v1 scope:** one staged attachment. Multi-attach later.

**Navigation:** keep staged state per peer when switching threads; clear on successful send.

### 62.4 One message for both sides
Encode a **stable, parseable marker** in the message body (prefer aligning with Feed):

```text
# Option A — markdown-adjacent (Feed-like)
📎 name
![name](spore:<magnetHex>)

# Option B — explicit machine line (easier to parse, still human-readable)
📎 name | spore:<magnetHex> | image/jpeg
```

- Old clients: see readable text + magnet.  
- New clients: parse → chip + preview + Open.  
- Avoid two independent `Msg` rows (text-only + orphan file) as the default UX.

### 62.5 Bubble layout (sender = receiver)
```text
┌─ crate (pink edge if mine, edge if theirs) ─────────┐
│ optional caption / peer label                        │
│ message text                                         │
│ ┌ attachment chip / image preview (max width) ─────┐ │
│ │ thumbnail OR filename · size · MIME              │ │
│ └──────────────────────────────────────────────────┘ │
│ SegmentedLed + “fetching a/b” or “served…” if magnet │
│ time · 🔒 · ✓ delivered / sig badges                 │
└──────────────────────────────────────────────────────┘
```

Tap preview/chip → fullscreen **AttachmentViewer** (Open / Share / Save).

### 62.6 Preview rules
| MIME | Inline | Fullscreen / Open |
|------|--------|-------------------|
| image/* | Yes, `inSampleSize` from bubble max width (~280dp), `Dispatchers.IO` | Yes |
| video/*, audio/* | Chip only (v1) | Open (player app); ExoPlayer later |
| application/pdf, other | Chip | Open via FileProvider |

**Do not** show a full decoded image until enough data exists to decode; incomplete transfer → LED + placeholder (dim crate or filename chip only).

**Reuse** Feed’s decode discipline; do not fork a second ad-hoc decoder.

### 62.7 FileProvider + cache
- **Preview/open cache:** `context.cacheDir/attachments/<magnetPrefix>/` — eligible for system reclaim.  
- **Explicit Save:** `externalFilesDir` or MediaStore downloads.  
- Manifest `FileProvider` + `res/xml/file_paths.xml` (Patch F).  
- Eviction: e.g. max ~50 MB or age ~14 days on cache dir.  
- Grant read URI permission for `ACTION_VIEW`; no world-readable paths.

### 62.8 Crypto / honesty
- Staged send must preserve **sealed publish when peer has key** (same as current `sendFile`).  
- Cache of plaintext open files is equivalent to “user opened the attachment”; do not write sealed ciphertext to a public dir.  
- Keep **“served from this node”** vs **“fetching”**; never imply peer downloaded chunks you cannot observe.

### 62.9 Priority
**P0** — highest daily annoyance.

---

## 63. Deep dive — Profile image + recommended petname

### 63.1 Petname / “name others see”
ANNOUNCE already carries petname bytes; Nearby already prefers local petname, else quoted announced name, else address label.

**UX gap:** Advanced/Connect does not strongly frame the field as public-facing.

**Fix (Android-only):**
- Label: **“Name others see”** (not only “petname”).  
- Live preview: *“Nearby will show: …”* using the same rules as `ChatsList`.  
- Confirm on save (already added in 0.5.0).

### 63.2 Avatar — two layers

| Layer | Behaviour | Wire change? |
|-------|-----------|--------------|
| **Local** | Pick image → `nativePublishFile` → store magnet in EncryptedSharedPreferences → show on own profile / Connect / optional chat header | No |
| **Mesh** | ANNOUNCE carries optional avatar magnet; peers map `addr → magnet`, fetch, cache, show in list avatar slot | Yes (small, follow-up) |

**Do not** overload `nativeSetName` to mean avatar.

**Constraints:**
- Avatar is presentation, not identity (key is identity) — fits “sticker on crate” visual language.  
- Hard size cap on published avatar (e.g. 64–128 KB) so HELLO/ANNOUNCE-driven fetches do not saturate audio/LoRa bulk budgets (`BULK_BYTES_PER_SEC` 0 on audio, 32 on Meshtastic).  
- Change avatar → new magnet → next announce; peers replace cache when magnet changes.  
- Initial letter avatar remains fallback when no image.

### 63.3 Priority
Local avatar + name copy: **P1**. Mesh advertise: **P1 follow-up core PR**.

---

## 64. Critique of proposed branch plan (`ux/android/attachments-profiles`)

### 64.1 What is right
- Stage until Send.  
- Inline preview + fullscreen viewer.  
- FileProvider Open/Save.  
- BridgeRow with explicit actions.  
- Split core avatar announce from Android-only work.  
- Docs + manual QA checklist first.  
- Micro-commits for review.

### 64.2 What to change
| Proposal | Adjustment |
|----------|------------|
| Bridge toggle before JNI stop | **Block** real toggle until `nativeUnregisterIface`; else honest disabled state |
| Avatar via `nativeSetName` | **Reject** — separate magnet field / local prefs |
| Single mega-PR for all themes | Prefer **3 PRs**: attachments · bridges · profile |
| Cache in `externalFilesDir` only | Prefer **`cacheDir` for previews**; external/MediaStore for user Save |
| Two independent messages (text + file) | Prefer **one body with marker** so sender/receiver share one bubble |
| Preview while chunks incomplete | Placeholder + LED until decodable |
| Debug `Log.d` in release | Gate with `BuildConfig.DEBUG` |

### 64.3 Missing items to add
1. Explicit **attachment marker** format (Feed-aligned).  
2. JNI **unregister** as dependency of bridge Stop.  
3. VISUALDESIGN compliance for chips/previews (crate, no pink-on-olive, LED).  
4. Sealed-file path preserved on staged send.  
5. Per-peer composer staged state.  
6. Single-attachment v1 scope.  
7. Feed Open parity for non-inline types.  
8. Accessibility: `contentDescription` on chips/LED/badges.

---

## 65. Recommended PR split

### PR1 — Chat attachments (ship first)
1. `Attachment` model + composer staging + remove  
2. Send: publish then send body with marker  
3. Bubble: parse marker, chip/preview, progress  
4. FileProvider + AttachmentViewer (Open/Share/Save)  
5. Cache + eviction  
6. `android/UX-ISSUES.md` + `TESTING.md` sections for attachments  

### PR2 — Bridges UI
1. Core/JNI: `Hub::unregister` + `nativeUnregisterIface` (tiny PR if separate)  
2. BridgeRow Start/Stop/Edit/Remove  
3. Honest unsupported messaging  

### PR3 — Profile
1. “Name others see” copy + preview  
2. Local avatar pick/publish/cache/display  
3. Follow-up: ANNOUNCE avatar magnet + peer fetch/cache  

**Branch naming:** `ux/android/attachments-profiles` is fine as an umbrella; or `ux/android/attachments`, `ux/android/bridges`, `ux/android/profile`.

---

## 66. Acceptance criteria

### Attachments
- [ ] Pick file → composer shows chip/thumbnail; thread unchanged  
- [ ] Remove staged → Send is text-only  
- [ ] Send → **one** bubble with text + attachment for sender  
- [ ] Receiver: same structure; LED while fetching; preview when decodable  
- [ ] Open via FileProvider for images (and PDF if straightforward)  
- [ ] Sealed peer files remain sealed until open; no cleartext public dir  
- [ ] Status language remains honest (served vs fetching)  

### Bridges
- [ ] Remove drops UI row and stops pumps / unregisters iface  
- [ ] Start/Stop round-trip for at least one Kotlin-driven and one URL bridge  
- [ ] Unsupported Stop does not look like a live switch  

### Profile
- [ ] “Name others see” matches Nearby rules  
- [ ] Set/change local avatar; visible on own surfaces  
- [ ] (Follow-up) Peer shows avatar after announce + fetch  

### Visual / a11y
- [ ] No pink-on-olive; focus visible; LED + words for status  
- [ ] Meaningful `contentDescription` on attachment chips and badges  

---

## 67. Marker format (proposal for PR1)

**Canonical line (machine-parseable, human-readable):**

```text
📎 <filename> | spore:<64-hex-magnet> | <mime>
```

- Placed at end of message body after user text (blank line separator if text non-empty).  
- Parser: last line matching `^📎 .+ \| spore:[0-9a-f]+ \| \S+`  
- Feed may keep `![name](spore:…)` for markdown posts; chat uses the 📎 line for reliability.  
- Document in `android/UX-ISSUES.md` and optionally DESIGN notes as **application convention**, not a wire-format break (payload is still opaque UTF-8 to relays).

---

## 68. Mapping to existing audit patches

| UX work | Existing patch / ID |
|---------|---------------------|
| Bridge stop/remove | Patch B, A-N2, C-H4 |
| FileProvider Open | Patch F, A-NS3 |
| AudioBridge stop hygiene | Patch D (related lifecycle) |
| BLE lifecycle | Patch E (reconnect; separate from list remove) |
| Store integrity on disk open | Patch C (when saving opened files long-term) |

Ratchet TTL (Patch A) remains security P0 and is **orthogonal** to this UX track; do not block attachments UI on it.

---

## 69. Non-goals (v1 UX track)

- Multi-file attach in one send  
- In-app video player / ExoPlayer  
- Full mesh avatar without ANNOUNCE extension  
- Editing message history after send  
- Read receipts beyond existing ACKREQ delivered badge  
- Bridge in-place mutate without remove+re-add  

---

## 70. Suggested first commit body (`android/UX-ISSUES.md`)

Document should include:

1. Problem statements (§59)  
2. Current behaviour table (§60)  
3. Target UX per area (§61–63)  
4. Marker format (§67)  
5. Acceptance criteria (§66)  
6. Non-goals (§69)  
7. PR split (§65)  
8. Pointers to JNI unregister requirement and “no fake toggles”  

This Part VII is the narrative source of truth for that file.

---

## 71. Summary judgment

| Item | Severity as daily annoyance | Fix complexity | Depends on core? |
|------|----------------------------|----------------|------------------|
| Stage + one-bubble attach + preview + Open | **Critical UX** | Medium | No (JNI file APIs exist) |
| Bridge edit/stop/delete | **High UX** | Medium | **Yes** — unregister |
| Name others see | Low | Low | No |
| Local avatar | Medium | Low–Medium | No |
| Mesh avatar advertise | Medium | Medium | **Yes** — ANNOUNCE field |

**Proceed with PR1 (attachments) immediately.**  
**PR2 after or with tiny unregister JNI.**  
**PR3 local profile without waiting on mesh avatar.**

The Copilot plan is a solid scaffold; this Part VII tightens honesty (no fake switches), message unity (marker), cache placement, crypto continuity, and visual-language compliance so the APK stops fighting its own design system and protocol.

---

*End of Part VII. Mega-audit Parts I–VII form the full static + UX research record for `sloev/spore` as of this pass.*
