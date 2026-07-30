# Android device-test checklist

CI (`android.yml`) proves the APK **builds**. It cannot prove anything that needs
a real device: that the identity actually survives a reinstall, that it is really
absent from a cloud/adb backup, that the node stays up for two days without the
native side aborting. Those are the rows below.

Each row is independent. Run the ones you can, and append a dated line to the
**History** section (device, Android version, APK commit, result). A row with no
history is unverified — treat the code as faithful but unproven on hardware, the
same convention [`docs/HARDWARE.md`](../docs/HARDWARE.md) uses for the radio paths.

> These are the app/OS-integration tests. The bridge/radio air-interface tests
> (UDP LAN, audio modem, Meshtastic, RNode, Wi-Fi Direct, NFC, …) live in
> `docs/HARDWARE.md` — this file does not duplicate them.

| # | Test | Setup | Pass looks like |
|---|------|-------|-----------------|
| 1 | **Fresh install** | Install the APK on a device with no prior SPORE data | App starts; the node comes up and shows its 16-hex address on the main screen; a bridge (UDP) reports `on` |
| 2 | **Upgrade from prior APK** | Install an older APK, note the address, install the new APK over it (no uninstall) | Same address after upgrade; Advanced → reveal seed returns the same seed; followed topics and petnames survive |
| 3 | **Reveal seed** | Advanced → reveal seed, twice | Both reveals show the identical 32-byte hex; the value matches what `nativeSeed` persisted (one accessor, no second copy) |
| 4 | **Backup exclusion — adb** | With the node up, run `adb backup -f spore.ab org.spore.node` (or `-noapk`); unpack with [`abe`](https://github.com/nelenkov/android-backup-extractor) or `dd`+`zlib` and grep the payload | **The seed and prekey-ring bytes are absent.** The `secretPrefs` file and the `store/` spill dir must not appear. (Backed by `android:allowBackup` / `fullBackupContent` / `dataExtractionRules` — this test proves the manifest actually excludes them.) |
| 5 | **Backup exclusion — cloud / device transfer** | Trigger a Google cloud backup or a device-to-device transfer that includes the app | On the restored/target device the app starts with a **new** address — spore prefs and store did not travel. (If the identity reappears, backup exclusion is broken.) |
| 6 | **24–48 h soak** | Leave the app running with several bridges up (UDP + BLE + audio), screen off, on charger | No native crash (`adb logcat` shows no `SIGABRT`/`SIGSEGV` from `libspore_jni`); the node still relays and delivers at the end; memory has not climbed without bound (`adb shell dumpsys meminfo org.spore.node` before/after) |
| 7 | **7-day forward-secrecy window** | Seal mail to the node, wait past the 7-day prekey lifetime (or fast-forward device clock in a throwaway profile), attempt to open old sealed mail after a prekey rotation | Mail sealed to a rotated-out prekey no longer opens; skipped ratchet keys older than 7 days are gone (PR0). Document the exact procedure and the observed result here |

## Why these and not more

The rows map to claims the project makes that only a device can settle:

- **Rows 1–3** — the identity is real, single-sourced, and persistent (not a fresh
  key each launch, not a second copy drifting out of sync).
- **Rows 4–5** — the **security** claim that the seed and prekey secrets never leave
  the device. This is the one most worth running: a passing build says nothing about
  whether the backup manifest actually holds.
- **Row 6** — the JNI handle lifecycle and the foreground-service teardown (PR3) hold
  under sustained load, where a use-after-free would surface as an abort, not a leak.
- **Row 7** — forward secrecy is bounded in time, not merely asserted (prekey 7-day
  lifetime; ratchet skipped-key TTL, PR0).

## History

*(none recorded yet — this checklist ships ahead of a hardware run; append results as
rows are verified)*
