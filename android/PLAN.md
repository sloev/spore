# SPORE for Android — plan

A pocket SPORE node: a real, always-on node in your hand with all the bridges, and
a simple messenger/feed/file UI that a Meshtastic user feels at home in. Lives in
this monorepo under `android/`.

> Status: **M0–M5 built.** All milestones below are implemented; the Rust core +
> JNI are tested in CI and the Kotlin/Compose app builds via the `android`
> workflow. Hardware-dependent paths are honest templates — see
> [`docs/HARDWARE.md`](../docs/HARDWARE.md). Decisions marked **[choice]** were
> resolved to their recommended defaults (Kotlin+Compose+JNI; headless WebView).

## Goals

- One **full node** running as a background service, not a demo.
- **Every practical bridge**: Wi-Fi Direct, UDP broadcast, TCP, audio modem,
  BT-Meshtastic, BT-Reticulum, BLE mesh, Nostr, WebSocket, WebRTC, WebTorrent.
- A **simple** UI — instant messaging, a feed (microblogging), and file sharing —
  that appeals to Meshtastic users: clean, green-ish Material, advanced options one
  tap away, a little **kawaii**.
- **Petnames** for addresses, and a visible **fragmentation status** on send/receive.
- A **CI-built APK release**, versioned by date.

## Architecture

Because the native bridges (Wi-Fi Direct, UDP broadcast, BLE, TCP, audio) are
impossible from a WebView, the node runs **natively**; a hidden WebView only carries
the web-origin bridges so their existing JS is reused verbatim.

```
┌──────────────────────────────────────────────────────────────┐
│  Compose UI (Kotlin)   messages · feed · files · bridges · ⚙  │
├──────────────────────────────────────────────────────────────┤
│  Foreground Service (Kotlin)                                   │
│    · owns the node · persistent notification · Multicast/Wake  │
│      locks · Doze handling · start-on-boot (opt)               │
├──────────────────────────────────────────────────────────────┤
│  JNI layer  (NEW: `android/jni` crate, or src/android.rs        │
│              #[cfg(target_os="android")])                       │
│    · additive — does NOT touch the frozen C ABI (bindings/…)   │
├──────────────────────────────────────────────────────────────┤
│  SPORE core  (existing Rust lib, frozen v1.0)                  │
│    · Node/Hub · Ed25519 · fountain fragmentation · store       │
├──────────────────────────────────────────────────────────────┤
│  headless WebView  ── web/transports/*.mjs (webrtc, webtorrent, │
│                        websocket, nostr) piped to the node      │
└──────────────────────────────────────────────────────────────┘
```

One `Node`/`Hub`. Native bridges register real ifaces on the hub; Kotlin-mediated
bridges (BLE, audio, Wi-Fi Direct) push inbound bytes over JNI and receive outbound
via a callback; the WebView relays raw envelopes over a `@JavascriptInterface`.

### **[choice] UI + embedding stack**
- **Default: Kotlin + Jetpack Compose + Rust-via-JNI.** Best background service, BLE,
  Wi-Fi Direct, notifications; matches Meshtastic's own app; reuses the frozen C ABI.
- Alt: Flutter + Rust FFI (one codebase, future iOS, but messier native
  background/BLE/Wi-Fi-Direct; adds the Dart toolchain).
- Alt: Web UI in a WebView + native service (max UI reuse, weaker native UX).

## The JNI surface (additive; frozen C ABI untouched)

A thin layer over `bridge::hub` — no core changes. Sketch:

| Function | Purpose |
|---|---|
| `spore_rt_new(seed)` / `spore_rt_free` | create/destroy the runtime (node + hub) from a persisted 32-byte seed (`Node::from_seed`) |
| `spore_rt_addr(out8)` / `spore_rt_seed(out32)` | identity in/out |
| `spore_rt_register_iface() -> iface` | a hub iface for a Kotlin-driven bridge |
| `spore_rt_on_rx(iface, bytes)` | push an inbound frame from Kotlin (BLE/audio/Wi-Fi Direct/WebView) |
| `spore_rt_set_forward_cb(cb)` | outbound frames (per iface) delivered to Kotlin |
| `spore_rt_subscribe(topic)` / `spore_rt_send(dest8, bytes)` | topics + originate; returns fragment count |
| `spore_rt_start_udp(port)` / `spore_rt_start_tcp(target)` | spin the pure-Rust bridges (they run on Android as-is) |
| `spore_audio_modulate(bytes) -> pcm` / `spore_audio_demod(pcm) -> frames` | reuse `bridge::audio` DSP with Kotlin doing AudioRecord/Track |
| file/store: `spore_rt_send_file(...)`, progress callbacks | fountain manifest + chunk progress for the UI |

