# Android production audit

SPORE Communicator, reviewed for the move from prototype (M0–M5) to a release
somebody else's phone runs all day.

**How to read this.** Every claim is tagged. **Verified** means I read the code and
cite the line. **Reasoned** means the architecture implies it and I have not
measured it. The distinction is the point: this repository has a register of
findings that existed only in prose, and an audit that blurs the two just adds to it.

---

## Progress

Updated as work lands, so this file never describes a state the code left behind.

| Item | Status |
|---|---|
| §0 Backup exfiltration — `allowBackup="false"` + extraction rules | ✅ fixed |
| §0 Encryption at rest — Keystore `EncryptedSharedPreferences` + migration | ✅ fixed, **not yet run on a device** |
| §2 Petname save feedback — snackbar + `enabled` state on both Save buttons | ✅ fixed, **not yet run on a device** |
| §2 Chat rewrite — bubbles, alignment, segmented fragment status | ✅ fixed, **not yet run on a device** |
| §2 Feed — markdown bodies, inline image attachments, Compose Post screen | ✅ fixed, **not yet run on a device** |
| §2 Bridges — grouped by transport, LED status per row | ✅ fixed, **not yet run on a device** |
| §2 Bridge stop/remove — `hub.unregister` + `nativeUnregisterIface` + Remove UI | ✅ fixed, **not yet run on a device** |
| §2 `FileProvider` + `ACTION_VIEW` for received files | open |
| §2 Message reactions | open |
| §1 Battery measurement, JNI soak | open |

## 0. Ship this first: the seed and prekey ring are world-readable to a backup

**Verified.** Severity: **High**. **Status: fixed — see the end of this section.**

```
NodeController.kt:110    getSharedPreferences("spore", MODE_PRIVATE)   ← seed, base64
NodeController.kt:213    saveRing(...)                                 ← prekey ring, base64
AndroidManifest.xml:28   android:allowBackup="true"
```

`MODE_PRIVATE` keeps other *apps* out. It does nothing about Android Auto Backup,
which uploads the app's `shared_prefs` to the user's Google Drive, or about
`adb backup` on an unlocked device.

So the identity seed **and every live prekey secret** leave the device
automatically, to a third party, by default.

This is not a generic hardening item. It specifically destroys the property added
in S-022: [`CONTINUITY.md`](CONTINUITY.md) states that *"a backup of the ring
defeats the seven-day window"*, and Android is performing exactly that backup on a
schedule nobody chose. **The seizure-resistance story is currently false on this
platform** — take the device or the Google account and you get the identity plus
every prekey that has not yet expired.

**Fix, in order.** Each step is shippable alone.

1. `android:allowBackup="false"`, plus `android:dataExtractionRules` excluding the
   `spore` preferences. One line; closes the exfiltration path today.
2. `EncryptedSharedPreferences` over a Keystore `MasterKey` (AES256-GCM,
   `setUserAuthenticationRequired(false)` — the service must run while the device is
   locked).
3. Longer term: prekey secrets should not cross the JNI boundary in the clear at
   all. Export them wrapped under a Keystore key so Kotlin never holds plaintext.

### What landed

Steps 1 and 2, together, because step 1 alone leaves the secrets in cleartext on
disk and step 2 alone leaves them going to Drive:

- `android:allowBackup="false"`, `android:fullBackupContent="false"`, and
  `res/xml/data_extraction_rules.xml` excluding `spore.xml`, `petnames.xml` and the
  `store` directory from **both** cloud backup and device-to-device transfer. The
  transfer path is separate from backup and would otherwise have carried the same
  secrets to a new phone in the clear.
- `NodeController.secretPrefs()` — `EncryptedSharedPreferences` over a Keystore
  `MasterKey` (AES256-SIV keys, AES256-GCM values), replacing every
  `getSharedPreferences("spore", MODE_PRIVATE)` call site.
- `migrateSecrets()` copies an existing install's seed and ring into the encrypted
  store on first run and clears the plaintext file. **Without this an upgrade looks
  like a factory reset** — new identity, new address, unreachable inbox.
- No user authentication to unwrap: the foreground service must keep relaying while
  the screen is locked. The threat this closes is offline extraction, not a thief
  holding an unlocked phone. Said plainly in the code comment so nobody later
  "hardens" it into a node that stops carrying mail at lock.
- Keystore failure falls back to plaintext prefs with a loud log rather than
  crashing. Losing an identity is worse than the status quo ante; the fallback is a
  deliberate trade, not an oversight.

### Two corrections to the above, both caught after it was written

