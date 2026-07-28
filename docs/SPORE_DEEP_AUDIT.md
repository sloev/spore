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

**Minimum credible phone node:** PR0 + PR1 + PR2 + one device-matrix pass.

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
    // ...
    rk: [u8; 32],
    cks: Option<[u8; 32]>,
    ckr: Option<[u8; 32]>,
    nr: u16,
    skipped: HashMap<([u8; 32], u16), [u8; 32]>,  // ← no age, no zeroize
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

Change signatures:

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

Also call `purge_skipped(now)` at the top of `skip` if invoked standalone.

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

`now` is already `spore::bridge::hub::now()` / `Node` paths. Grep for `.decrypt(` and `Ratchet::` in `src/session.rs`, `src/node/*`, `src/lib.rs` and pass `now`. Prefer adding `now` only where missing; do not invent a second clock.

### 6. Tests (in `src/ratchet.rs`)

```rust
#[test]
fn skipped_keys_expire_after_ttl() {
    // alice encrypts msgs 0..3; bob receives only 3 first → skips 0..2
    // advance now by SKIP_TTL_SECS + 1; bob must fail to open 0 and skipped.is_empty()
}

#[test]
fn skipped_keys_live_inside_ttl() {
    // same setup; now += SKIP_TTL_SECS - 60; open still works
}

// existing: the_skipped_key_cache_cannot_grow_without_bound, absurd_gap_refused
```

Optional: debug canary that `Drop` clears (e.g. pattern fill + assert after drop in a scope).

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

## Still open update (`docs/SECURITY_FINDINGS.md`)
Mark S-024a closed with link to this PR; leave “no platform has field-verified the seven-day window” as process item for PR6.

---

# PR1 — Chat attachments: stage → one bubble → preview → Open

## Why
Release APK: pick file publishes immediately; no stage; bubbles lack preview/Open; no FileProvider. Highest daily annoyance. JNI file APIs already exist (`nativePublishFile`, `nativeOpenFile`, `nativeSaveFile`).

## Files
| Path | Action |
|------|--------|
| `android/.../ChatScreens.kt` | Composer staging, marker parse, bubble layout, viewer entry |
| `android/.../NodeController.kt` | Optional helpers; keep `sendFile` for publish; new `sendTextWithAttachment` |
| `android/app/src/main/AndroidManifest.xml` | FileProvider provider |
| `android/app/src/main/res/xml/file_paths.xml` | **New** |
| `android/UX-ISSUES.md` | **New** — problem, marker, acceptance |
| Optional small composable file | `AttachmentViewer.kt` if ChatScreens grows too large |

## Current behaviour (broken UX)

```kotlin
// ChatScreens.kt ~183
val pickFile = rememberLauncherForActivityResult(ActivityResultContracts.GetContent()) { uri ->
    // ...
    if (data != null) NodeController.sendFile(peer, name, data)  // immediate publish
}
```

`sendFile` publishes and appends a message with `magnet` but no unified text+attach body; Bubble has no Open/preview path.

## Implementation steps

### 1. Staging model (composer state)

In `ChatDetail` (per-peer):

```kotlin
data class StagedAttachment(
    val name: String,
    val bytes: ByteArray,
    val mime: String,
)

// remember per peer (key by peer string)
var staged by remember(peer) { mutableStateOf<StagedAttachment?>(null) }

val pickFile = rememberLauncherForActivityResult(ActivityResultContracts.GetContent()) { uri ->
    if (uri == null) return@rememberLauncherForActivityResult
    val name = /* DISPLAY_NAME as today */ ?: "file.bin"
    val mime = ctx.contentResolver.getType(uri) ?: "application/octet-stream"
    val data = ctx.contentResolver.openInputStream(uri)?.use { it.readBytes() } ?: return@rememberLauncherForActivityResult
    staged = StagedAttachment(name, data, mime)
    // do NOT call sendFile here
}
```

UI under composer field:
- If `staged != null`: chip with name / optional thumbnail + **✕** clears staged.
- Send button:
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

Keep staged when switching away and back to the same peer (`remember(peer)`).

### 2. Send path — one body with marker

