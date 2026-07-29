# SPORE Multi-PR Release Plan (Actionable)

**Project:** `sloev/spore`  
**Audited version:** 0.6.0 (`Cargo.toml`) / master 2026-07-28  
**Source:** Consolidated from `docs/SPORE_DEEP_AUDIT.md` Parts I–VII + live tree in `spore-master`  
**Goal:** Ship reviewable PRs ordered by urgency. Each PR is self-contained with files, code sketches, tests, acceptance, and CHANGELOG text.

Wire format and C ABI stay frozen. No `allow-frozen-change` required for this series.

---

## Principles

1. One concern per PR — reviewable, revertible, CI-green alone.
2. No fake UI — never ship a control whose backend is missing.
3. Honesty preserved — served/fetching language, 🧪 markers, Still open, VISUALDESIGN.
4. Security P0 is orthogonal to UX — PR0 does not block PR1.
5. Micro-commits inside a PR are fine; merge units stay the PR boundaries below.

---

## PR map

| PR | Title | Urgency | Depends | Parallel with |
|----|-------|---------|---------|---------------|
| **PR0** | Ratchet age-bound skipped keys + zeroize | **P0 security** | — | PR1, PR5 |
| **PR1** | Chat stage/attach/preview/FileProvider | **Critical UX** | — | PR0, PR5 |
| **PR2** | Hub unregister + bridge stop/remove | **High UX** | — | PR0, PR1 |
| **PR3** | Service / Audio / BLE lifecycle | High reliability | **PR2** | — |
| **PR4** | Name others see + local avatar | Medium product | — | PR3+ |
| **PR5** | Store spilled id verify | Medium hardening | — | PR0–PR2 |
| **PR6** | Device matrix + HARDWARE honesty | Process | PR0–PR3 ideally | — |
| **PR7** | Polish batch | Low | PR4 | — |
| **PR8** | SPORE Direct: negotiated E2E pipe (general) | Feature / product | — (no core freeze) | PR0–PR7 |
| **PR9** | Offline crypto lifetime knobs (FS vs DTN) | **Semi-urgent** product/security honesty | **PR0** (ratchet TTL exists) | PR1–PR2 |
| **PR10** | Iroh bridge (QUIC p2p + relay fallback) | Feature / networking | — | PR2 helpful for stop/unregister |

**Minimum credible phone node:** PR0 + PR1 + PR2 + one device-matrix pass.

**Direct-pipe track (orthogonal):** PR8 can start anytime; does not block phone-node definition of done. Ships as optional library + docs, not a relay behaviour change.

**FS/DTN honesty track:** PR9 should land soon after PR0 so defaults and UI match the real decrypt window. Optional longer offline is opt-in with theft warning.

**Iroh track:** PR10 is a normal bridge (like tor/i2p/tcp): carry SPORE envelopes over iroh QUIC; 🧪 until exercised.

---

# PR0 — Ratchet skipped-key TTL + zeroize (S-024a)

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
Mark S-024a closed in `docs/SECURITY_FINDINGS.md`; leave field-verification of the 7-day window for PR6.

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

### 3. Mesh avatar
Prefer local-only in PR4. Mesh ANNOUNCE convention as optional PR4b if no freeze risk.

## Acceptance
- [ ] Preview matches Nearby
- [ ] Set/change local avatar; visible on own surfaces
- [ ] Size cap enforced before publish

