# SPORE Communicator (Android)

A native SPORE node in your pocket. See [`PLAN.md`](PLAN.md) for the full design.

**Downloads:** [latest rolling build](https://github.com/sloev/spore/releases/tag/rolling)
(freshest, from `master`) · [latest stable release](https://github.com/sloev/spore/releases/latest)
(tagged).

> **Status: M0–M5 (all milestones).** A full node in a foreground service with a
> stable persisted identity, and every planned bridge: **UDP broadcast, TCP,
> audio modem, BT-Meshtastic, BT-Reticulum (RNode), Wi-Fi Direct**, and the
> web-origin bridges (**WebSocket, Nostr, WebTorrent**) via a headless WebView.
> UI: a **Nearby** list of nodes you've heard from, per-peer **conversations**
> with **petnames** and **file sharing**, a microblog **Feed**, live **fragment
> status** both ways, a **Bridges** screen with permission-gated toggles, and an
> **Advanced** screen (identity / seed export / node health). Kawaii green theme
> + a mascot that sparkles when the mesh breathes.
>
> Protocol behaviour: the node **announces itself** (so peers learn its address,
> prekey, a path back, and the **name it announces**), **seals direct messages**
> to a peer's prekey once heard (🔒), and asks for **delivery receipts**
> (✓ delivered). Broadcasts and topic posts are signed but public by nature —
> the UI says so.
>
> **Connect (👋)** shows an invite QR carrying your address, your name and the
> relay/swarm bridges you're on; a friend scans (or pastes) it, confirms a
> petname — prefilled with the name you announce — and can opt in to your
> bridges. Invites are unauthenticated by nature, so the name is shown as a
> claim and bridges are never joined without a tick.
>
> **Files** ride the protocol's own manifest + chunk layer — a signed manifest
> (magnet) names fountain-coded chunks any relay can carry and serve, so a large
> file survives lossy links and resumes rather than restarting. Past ~93 KB the
> manifests nest into a tree, so file size is bounded by what the phone will
> store (64 MB by default, ~30 MB per file), not by the wire format. To a known
> peer a file is **sealed**: contents *and* file name, so relays carrying the
> chunks learn neither. Sealing is per chunk, and the core writes a received file
> straight to disk as it decrypts, so the bytes never pass through the JVM heap.
>
> The Rust core + JNI are host-`cargo check`ed and unit-tested in CI; the
> Kotlin/Compose app is proven to build by the `android` CI workflow. The
> hardware-dependent paths (radios, BLE, mic, live peers) are honest templates —
> verify them with [`docs/HARDWARE.md`](../docs/HARDWARE.md).

## Layout

```
android/
  PLAN.md      # design + milestones
  jni/         # Rust crate: `spore` + `jni` → libspore_jni.so (built by cargo-ndk)
  app/         # Kotlin + Jetpack Compose app (foreground service, UI)
```

The `jni` crate is its own Cargo workspace, so the root `spore` build ignores it.
It calls only the public core API plus `Hub::set_delivery_sink` / `Hub::send`
(added for embedders); nothing in the frozen v1.0 surface changed.

## Build locally

Prerequisites: Android SDK + NDK, a JDK 17, Rust, and
[`cargo-ndk`](https://github.com/bbqsrc/cargo-ndk) (`cargo install cargo-ndk`).

```sh
# 1) cross-compile the native lib into the app's jniLibs
cd android/jni
cargo ndk -t arm64-v8a -t armeabi-v7a -t x86_64 \
  -o ../app/src/main/jniLibs build --release

# 2) assemble the APK (debug-signed)
cd ..
gradle :app:assembleDebug          # -> app/build/outputs/apk/debug/app-debug.apk
```

Install the APK on a phone, and run a second peer (another phone, or the desktop
daemon `spore broadcast`) on the same Wi-Fi to see messages flood over UDP.

Just checking the Rust side (no Android toolchain needed):

```sh
cd android/jni && cargo check
```

## CI + releases

`.github/workflows/android.yml` builds the `.so`s with cargo-ndk and assembles the
APK. Releases come two ways:

- **Rolling** — every push to `master` updates a pre-release at the moving
  `rolling` tag, versioned **`<current version>+<date>`** (current version = the
  latest git tag; date = build date), e.g. `v0.1+2026.07.24`. This is always the
  newest master build.
- **Tagged** — push a `v*` tag to cut a stable release at that tag (the next
  current version). Rolling builds then version from it.

PRs build a debug APK artifact only. `versionCode` is `YYYYMMDD` (monotonic).
Release builds stay **debug-signed** until a release keystore is added to repo
secrets (`ANDROID_KEYSTORE_BASE64`, `ANDROID_KEY_ALIAS`, `ANDROID_KEYSTORE_PASS`,
`ANDROID_KEY_PASS`).