```kotlin
// NodeController.kt
fun sendTextWithAttachment(peer: String, text: String, att: StagedAttachment) {
    // 1) publish file (existing sendFile core without appending a separate msg)
    val magnet = publishFile(peer, att.name, att.bytes) ?: return
    // 2) body
    val marker = "📎 ${att.name} | spore:$magnet | ${att.mime}"
    val body = if (text.isBlank()) marker else "$text\n\n$marker"
    send(peer, body) // existing text path (seal/ACK as today)
}
```

Extract publish-only from current `sendFile` so we do not double-append messages.

**Marker (canonical):**
```text
📎 <filename> | spore:<hex-magnet> | <mime>
```
- Last line matching `^📎 .+ \| spore:[0-9a-fA-F]+ \| \S+`
- Application convention only; relays see opaque UTF-8 payload.

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

Bubble structure (preserve side + border for mine/theirs):

```
┌ crate ─────────────────────────────────────┐
│ caption / peer                             │
│ message text (without marker line)         │
│ ┌ attachment chip / image ───────────────┐ │
│ │ thumbnail OR filename · mime           │ │
│ └────────────────────────────────────────┘ │
│ SegmentedLed + served/fetching (existing)  │
│ time · 🔒 · badges                         │
└────────────────────────────────────────────┘
```

- `image/*` + enough bytes: decode with `inSampleSize` from ~280.dp width on `Dispatchers.IO` (reuse Feed approach).
- Incomplete transfer: placeholder chip + LED only — **never** partial corrupt bitmap.
- Tap chip → `AttachmentViewer`.

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
// On open request:
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

- Preview/open cache under `cacheDir` (reclaimable).
- Explicit **Save** → `externalFilesDir` or MediaStore.
- Eviction: on start or idle, delete cache dirs older than 14 days or over ~50 MB total.
- Never write sealed ciphertext to a world-readable path.

### 6. Crypto honesty
- Staged send must use the same sealed-publish path as today’s `sendFile` when peer key is known.
- Keep **“served from this node”** / **“fetching”** — do not invent “delivered to peer”.

### 7. Docs
Create `android/UX-ISSUES.md` with: problem statement, current vs target, marker format, acceptance, non-goals (multi-file, in-app video, edit history).

## Non-goals (v1)
- Multi-file per send
- ExoPlayer / in-app video
- Editing messages after send

## Acceptance
- [ ] Pick → composer chip; thread unchanged until Send
- [ ] Remove staged → text-only Send
- [ ] Send → **one** bubble text+attach for sender and receiver
- [ ] LED while fetching; preview when image decodable
- [ ] Open via FileProvider for images (PDF if straightforward)
- [ ] No pink-on-olive; contentDescription on chips
- [ ] Sealed path preserved

## CHANGELOG
```markdown
- Android: chat attachments stage until Send; one bubble with preview; FileProvider Open/Share/Save.
```

## Manual QA checklist
1. Stage image, add text, Send → one bubble both sides (two devices or loopback).
2. Stage then ✕ → Send text only.
3. Large image → LED then preview; Open works.
4. Peer without file yet → fetching language, no crash on Open.
5. Reduced motion still respects system setting.

---

# PR2 — Hub unregister + bridge stop/remove UI

## Why
`Hub` slots and JNI `ifaces` only grow. UI cannot stop/delete bridges. A live-looking switch without backend repeats the “Save did nothing” failure mode.

## Files
| Path | Action |
|------|--------|
| `src/bridge/hub.rs` | `unregister(iface)` |
| `android/jni/src/lib.rs` | `nativeUnregisterIface` |
| `android/.../SporeNative.kt` | `external fun nativeUnregisterIface` |
| `android/.../NodeController.kt` | stop helpers, list removal |
| `android/.../NodeScreens.kt` | BridgeRow actions |
| Bridge classes | Call unregister from existing `stop()` |

## Current shape

```rust
// hub.rs
pub fn register(&self) -> (Iface, Receiver<Forward>) {
    let mut o = lock(&self.out);
    let iface = o.len() as Iface;
    o.push(Slot { tx: Some(tx), bulk: None });
    // never shrinks
}
```

```kotlin
// SporeNative.kt
external fun nativeRegisterIface(ptr: Long): Int
external fun nativeRegisterIfaceLimited(ptr: Long, bulkBytesPerSec: Int): Int
// no unregister
```

```kotlin
// NodeScreens BridgeRow — display only, no Stop/Remove
```

## Implementation steps

