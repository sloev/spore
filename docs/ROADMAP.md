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
| **PR0** | Ratchet TTL + zeroize **and** offline crypto lifetime knobs | **P0 security + honesty** | 🟡 **partial** — Part A merged (#40); Part B carried forward | — | PR1, PR5 |
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
| **B3** | Empty states + PUBLIC/broadcast confirm | High UX | 🟢 in review (🧪) | — | B-series |
| **B4** | Notifications + transfers overflow | High UX | ⬜ todo | — | B-series |
| **B5** | Advanced: ring health + cautious export | Medium | ⬜ todo | — | B-series |
| **B6** | Bridges: status enum + permission recovery | High UX | ⬜ todo | — | B-series |
| **B7** | Accessibility + density pass | High UX | ⬜ todo | B1–B6 | C1 |
| **B8** | Feed polish | Medium | ⬜ todo | — | B-series |
| **C1** | Token parity + forbidden-pair audit | High UX | ⬜ todo | — | B7 |
| **Site** | Readability + less generic + more fun UI (à la gitingest.com) | Medium UX | ⬜ todo | — | C1 |

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

Only **Part A** (the crypto: age-bound + zeroize on drop) shipped. **Part B —
offline-lifetime knobs and UI — did not**, and is the larger remaining piece:

- [ ] **Daemon config knobs** `prekey_lifetime_secs` / `ratchet_skip_ttl_secs`
      (default `604800`), read from **one policy object**, not scattered consts.
- [ ] **Android Advanced presets** — 7 d (default) / 14 d / 30 d / custom, persisted
      via the single secret-accessor pattern.
- [ ] **About / security blurb** stating the active window in plain language.
- [ ] **Decrypt-failure UI** — "key expired for offline window; ask resend or raise
      it," shown on a failed open of expired sealed mail.
- [ ] **Raise-above-default warning** — a longer window means a stolen device reads
      more history.
- **Why deferred:** the Double Ratchet is a tested primitive **not yet wired into
      any production send/receive path** (`decrypt` is called only from tests), so a
      runtime TTL knob would configure dead code, and the Android sliders can't be
      device-verified here — shipping them would break Principle #2 (no fake UI).
- **Unblocked by:** wiring the ratchet into the DM/session path. Until then the
      code-level anti-drift move is done (`SKIP_TTL_SECS = PREKEY_LIFETIME_SECS`, one
      shared window). **Suggest a dedicated PR0b** once the ratchet is integrated.
- [ ] **Field-verify the 7-day window end to end** on a device — already tracked in
      **PR6**; unit tests prove the deadline logic, not a real node's clock/delivery.

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

### From PR2 — hub unregister + bridge stop/remove (in review #42, `feat/hub-unregister-bridges`)

Shipped: `Hub::unregister`, `nativeUnregisterIface`, Remove for Audio/BLE/Wi-Fi
Direct/Web. Carried forward:

- [ ] **Edit a bridge in place** (change a URL/params) — today it is Remove +
      re-add; needs a native mutate or re-register helper.
- [ ] **Enable/disable *toggle*** distinct from Remove (keep the row, stop/restart
      the transport) — depends on **PR3**'s reconnect/backoff so a re-enable has a
      clean start path.
- [ ] **Stop/remove for core-owned TCP/UDP** — they show no control today (honest,
      not a dead button); a clean core-side stop for a specific TCP/UDP iface would
      let them be removable too.
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

### Extra acceptance (Part B)
- [ ] Default remains ~7 days for prekey + ratchet skip TTL
- [ ] User/daemon can raise lifetime; value survives restart
- [ ] Warning shown when raising above default
- [ ] About/Advanced states the active window in plain language
- [ ] Failed open of expired sealed mail shows actionable message

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

## B1 — Chat navigation: BackHandler + scroll-to-latest + IME insets — 🟢 in review (#58)
**Why:** single `var screen` stack, so system Back leaves the app from a nested screen;
the chat list doesn't stay on the latest message; the keyboard covers the composer.
**Files:** `MainActivity.kt`, `ChatScreens.kt`, maybe `Chrome.kt`.
**Steps:** hierarchical Back (Chat→Chats, Compose→Feed, else→Chats; only Chats
backgrounds); `rememberLazyListState` + `LaunchedEffect(thread.size) { scrollToItem }`;
`imePadding()` on the composer; keep 48 dp targets.
**Non-goals:** full Navigation-Compose rewrite. **Acceptance:** Back never exits from a
nested chat; new message visible; composer usable with the keyboard open.

## B2 — Send/error feedback — ⬜ todo
**Why:** the plain-text `send` path clears the composer even when `ptr==0` or the native
call fails — silent data loss.
**Files:** `ChatScreens.kt`, `NodeController.kt`, the Feed compose send path.
**Steps:** send helpers return `Boolean`/result; snackbar via `LocalSnackbar` on failure;
clear the composer only on success; mirror the existing "Node not started" path from
`setMyName`. **Acceptance:** a failed send keeps the text and says why.

## B3 — Empty states + PUBLIC/broadcast confirm — ⬜ todo
**Why:** empty chat/feed/bridge lists give no guidance; PUBLIC is a mis-tap from
everyone. **Files:** `ChatScreens`, `FeedScreens`, `NodeScreens`, `Chrome` (Baud).
**Steps:** one plain-language empty line + a decorative `aria-hidden` Baud; a confirm
dialog before a PUBLIC/broadcast send; **no** unread badges (no read tracking → would be
fake). **Acceptance:** first-run comprehensible; PUBLIC requires confirm.

## B4 — Notifications + transfers overflow — ⬜ todo
**Why:** the foreground-service notification is static; `TransfersBar` shows only the
first few. **Files:** `NodeService.kt`, `MainActivity.kt` (Transfers/ReceivingBar).
**Steps:** notification text with short address / peer count / "relaying"; tap opens
MainActivity; transfers show `+N more`. **Non-goals:** per-message notification privacy
design. **Acceptance:** notification informative; no crash at zero peers.

## B5 — Advanced: ring health + cautious export — ⬜ todo
**Why:** the audit asked for prekey-ring health; any export defeats the 7-day window.
**Files:** `NodeScreens` (Advanced), `NodeController` (`secretPrefs` accessor only).
**Steps:** show live count / oldest age / next mint (thin JNI read, no widened freeze);
Export only behind a "this defeats the 7-day window" modal; single accessor, no second
seed copy (S-015 class). **Non-goals:** adjustable lifetime sliders (PR0b gate — dead-code
UI forbidden). **Acceptance:** health readout; export gated + warned.

## B6 — Bridges: status enum + permission recovery (no fake toggle) — ⬜ todo
**Why:** the LED uses brittle substring matching on status strings; a denied permission
dead-ends. **Files:** `NodeScreens` (BridgeRow), bridge classes, manifest flows.
**Steps:** map status → `up/connecting/down/error`; deep-link to app settings when
mic/BT/nearby is denied; **Remove stays the control** — no Start/Stop `Switch` until
start/stop is real for that row. **Acceptance:** no fake switch; denied permission has a
recovery path.

## B7 — Accessibility + density pass — ⬜ todo (depends B1–B6)
**Why:** icon-only 📎, LEDs/chips without descriptions, sub-48 dp targets, no way back to
the bottom of a scrolled chat. **Files:** `Chrome.kt`, `ChatScreens`, `FeedScreens`,
`MainActivity` (bottom nav). **Steps:** `contentDescription` on LEDs/chips/Baud/nav/📎; 48
dp targets; focus order → composer; jump-to-bottom FAB when scrolled up; verify
reduced-motion is fully static. **Acceptance:** TalkBack usable for send/attach/open; no
pink-on-olive.

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

## C1 — Token parity + forbidden-pair audit — ⬜ todo
**Why:** the design language rots when the four surfaces drift (VISUALDESIGN,
`site/style.css`, the web-standalone tokens, Android `Chrome.kt` Palette).
**Steps:** (1) table every token across the four surfaces, fix mismatches; (2) grep
Android for pink-on-kevlar, repaint with void/asphalt or an outline; (3) `Send` = pink
face + void ink (never pink-on-olive); (4) cyan 2 px focus on Toughbook fields + buttons;
(5) disabled = kevlar + dim label, not translucent pink; (6) verify reduced-motion gates
scanlines/bloom/sparkle on Android. **Acceptance:** parity table in the PR; no forbidden
pairs; VISUALDESIGN §7 checklist.

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

- [ ] PR0 merged — FS claim matches code **and** offline window disclosed + configurable
- [ ] PR1 merged — attachments usable end-to-end
- [ ] PR2 merged — bridges stoppable/removable
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
