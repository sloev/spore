# SPORE for Android

A native SPORE node in your pocket. See [`PLAN.md`](PLAN.md) for the full design.

> **Status: M0 skeleton.** One node in a foreground service, identity persisted,
> the **UDP broadcast** bridge, and a minimal Compose UI (address + send/receive
> log). The Rust JNI layer is host-`cargo check`ed in CI; the Kotlin/Gradle app is
> a first cut proven by the `android` CI workflow. Milestones M1–M5 (more bridges,
> petnames, feed, files, kawaii polish, signed release) are in the plan.

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