Built with `cargo-ndk` to `arm64-v8a`, `armeabi-v7a`, `x86_64` `.so`s.

## Bridge implementation map

| Bridge | Where it runs | Notes |
|---|---|---|
| UDP broadcast | Rust `bridge::udp::run_primary` | needs `WifiManager.MulticastLock` + `INTERNET`/`ACCESS_WIFI_STATE` |
| TCP | Rust `bridge::tcp` | connect or listen |
| Audio modem | Rust DSP + Kotlin `AudioRecord`/`AudioTrack` | 48 kHz mono; bit-compatible with `bridge::audio` |
| BT-Meshtastic | Kotlin BLE (Meshtastic service) ↔ Rust `bridge::meshtastic` codec | ToRadio/FromRadio over GATT |
| BT-Reticulum | Kotlin BLE (Nordic UART) ↔ RNode KISS | mirrors `web/transports/reticulum.mjs` |
| BLE mesh | Kotlin BLE-mesh vendor model ↔ node | later milestone |
| Wi-Fi Direct | Kotlin `WifiP2pManager` → Rust UDP on the p2p iface | group owner = soft-AP subnet |
| Nostr | Kotlin OkHttp WS (or the WebView) | kind-30078, tag `spore-v1` |
| WebSocket | WebView transport (or Kotlin OkHttp) | relay/peer |
| WebRTC, WebTorrent | headless WebView (`webrtc.mjs`, `webtorrent.mjs`) | see below |
| NFC / QR seed | Kotlin, later | tap/scan to seed identity or bundle |

### **[choice] Web-origin bridges (WebRTC, WebTorrent)**
- **Default: headless WebView** hosting the existing `web/transports/*.mjs`, piping
  raw envelopes to the native node. Maximum reuse, minimal new code.
- Alt: native Google WebRTC + a tracker client (heavy, reimplements the swarm).
- Alt: defer WebRTC/WebTorrent; ship Nostr + WebSocket natively first.

## UI

Simple-first. The launch screen is a conversation list; everything advanced is one
tap behind a gear. Material 3, a Meshtastic-adjacent green, rounded cards.

- **Messages** — conversations keyed by **petname**; a DM composer; delivery + a
  fragment progress row per outgoing message.
- **Feed** — microblogging: a scroll of signed short posts on topics you follow, with
  a composer that posts to a chosen topic. (SPORE topics + signed envelopes already
  exist in core.)
- **Files** — pick a file → send; a **fragment status bar** (chunk X/N, bytes) on both
  send and receive, driven by the core's fountain manifest + chunk pinning.