### 1. Hub unregister

```rust
/// Drop the outbound sender for `iface`. Receivers see disconnect; slot stays
/// as a hole so iface indices remain stable for any in-flight references.
pub fn unregister(&self, iface: Iface) {
    let mut o = lock(&self.out);
    if let Some(slot) = o.get_mut(iface as usize) {
        slot.tx = None;
        slot.bulk = None;
    }
}
```

Index stability: do **not** `remove()` from the Vec (would renumber later ifaces). Clear the slot in place. Document that iface ids are never recycled within a process lifetime (same as today’s append-only allocation).

### 2. JNI

```rust
// android/jni/src/lib.rs
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
// SporeNative.kt
external fun nativeUnregisterIface(ptr: Long, iface: Int)
```

### 4. Wire bridge stop()

Each bridge that registered an iface must:
1. Cancel pump jobs
2. Call `nativeUnregisterIface(ptr, iface)`
3. Release platform resources (GATT, AudioRecord, etc.)

```kotlin
// NodeController — example
fun stopBridge(kind: String) {
    when (kind) {
        "Audio" -> audioBridge?.stop()
        "Meshtastic BLE" -> meshtasticBridge?.stop()
        // ...
    }
    // stop() implementations call unregister for their iface id
    bridges.value = bridges.value.filterNot { it.kind == kind }
}
```

Store `iface: Int` on the bridge instance or in `BridgeState` when registering.

### 5. BridgeRow UI

```kotlin
@Composable
private fun BridgeRow(b: BridgeState) {
    // existing LED + kind + detail + status word
    Row {
        // ...
        CrateButton("Stop", { NodeController.stopBridge(b.kind) }, enabled = b.canStop)
        CrateButton("Remove", {
            NodeController.stopBridge(b.kind)
            // already removed from list in stopBridge
        })
    }
}
```

- **Edit** (URL bridges: ws/nostr/tcp): implement as Remove + re-add with new value until native supports mutate.
- If a kind cannot stop yet: omit Stop or disable with caption *“Stop requires a core update”* — **never** a grey switch that no-ops.

### 6. Tests
- Rust unit: register two ifaces, unregister first, `on_rx` / originate only fans to live slots.
- Android manual: add Audio, Stop → status offline, Remove → row gone; re-add works.

## Acceptance
- [ ] Remove drops UI row, stops pumps, unregisters iface
- [ ] Start/Stop round-trip for ≥1 Kotlin bridge and ≥1 URL bridge
- [ ] Unsupported Stop does not look live
- [ ] No renumbering of still-live ifaces after unregister

## CHANGELOG
```markdown
- Hub/JNI: unregister interface; Android bridge list supports stop/remove.
```

---

# PR3 — Android lifecycle hygiene (depends on PR2)

## Why
Sticky service restart can orphan native handles; AudioBridge stop not re-entrant-safe; BLE drain launches overlapping coroutines; no reconnect; Wi-Fi Direct may start UDP before group is up.

## Files
| Path | Action |
|------|--------|
| `NodeService.kt` | Controlled shutdown on destroy |
| `NodeController.kt` | `stop()` that cancels jobs, stops bridges, `nativeFree` when last owner |
| `AudioBridge.kt` | Null fields after release |
| `BleBridges.kt` | Single drain job; reconnect backoff |
| `WifiDirectBridge.kt` | Wait for group/CONNECTION_CHANGED |

## Implementation steps

### 1. NodeService + NodeController.stop

```kotlin
// NodeService.kt
override fun onDestroy() {
    multicastLock?.release()
    multicastLock = null
    NodeController.stopFromService()
    super.onDestroy()
}
```

```kotlin
// NodeController
fun stopFromService() {
    // cancel housekeeping / poll jobs
    audioBridge?.stop(); audioBridge = null
    // ... all bridges stop (unregister via PR2)
    val p = ptr
    if (p != 0L) {
        SporeNative.nativeFree(p)
        ptr = 0L
    }
}
```

`START_STICKY` restart must go through normal `start()` and `nativeNew` again — never reuse a freed jlong (registry already no-ops bad handles; still free explicitly).

### 2. AudioBridge.stop hygiene

