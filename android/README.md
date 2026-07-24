# SPORE for Android

A native SPORE node in your pocket. See [`PLAN.md`](PLAN.md) for the full design.

> **Status: M0–M5 (all milestones).** A full node in a foreground service with a
> stable persisted identity, and every planned bridge: **UDP broadcast, TCP,
> audio modem, BT-Meshtastic, BT-Reticulum (RNode), Wi-Fi Direct**, and the
> web-origin bridges (**WebSocket, Nostr, WebTorrent**) via a headless WebView.
> UI: per-peer **conversations** with **petnames** and **file sharing**, a
> microblog **Feed**, live **fragment status** both ways, a **Bridges** screen
> with permission-gated toggles, and an **Advanced** screen (identity / seed
> export). Kawaii green theme + a mascot that sparkles when the mesh breathes.
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
APK on every android/core change, uploading a **debug APK artifact**. Pushing a
`vYYYY.MM.DD` tag attaches the APK to a GitHub Release. The app **versions itself by
build date** (`versionName = YYYY.MM.DD`, `versionCode = YYYYMMDD`).

Release builds stay **debug-signed** until a release keystore is added to repo
secrets (`ANDROID_KEYSTORE_BASE64`, `ANDROID_KEY_ALIAS`, key/store passwords); the
plan's M5 wires those in.