**The manifest did not parse.** The explanatory comment went *inside* the
`<application>` start tag, between the tag name and its attributes, which is not
well-formed XML. The APK job failed with a bare
`ManifestMerger2$MergeFailureException: Error parsing AndroidManifest.xml` and no
line number. There is no Android SDK in the environment this was written in, so the
change was verified by reading — and reading is exactly what does not catch a
misplaced comment. Fixed, and every XML file in the repository now parses.

**`<device-transfer>` was excluding three named paths, not the data.** The seed
(`spore.xml`) and the store (`filesDir/store`) were covered. Received attachments
were not: they are written into the external files dir under names taken from the
sender (`NodeController.kt:413`), so there is no list of paths to enumerate. Since
`allowBackup="false"` does *not* govern device-to-device transfer on API 31+, a
transfer to a new phone would have carried every received file across in the clear
while the identity beside it was correctly excluded.

That is the repository's most-recorded failure shape — a fix verified on the artefact
it was written for and assumed on its neighbour (S-015, S-019, S-023, S-025, S-026,
S-029, S-030). The rules now exclude whole domains rather than named paths, in both
blocks, so a new file in a new location is excluded by default rather than by
someone remembering to add it.

**Not yet verified on a device.** This compiles and the logic is straightforward,
but the migration path — old install, upgrade, same address — has not been run on
real hardware, and neither has a device-to-device pairing. Those are the checks that
matter and they have not happened. The `path="."` idiom in particular is **Reasoned**:
it follows from how the framework resolves a rule path against its domain root, and I
have not watched a transfer skip the files.

Worth a findings-register entry when it is confirmed working.

---

## 1. Architecture

### Foreground service

**Verified:** `foregroundServiceType="dataSync"` is declared, and the housekeeping
loop is a single coroutine on `Dispatchers.IO` — so the "heavy work on the main
thread" risk a generic review would flag does not apply here.

**Verified:** the loop recomputes `peers`, `storeLen`, delivery and file state on
every tick regardless of whether any UI is observing. Gate it on lifecycle; a
backgrounded app does not need `peers.value` recomputed every 30 s.

**Verified and fixed (S-023):** the mesh-wide ANNOUNCE flood is closed — `nativeHello`
carries the frequent link-local beacon and the flood runs hourly. Any review still
listing this as open is working from stale information.

**Reasoned:** `MulticastLock` should be released when Wi-Fi drops or it pins the
radio out of power-save.

### JNI boundary

**Verified:** the `Runtime` behind the `jlong` handle is never freed — the app holds
one for its lifetime and `nativeFree` is never called. Defensible, but a path that
calls `nativeNew` twice leaks an entire node. Worth an explicit guard.

**Reasoned, and the most likely real defect:** local-reference accumulation.
`env.byte_array_from_slice(...).into_raw()` runs inside `nativePollForward` and
`nativePollDelivery`, which are poll loops by construction. JNI local refs are
released when the native frame returns, so a single call is fine — but any loop
*inside* a native call that allocates per iteration needs `DeleteLocalRef` or an
explicit frame. A leak here aborts under load rather than degrading, so it will not
show up in light testing.

**Reasoned:** large payloads currently copy across the boundary. `DirectByteBuffer`
would avoid it. Worth doing only after measurement — at mesh message sizes the copy
is probably noise, and this is exactly the kind of optimisation that gets done
because it sounds right.

### Headless WebViews

**Reasoned, unmeasured, and my prime suspect for battery.** Each WebView is a full
renderer process that does not suspend cleanly under Doze. Two or three of those
plus a partial WakeLock is a phone that gets warm in a pocket.

A generic recommendation here is "move them to `android:process`". **That does not
work as stated for this app:** the node lives behind a `jlong` handle in the app's
own address space, so a WebView in a separate process cannot reach it without an
IPC layer that does not exist. Isolating them is a real option, but it is an
architecture change, not a manifest attribute.

The cheaper wins: cap concurrent instances, pool and reuse, call `destroy()`
explicitly, and offer a **Lite mode** that disables WebView-backed bridges entirely
for low-power installs.

**Measure before optimising.** Battery Historian on a 12-hour run with all bridges
up. The hypothesis above is a hypothesis.

---

## 2. UX

### Confirmed bugs

Line numbers are omitted on purpose: the screens have since been rewritten, and a
citation that has drifted is worse than none.

| Report | What the code actually does |
|---|---|
| "No reaction on clicking save on settings.petname" | `onClick = { Petnames.set(peer, editingName) }`. **It does save.** There was no snackbar, no dismiss, no visible state change, so it read as broken. The fix is feedback, not persistence — a review guessing "missing onClick handler" would have fixed the wrong thing. **Fixed — see below.** |
| "Can't delete/edit/disable/enable bridges" | **Fixed (PR2)** for the removable kinds. `Hub::unregister` + `nativeUnregisterIface` retire an interface (as a hole — ids never recycle, so `Flood`'s index `except` can't misroute); `stopBridge` cancels the bridge's pumps and drops its row. Audio, BLE (Meshtastic/RNode), Wi-Fi Direct and Web get a real **Remove**; core-owned TCP/UDP show no button rather than a dead one. *Edit* is still Remove-and-re-add; *enable/disable toggle* (vs remove) awaits PR3's reconnect work. |
| "Can't open attached files" | Only `ACTION_SEND` (share out). No `FileProvider`, no `ACTION_VIEW`. Still open for received files; feed images now render inline, which is a different path. |