```kotlin
fun stop() {
    rxJob?.cancel(); txJob?.cancel()
    rxJob = null; txJob = null
    try { record?.stop(); record?.release() } catch (_: Exception) {}
    try { track?.stop(); track?.release() } catch (_: Exception) {}
    record = null; track = null
    // unregister iface if held
}
```

### 3. Meshtastic single drain

```kotlin
private var drainJob: Job? = null

fun requestDrain(g: BluetoothGatt) {
    if (drainJob?.isActive == true) return
    drainJob = scope.launch {
        try {
            // existing FromRadio read loop (up to 32)
        } finally {
            drainJob = null
        }
    }
}
```

### 4. BLE reconnect

On `STATE_DISCONNECTED`:
- Update BridgeState status to `"reconnecting"`
- Schedule reconnect with exponential backoff (1s, 2s, 4s, … cap 60s)
- Cancel on explicit `stop()` / Remove

Mirror native `stream_link` spirit; keep UI status word honest.

### 5. Wi-Fi Direct

Do not call `nativeStartUdpLimited` until `CONNECTION_CHANGED` / group info confirms the P2P iface is up. On failure/BUSY, surface status and allow retry.

## Acceptance
- [ ] Force-stop / destroy does not leave zombie native pumps after sticky restart
- [ ] AudioBridge can start → stop → start without crash
- [ ] Rapid FromNum notifies do not stack drain coroutines
- [ ] BLE disconnect shows status and backs off

## CHANGELOG
```markdown
- Android: service/native shutdown; AudioBridge stop hygiene; BLE single-drain + reconnect.
```

---

# PR4 — Profile: “Name others see” + local avatar

## Why
ANNOUNCE already carries petname; Nearby already prefers local → quoted announced → address. UX does not frame the field as public-facing. No avatar at all.

## Files
| Path | Action |
|------|--------|
| `NodeScreens.kt` / Connect / Advanced | Label + live preview |
| `NodeController.kt` / prefs | Avatar magnet in EncryptedSharedPreferences |
| `ChatScreens.kt` Nearby / headers | Optional avatar slot |
| Optional core follow-up | ANNOUNCE avatar magnet (document as app convention) |

## Implementation steps

### 1. Name copy
- Label: **“Name others see”** (not only “petname”).
- Live preview string using same rules as `ChatsList` Nearby.
- Existing dirty-enabled Save + confirm stays.

### 2. Local avatar
1. User picks image (size-cap before publish, e.g. max edge 256 px / ≤128 KB).
2. `nativePublishFile` (or existing publish helper) → magnet.
3. Store magnet in EncryptedSharedPreferences via the **single seed/prekey accessor pattern** (load-bearing — do not invent a second prefs path).
4. Show on own Connect / profile / optional chat header; letter avatar fallback.

### 3. Mesh avatar (optional same PR or PR4b)
- ANNOUNCE payload convention: optional trailing avatar magnet bytes — **document in DESIGN notes as application-level**, do not claim wire freeze change if it fits existing name blob extensibility; if it requires a new flag/field, split to a labelled discussion.
- Prefer **local-only in PR4** if any freeze risk.

## Acceptance
- [ ] Preview matches Nearby
- [ ] Set/change local avatar; visible on own surfaces
- [ ] Size cap enforced before publish

## CHANGELOG
```markdown
- Android: “Name others see” framing; local profile avatar publish/cache.
```

---

# PR5 — Store spilled-file id verify (C-ST4)

## Why
Spill path trusts filename == content id. Disk bit-rot or a replaced file would be served as valid.

## Files
| Path | Action |
|------|--------|
| `src/store.rs` | Verify on `wire()` read from disk |

## Current

```rust
pub fn wire(&self, id: &Id) -> Option<Vec<u8>> {
    match &self.map.get(id)?.body {
        Body::Mem(w) => Some(w.clone()),
        Body::Evicted => std::fs::read(self.spill.as_ref()?.join(filename(id))).ok(),
    }
}
```

## Implementation

```rust
pub fn wire(&self, id: &Id) -> Option<Vec<u8>> {
    match &self.map.get(id)?.body {
        Body::Mem(w) => Some(w.clone()),
        Body::Evicted => {
            let path = self.spill.as_ref()?.join(filename(id));
            let bytes = std::fs::read(path).ok()?;
            // id = SHA-256(envelope with hops=0)[..16]
            if let Ok((env, n)) = crate::envelope::Envelope::decode(&bytes) {
                if n == bytes.len() && env.id() == *id {
                    return Some(bytes);
                }
            }
            None // treat as not held; mesh can re-fetch
        }
    }
}
```