- **Bridges** — a list of bridges with on/off toggles, connection state, and live
  peer/traffic counts (the native twin of the web node's bridges panel).
- **Petnames** — a local address book mapping `petname ⇄ 8-byte address`, stored
  encrypted; never on the wire. Assign a petname from any received message.
- **Advanced (⚙)** — node seed/QR export, topics, per-source quota, store budget,
  raw address, and the bridge internals.

### Kawaii
A small **spore/mushroom mascot** with a few moods (idle, relaying, sleeping when
no peers). Soft pastel accents over the Meshtastic green, cute empty states
("no spores nearby yet 🍄"), playful but unobtrusive microcopy, a gentle wiggle when
a relay happens. Tasteful — it should still read as a serious mesh tool.

## Node lifecycle & platform

- **Foreground Service** with a persistent notification ("🍄 SPORE — N peers"), the
  only reliable way to keep networking alive on modern Android.
- `MulticastLock` (UDP broadcast) + partial `WakeLock`; prompt for battery-
  optimisation exemption; optional `RECEIVE_BOOT_COMPLETED` auto-start.
- **Identity**: the 32-byte `Node::seed()` stored in Android Keystore /
  EncryptedSharedPreferences; restored with `Node::from_seed`.
- **Permissions**: `INTERNET`, `ACCESS_WIFI_STATE`/`CHANGE_WIFI_STATE`,
  `NEARBY_WIFI_DEVICES` (Wi-Fi Direct, API 33+), `BLUETOOTH_SCAN`/`_CONNECT`,
  `RECORD_AUDIO` (modem), `POST_NOTIFICATIONS`, `FOREGROUND_SERVICE*`.

## CI + APK releases (rolling + tagged)

New workflow `.github/workflows/android.yml` (a new file — **not** frozen, no label
needed):

- Set up JDK + Android SDK/NDK + Rust + `cargo-ndk`; build the `.so`s; assemble.
- **Rolling** — every push to `master` publishes a **pre-release** at the moving
  `rolling` tag, versioned **`<current version>+<date>`** (current version =
  latest git tag via `git describe`; date = build date), e.g. `v0.1+2026.07.24`.
  Always the newest master build; updated on every merge.
- **Tagged** — pushing a `v*` tag cuts a normal release **at that tag** — the next
  current version, which rolling builds then version from.
- **PRs** build a debug APK artifact only.
- **versionCode** = `YYYYMMDD` (monotonic, ~120 years of headroom); **versionName**
  = the computed name (CI passes `SPORE_VERSION_NAME`/`_CODE`; local builds fall
  back to the date).
- **Signing**: release builds use a keystore from repo secrets
  (`ANDROID_KEYSTORE_BASE64`, `ANDROID_KEY_ALIAS`, `ANDROID_KEYSTORE_PASS`,
  `ANDROID_KEY_PASS`). Until those exist, CI ships a **debug-signed** APK so the
  pipeline is green from day one.

## Monorepo layout

```
android/
  PLAN.md              # this file
  app/                 # Kotlin + Jetpack Compose application module
    src/main/…         # UI, Service, BLE/Wi-Fi-Direct/audio bridges, WebView host
  jni/                 # thin Rust crate: depends on `spore` + `jni`, built by cargo-ndk
    src/lib.rs         # the JNI surface above (#[cfg(target_os="android")])
  gradle*, settings.gradle, build.gradle
```

The core crate is unchanged; `android/jni` depends on it. No frozen file is touched
(the JNI is a new crate; the release workflow is a new file). The core's frozen v1.0
contract and all existing guards stay intact.

## Milestones (all done)

- **M0 ✅ skeleton.** Compose app + `android/jni` cross-compiled via cargo-ndk; node
  in a foreground service; identity persisted; **UDP broadcast**; debug APK from CI.
- **M1 ✅ messaging.** DM-by-petname UI + petname address book + bridges screen +
  **TCP**, plus the generic Kotlin-driven-bridge iface poll API.
- **M2 ✅ radios.** **audio modem**, **BT-Meshtastic**, **BT-Reticulum (RNode)**,
  **Wi-Fi Direct**; `MulticastLock`.
- **M3 ✅ web bridges.** Headless WebView carrying **WebSocket, Nostr, WebTorrent**
  (WebRTC data channels under WebTorrent). Manual copy/paste WebRTC stays a web-node
  feature.
- **M4 ✅ feed + files.** Microblog **Feed** on topics + composer; file send/receive
  (app-layer framing) with **fragment status** both ways (`Node::frag_progress`).
- **M5 ✅ polish + release.** Kawaii green theme + sparkle mascot, an **Advanced**
  screen (identity/seed), and **date-versioned** release APK signing (debug key
  fallback until a keystore is in secrets).

## Interplay with governance

- Nothing here weakens the frozen v1 core: the JNI is additive, the release workflow
  is a new file, and the Android bridges reuse `src/bridge/*` (already covered by the
  docs-sync bridge guard) plus the web transports (already covered).
- The APK is a **release artifact**, never committed.
- Consider extending `scripts/check_docs_sync.py` later so the Android bridge list and
  `docs/BRIDGES.md` can't drift, the same way the web transports are checked.

## Open decisions (defaults chosen above)

1. **UI + embedding stack** — default Kotlin + Compose + JNI.
2. **Web-origin bridges** — default headless WebView.
3. **First step** — commit this plan, then scaffold **M0**.