#### What landed for the save buttons

A `SnackbarHostState` on the existing `Scaffold`, provided to the screens through a
`LocalSnackbar` composition local, and a `rememberConfirm()` helper. Both Save
buttons now confirm, and both are `enabled` only while the field differs from what
is stored — so the button carries state *before* the press as well as after, which
is the half a snackbar alone does not cover.

Two things surfaced while wiring it that the report could not have known:

- **Compare what will be stored, not what is typed.** `Petnames.set` trims and
  `setMyName` trims *and* caps at 32. Comparing the raw field would leave Save lit
  forever against a value already saved — a trailing space, or a 40-character name.
  The `enabled` check uses the same transform the setter applies.
- **`setMyName` returned `Unit` and bailed on `ptr == 0L`.** Before the node is up
  the call is a silent no-op, so a snackbar bolted on at the call site would have
  cheerfully confirmed a save that never happened — the original bug wearing a
  different hat. It now returns `Boolean` and the UI says "Node not started yet —
  not saved". `Petnames.set` needs no equivalent: it writes to prefs and is
  independent of the node handle.

**Not yet run on a device.**

#### A regression the encryption fix left behind

`MainActivity.kt` read `getSharedPreferences("spore", MODE_PRIVATE)` directly to
show the seed on the Advanced screen. `migrateSecrets()` clears that file, so on
every upgraded install "Reveal seed" displayed `unavailable` — the identity was
intact, the one screen that shows it was not. I replaced all the call sites in
`NodeController.kt` and none in the UI.

Same shape as S-015/S-019/S-023/S-025/S-026/S-029/S-030: verified on the artefact
the change was written for, assumed on its neighbour. Now a `NodeController.seedHex()`
accessor, so exactly one file knows where secrets live and the UI cannot go around
it.

### Chat view

The weakest screen, and the one people judge the app by. **Rewritten** against the
Claude Design mock, with `docs/VISUALDESIGN.md` §3's chrome rather than the mock's
own rounded-and-soft styling — see that file's status table for the two places the
mock and the spec disagreed and which won.

What landed:

- Messages are crates, right-aligned when mine and left when not. Sent and received
  are told apart by **border colour *and* side**, never fill alone — §1 forbids
  signalling by colour only, and alignment is what carries it for anyone who cannot
  see the difference.
- **Fragment status is real, on both directions.** `Msg` gained a `magnet`, so a
  file bubble reads its chunk state out of the existing `transfers` flow rather than
  keeping a second copy that can disagree. Progress is a §3 segmented LED, because
  fountain chunks are a countable unit of work and a smooth bar would be inventing
  precision.
  - An *incoming* file shows `have/count · fetching` and fills as chunks land.
  - An *outgoing* file is complete the moment it is published, so the LED fills
    immediately and the label says **"served from this node"** rather than
    implying anyone fetched it. Whether a peer pulled a chunk is not observable
    from here, and a status line that claims delivery it cannot see is a lie.
- The mock's **unread badge is deliberately absent.** There is no read tracking in
  the app, so the number would have nothing behind it.

Still open: reactions (a `FlowRow` overlapping the bubble's bottom edge, opened by
long-press), and audio/video previews — those need the `FileProvider` work above
first, and ExoPlayer, which is a dependency worth its own decision.

### Feed

Also rewritten, and it gained the mock's Compose Post screen.

- **Markdown renders.** `Markdown.kt` handles `**bold**`, `*italic*`/`_italic_`,
  `` `code` `` and `[text](url)` — inline only, no headings or lists, no dependency.
  A post is a sentence or two under a per-envelope size budget; a full parser here
  would be a fuzz target for no gain. Unmatched delimiters stay literal, so `2 * 3`
  survives and a truncated post still reads. Links are styled but **not tappable**:
  the text is attacker-controlled and signed-but-public, so the URL rides as an
  annotation and nothing opens it.
- **Images are referenced from the post body**, as
  `![name](spore:<magnet>)`. The image cannot be *in* the post — a post is one
  signed envelope of UTF-8 — so the bytes go through the same manifest-and-chunk
  path as any shared file and the marker points at them. A reader with the chunks
  renders it; a reader without sees the LED fill. A client that does not know the
  marker sees a plain markdown image link, which is a reasonable thing to see.
