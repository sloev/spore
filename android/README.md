# SPORE for Android

A native SPORE node in your pocket. See [`PLAN.md`](PLAN.md) for the full design.

> **Status: M0 + M1.** A node in a foreground service (identity persisted),
> the **UDP broadcast** and **TCP** bridges, **petnames**, per-peer **conversations**
> with a compose box, and a **Bridges** screen. The Rust core + JNI are
> host-`cargo check`ed in CI; the Kotlin/Compose app is proven by the `android` CI
> workflow (APK build). Milestones M2–M5 (audio/BLE/Wi-Fi-Direct, the WebView web
> bridges, feed + files with fragment status, kawaii polish + signed release)
> follow the plan.

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