## CHANGELOG
```markdown
- Android: "Name others see" framing; local profile avatar publish/cache.
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
- [ ] Intact spill loads
- [ ] Mismatch -> None, no panic
- [ ] No freeze surface touch

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
- [ ] Checklist exists; backup + migration run once on hardware
- [ ] HARDWARE results **or** marketing demoted

## CHANGELOG
```markdown
- Docs: Android device-test checklist; HARDWARE.md [results or honesty pass].
```

---

# PR7 — Polish (batchable)

| Item | Sketch |
|------|--------|
| Ring health UI | `Prekeys: N live · oldest Xd · next mint ~Yh` + Export with FS warning |
| Group key_id badge | Warn on mismatch; never claim roster consensus |
| Boot receiver | Optional, **default off** |
| Sound/particles | Behind setting, default off |
| contentDescription | LEDs, badges, attachment chips |
| Housekeeping assert | Android intervals match SPEC 5->80 min / 1 h |
| demod_out cap | JNI VecDeque max ~32, drop oldest |
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

## Acceptance
- [ ] Negotiation selects a medium that meets `min_bps` + MTU or rejects clearly
- [ ] Keys derived; records authenticate; no media in SPORE store
- [ ] Same record bytes work on ≥2 adapters (UDP + TCP framing minimum)
- [ ] General `DATA` path works without any audio codec in tree
- [ ] Zero changes to frozen contract / envelope layout
- [ ] `docs/DIRECT.md` states threat model (underlay can drop/delay/record ciphertext)

## CHANGELOG
```markdown
- **Direct (optional):** application profile for negotiated E2E datagram pipes (throughput + medium + key); same record, per-medium ports; does not alter v1 relay behaviour.
```

## Branch naming
```
feat/spore-direct-pipe
feat/offline-crypto-lifetime-knob
feat/bridge-iroh
```

## Depends / parallel
- **No dependency** on PR0–PR7 for a library + UDP example
- Android/ESP32 UI and Codec2-on-pipe can follow as PR8b / separate apps
- Complements session layer (`src/session.rs`): sessions ride envelopes; Direct rides a sideband port after SPORE signaling

---


---

# PR9 — Offline crypto lifetime knobs (FS vs sneakernet honesty)

## Why
Sneakernet can deliver ciphertext weeks late; default prekey / (post-PR0) ratchet skipped-key lifetimes are ~**7 days**. Users who expect “message in a bottle” will hit undecryptable sealed mail without explanation. The fix is not one true lifetime — it is **disclose the window, let them raise it, warn that theft exposure grows**.

Semi-urgent: without this, PR0’s honest 7-day ratchet TTL and existing prekey ring feel like a silent foot-gun.

## Depends
- **PR0** should land first (or same release train) so ratchet age bound exists to configure.
- Prekey lifetime already exists in core (`PREKEY_LIFETIME_SECS` / ring sweep) — expose and document; do not invent a second clock.

## Goals
1. Single clear **policy surface**: how long sealed/session material stays openable while offline.
2. **Safe default** (~7 days) unchanged for phones.
3. **Opt-in longer window** in Android settings + daemon config.
4. Honest copy on decrypt failure and in Advanced / About.
5. Optional alignment note: store may still hold ciphertext longer than keys survive.

## Files (indicative)

| Path | Action |
|------|--------|
| `src/node/identity.rs` (or constants) | Confirm prekey lifetime is configurable / not only a hard const |
| `src/ratchet.rs` | `SKIP_TTL_SECS` from config/param after PR0 (default 7d) |
| Daemon YAML / CLI config | `prekey_lifetime_secs`, `ratchet_skip_ttl_secs` |
| Android Advanced settings | Slider or presets: 7d (default) / 14d / 30d / custom |
| EncryptedSharedPreferences | Persist chosen policy via **single accessor** pattern |
| About / security blurb | “Encrypted DMs readable ~N days offline…” |
| `docs/DESIGN.md` or `SECURITY.md` | Document FS vs DTN tradeoff |
| Decrypt-failure UI | “Key expired for offline window; ask resend or raise Offline encrypted mail” |

## Behaviour

| Setting | Effect |
|---------|--------|
| Default (7d) | Current security posture |
| Raised (e.g. 30d) | Keep prekey secrets + skipped keys longer; **warn**: stolen device reads more history |
| Ring backup | Still defeats the window if restored — label that separately |

Do **not** restore deleted prekeys from seed alone. Longer lifetime only keeps secrets that were never wiped.

## Acceptance
- [ ] Default remains ~7 days for prekey + ratchet skip TTL
- [ ] User/daemon can raise lifetime; value survives restart
- [ ] Warning shown when raising above default
- [ ] About/Advanced states the active window in plain language
- [ ] Failed open of expired sealed mail shows actionable message
- [ ] No freeze-surface change

## CHANGELOG
```markdown
- Config/UI: offline encrypted-mail window (prekey + ratchet skip TTL) adjustable; default 7 days; longer = more theft exposure.
```

## Branch
```
feat/offline-crypto-lifetime-knob
```

---

# PR10 — Iroh bridge (QUIC p2p envelopes)

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
- **SPORE Direct (PR8):** sideband **non-stored** pipe after negotiation; can later list `iroh` as a **Direct candidate medium** once both exist — not required for PR10.

---

## Suggested calendar

```
Week 1
  d1–2  PR0  ratchet (core)
  d2–4  PR1  attachments (Android)     || PR0
  d4–5  PR2  unregister + bridge UI

Week 2
  d1–2  PR3  lifecycle (after PR2)
  d2–3  PR4  profile local
  d3    PR5  store verify              || anytime
  d4–7  PR6  device matrix + hardware

Later   PR7  polish

Week 2–3 (semi-urgent honesty)
  after PR0  PR9  offline crypto lifetime knobs + UI/config copy

Anytime (parallel track)
  PR8   spore-direct library + docs/DIRECT.md + UDP/TCP example
  PR8b  Codec2 / ESP-NOW / Android call UI (optional follow-ups)
  PR10  bridge-iroh feature + BRIDGES.md (desktop/daemon first)
```

## Branch naming

```
fix/ratchet-skip-ttl
feat/android-attachments
feat/hub-unregister-bridges
fix/android-lifecycle
feat/android-profile-local
fix/store-spill-verify
docs/device-hardware-matrix
feat/spore-direct-pipe
```

## Definition of done — credible phone node

- [ ] PR0 merged — FS claim matches code
- [ ] PR1 merged — attachments usable end-to-end
- [ ] PR2 merged — bridges stoppable/removable
- [ ] >=1 device-matrix pass (backup exclusion + migration)
- [ ] One radio path checked in HARDWARE.md **or** marketing demoted

---

## Audit ID index

| PR | IDs |
|----|-----|
| PR0 | S-024a, C-R1, C-R2 |
| PR1 | A-NS3, Part VII §62, Patch F |
| PR2 | A-N2, C-H4, A-NS2, Patch B |
| PR3 | A-S1, A-A2, A-B1/B2/B6, A-W1, C-B1, Patch D/E |
| PR4 | Part VII §63 |
| PR5 | C-ST4, Patch C |
| PR6 | Still open field gaps, D-1 |
| PR7 | UX-1/2, A-M3, A-NC3, A-J3, C-D1 |
| PR8 | Design discussion (Direct plane); no prior S-nnn — new optional profile |
| PR9 | FS vs DTN / prekey window productization; pairs with S-022 residual + S-024a |
| PR10 | New bridge; follow BRIDGES.md 🧪 pattern (tor/i2p class) |

---

## Out of scope

- Wire / C ABI changes
- Group membership consensus protocol
- Multi-file attach, in-app video, post-send edit
- Claiming 🧪 radios production-ready without HARDWARE results
- Full release-pipeline fixture automation (note only)
- Routing Direct records through store-and-forward relays (Direct is non-routed by definition)

---

*Actionable plan derived from static audit of the 0.6.0 tree (2026-07-28), plus Direct-pipe, offline-lifetime knobs, and iroh bridge tracks. Update when PRs land or hardware results arrive.*