- The author's own copy is written locally on IO, because our own file never comes
  back through the mesh — otherwise the one person who cannot see the image is the
  person who posted it.
- Decoding uses `inSampleSize` inside `produceState` on `Dispatchers.IO`. A phone
  photo is tens of megapixels; decoding it whole for a 220 dp row would spend
  ~100 MB of heap, and doing it in composition would stall a scrolling list.

### Bridges

Grouped by transport (Radio / Network / Web) as the mock does, with a status LED per
row. The buckets are **derived from the kind string**, so a new bridge kind lands in
"Other" rather than vanishing from the list.

The mock draws a switch on each row. There is still no `nativeStopBridge`, so a
switch would be a control that cannot turn anything off — the row reports state
instead, and the toggle lands when the JNI call does.

### Text density

The reported "too much text" is real and partly self-inflicted:
[`VISUALDESIGN.md`](VISUALDESIGN.md) §2 assigns the Android app "full flavour"
voice, which pushes toward *more* words, on top of existing explanatory copy.

The rule that actually holds the line: **a settings row gets a label and nothing
else.** Explanation lives behind an ⓘ that opens a bottom sheet. If a row needs a
paragraph to be usable, the control is wrong — fix the control.

"Learn more" links are a reasonable pattern but degrade into a second body of prose
nobody reads. Prefer deleting the sentence.

---

## 3. Security and permissions

**Verified:** 15 `uses-permission` entries. That is a lot to justify at install
time, and the app currently has no per-feature rationale flow.

- Request Bluetooth, audio and location **at the moment a bridge is enabled**, never
  at launch, each with one sentence of rationale before the system dialog.
- On API 31+, `BLUETOOTH_SCAN` with `android:usesPermissionFlags="neverForLocation"`
  avoids the location grant entirely. Take it — "this messaging app wants your
  location" is an install-killer, and the mesh does not need coordinates.
- `RECORD_AUDIO` only when the audio modem is actually started.

**On Doze:** do not fight it. A generic review will suggest
`setExactAndAllowWhileIdle` and asking users to disable battery optimisation. For
this protocol both are the wrong instinct — SPORE is *store-and-forward*, and
missing a fifteen-minute window is normal operation, not failure. Catching up is the
design. Say so in the UI rather than burning battery to pretend otherwise.

**Backup and recovery UX.** Since a lost prekey ring means a permanently unreadable
inbox, the app should show ring health — count, oldest entry, next expiry — and
prompt for an encrypted export. Note the tension honestly in that flow: an export
is a copy, and a copy is precisely what defeats forward secrecy. Users should
understand they are choosing recoverability over the seven-day window, because they
are.

---

## 4. Launch roadmap

1. ~~**`allowBackup="false"` + `EncryptedSharedPreferences`.**~~ Done. Everything
   else was cosmetic next to shipping identity keys to Google Drive.
2. ~~**The chat and feed rewrite.**~~ Done — bubbles, alignment, fragment status,
   markdown, inline images, Compose Post.
3. **Bridge lifecycle** — a `nativeStopBridge` on the JNI side, a real
   `data class Bridge(id, kind, detail, enabled)` model, `SwipeToDismissBox` and a
   trailing `CrateSwitch` in the UI. Without it the app accumulates dead bridges
   until reinstall. `CrateSwitch` already exists in `Chrome.kt` waiting for it.
4. **`FileProvider` + `ACTION_VIEW`.** Received files that cannot be opened make
   file sharing a demo. Feed images render inline now; a received PDF still cannot
   be opened.
5. **Reactions**, then audio/video previews — the latter needs ExoPlayer, which is
   a dependency decision, not a UI task.
6. **Run it on a device.** Everything above is compile-and-reason; see §5.
7. **Battery instrumentation.** Battery Historian, 12 hours, all bridges up, before
   any optimisation. Then a soak run for the JNI reference question, since that
   failure mode is an abort under sustained load rather than a slow leak.

---

## 5. What this audit did not cover

Listed so it is not mistaken for a clean bill:

- No soak test was run. Memory and reference-leak claims are structural readings.
- No battery measurement. The WebView hypothesis is untested.
- `BleBridges.kt`, `WifiDirectBridge.kt` and `AudioBridge.kt` were not read
  line-by-line; bridge event-loop races remain open (task #26).
- No review of Compose recomposition cost in the message list, which matters once
  the chat rewrite lands.
- **Nothing here was run on a device, or even compiled locally** — there is no
  Android SDK in the environment this was written in, so the only build feedback is
  what `android.yml` reports after a push. The manifest that did not parse (§0) is
  what that costs, and it is a compile error; a runtime one would have been quieter.