Use the same `Envelope::decode` / `id()` path the rest of the crate uses (confirm exact method names in `envelope.rs`).

## Tests
- Put envelope, spill, `wire` OK.
- Corrupt one byte on disk → `wire` returns `None`.
- Truncated file → `None`.

## Acceptance
- [ ] Intact spill still loads
- [ ] Mismatch → `None`, no panic
- [ ] No freeze surface touch

## CHANGELOG
```markdown
- Store: verify spilled envelope id on disk read.
```

---

# PR6 — Field verification + docs honesty

## Why
Still open: no platform field-verified 7-day FS; all radio bridges 🧪; HARDWARE.md procedure only; Android never device-run for backup exclusion.

## Deliverables

### 1. Device matrix checklist
Add to `docs/ANDROID_AUDIT.md` and/or `android/TESTING.md`:

| # | Test | Pass criteria |
|---|------|----------------|
| 1 | Fresh install | Node starts, address shown |
| 2 | Upgrade from prior APK | Seed reveal works (0.6.0 regression class) |
| 3 | Reveal seed | Matches expected; single accessor |
| 4 | `adb backup` / cloud backup attempt | Identity + prekey ring **absent** from backup |
| 5 | Device transfer (if available) | spore prefs/store absent per extraction rules |
| 6 | 24–48 h soak | No native crash; audio/BLE optional |
| 7 | 7-day FS procedure | Document steps; mark result when run |

### 2. HARDWARE.md
- Run procedure for **one** of Meshtastic BLE or RNode **or**
- Demote README/APPS language that implies radio readiness until results exist.

### 3. Surface Still open
- Docs site security page: short “Still open” blurb
- App Advanced/About: one paragraph on FS windows (prekey 7d; ratchet age-bounded after PR0)

### 4. Optional note
Release pipeline dry-run / fixture approach (historical S-021…S-030) — design note only unless implementing.

## Acceptance
- [ ] Checklist exists and at least backup + migration run once on hardware
- [ ] HARDWARE.md has results **or** marketing demoted

## CHANGELOG
```markdown
- Docs: Android device-test checklist; HARDWARE.md [results or honesty pass].
```

---

# PR7 — Polish (batchable)

| Item | Sketch |
|------|--------|
| Ring health UI | `Prekeys: N live · oldest Xd · next mint ~Yh` + Export with FS warning |
| Group `key_id` badge | On mismatch show warning; never claim roster consensus |
| Boot receiver | Optional, **default off**, user setting |
| Sound/particles | Behind setting, default off |
| contentDescription | LEDs, badges, attachment chips |
| Housekeeping assert | Android intervals match SPEC 5→80 min HELLO / 1 h flood |
| `demod_out` cap | JNI VecDeque max ~32 frames, drop oldest |
| Docs sync | Android bridge list ⊆ `docs/BRIDGES.md` if not automated |

Ship as one PR or small stacked PRs under the same milestone.

---

## Suggested calendar

```
Week 1
  d1–2  PR0  ratchet (core)
  d2–4  PR1  attachments (Android)     ∥ PR0
  d4–5  PR2  unregister + bridge UI     ∥ if capacity

Week 2
  d1–2  PR3  lifecycle (after PR2)
  d2–3  PR4  profile local
  d3    PR5  store verify              ∥ anytime
  d4–7  PR6  device matrix + hardware note

Later   PR7  polish
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
```

## Definition of done — credible phone node

- [ ] PR0 merged — FS claim matches code
- [ ] PR1 merged — attachments usable end-to-end
- [ ] PR2 merged — bridges stoppable/removable
- [ ] ≥1 device-matrix pass (backup exclusion + migration)
- [ ] One radio path ✅ in HARDWARE.md **or** marketing demoted

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

---

## Out of scope

- Wire / C ABI changes
- Group membership consensus protocol
- Multi-file attach, in-app video, post-send edit
- Claiming 🧪 radios production-ready without HARDWARE results
- Full release-pipeline fixture automation (note only)

---

*Actionable plan derived from static audit of the 0.6.0 tree (2026-07-28). Update when PRs land or hardware results arrive.*
