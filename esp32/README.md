# SPORE on ESP32-S3

The embedded runtime from [Roadmap](../docs/ROADMAP.md) Milestone 8 — a headless
board that relays envelopes over raw 802.11 and bridges to a phone over USB or
BLE. This directory is currently **E1 only**: the toolchain scaffold and a
bring-up binary that proves the core runs here. The radio, flash store, USB and
BLE halves (E2–E5) are not written yet.

## Why it is its own crate

Same reason as `android/jni`: it declares an empty `[workspace]`, so the parent
`spore` package never adopts it. ESP32-S3 is Xtensa, which upstream Rust does
not target — it needs Espressif's `esp` toolchain fork, and `std` has to be
built from source (`build-std`). Letting the main workspace see this crate would
break `cargo test --all-targets` on every ordinary host.

The core is a plain path dependency (`spore = { path = ".." }`) with **no
ESP-specific features and no `cfg` branches in the core** — a runtime supplies
the four nutrients and otherwise takes the core as-is.

## Building

Everything runs in Espressif's image, so no host toolchain is needed and nothing
is installed outside Docker:

```sh
docker pull espressif/idf-rust:esp32s3_latest
docker run --rm -v "$PWD":/work -w /work/esp32 \
  -u "$(id -u):$(id -g)" -e HOME=/tmp/esphome \
  espressif/idf-rust:esp32s3_latest \
  bash -lc 'export PATH="/home/esp/.cargo/bin:$PATH"; \
            export RUSTUP_HOME=/home/esp/.rustup CARGO_HOME=/tmp/esphome/.cargo; \
            mkdir -p /tmp/esphome/.cargo; cargo build --release'
```

The first build also downloads and compiles ESP-IDF itself, which takes a long
time; later builds reuse it. `HOME` is redirected because the image's own `esp`
user home is not writable when the container runs as your uid.

### Other variants

The default target is ESP32-S3, which is what the roadmap and issue #149 name.
Others build from the same source, but **the target, the `MCU` env var and the
image all have to agree** — each image ships only its own chip's linker, and a
mismatched `MCU` produces a binary that flashes and then crashes:

```sh
# LOLIN S2 Mini and other ESP32-S2 boards
docker run --rm -v "$PWD":/work -w /work/esp32 -e MCU=esp32s2 \
  espressif/idf-rust:esp32s2_latest \
  bash -lc 'cargo build --release --target xtensa-esp32s2-espidf'
```

<!-- generated from boards.toml: python3 esp32/boards.py --table -->
| Board | Target | Wi-Fi | Bluetooth | Native USB | SRAM |
|---|---|---|---|---|---|
| ESP32-S3 | `xtensa-esp32s3-espidf` | yes | BLE 5.0 | yes | 512 KB |
| LOLIN S2 Mini | `xtensa-esp32s2-espidf` | yes | **none** | yes | 320 KB |

The S2 has no Bluetooth radio of any kind — not a driver gap, the hardware is
absent (ESP-IDF's own `soc_caps.h` defines neither `SOC_BT_SUPPORTED` nor
`SOC_BLE_SUPPORTED` for it). The BLE bridge cannot run on an S2; USB is its
tether. Its console also has to be routed over USB CDC rather than UART0, which
`sdkconfig.defaults.esp32s2` does — without it a correctly-booting board looks
completely dead.

## Footprint

`size-report.sh` prints what M8's E1 checkpoint turns on. CI runs it on every
build so growth is visible while it is still cheap to act on:

```sh
docker run --rm -v "$PWD":/work -w /work/esp32 \
  espressif/idf-rust:esp32s3_latest sh size-report.sh
```

It reports flash (`.flash.text` + `.flash.rodata` + the initialisers for
`.dram0.data`) and static internal SRAM (`.iram0.text` + `.dram0.data` +
`.dram0.bss`). Both are **static sections, not live heap** — runtime headroom
is a device-run number the bring-up binary still owes — and the flash figure
excludes the bootloader, partition table, and any `littlefs` partition E3 adds.

Do not read the last digits as exact. The flash total shifts by ~100 bytes
depending on where the build ran, because panic messages embed absolute source
paths and `CARGO_HOME` sitting one directory deeper changes their length. It is
a number to compare across runs, not to quote to the byte.

## Flashing (needs the board)

`espflash` is in the image, but USB passthrough makes this easier from the host:

```sh
cargo install espflash
espflash flash --monitor target/xtensa-esp32s3-espidf/release/spore-esp32
```

## What the bring-up binary does

`src/main.rs` is deliberately the smallest thing that exercises what is most
likely to break on an MCU rather than a demo of the protocol:

| Nutrient | How it is exercised |
|---|---|
| Randomness | `Node::new` draws its seed from `OsRng`, which `getrandom` routes to the hardware TRNG on this target |
| Time | `esp_timer_get_time`, seconds since reset — see the note below |
| Scheduling | a FreeRTOS delay loop calling `Node::tick` |
| Storage | not supplied yet: no spill backend is registered, so the store is memory-only, which is the honest state until E3 |

It then signs one envelope and verifies it, because that is the heaviest crypto
the core does and the most likely thing to overflow a small stack, and prints
free heap before and after so E1's checkpoint has real numbers.

**On the clock.** A board with no RTC battery and no network has no trusted wall
clock at boot, so `now()` here is seconds-since-reset. That is not a bug being
deferred: [Spec](../docs/SPEC.md) §Time already specifies this case — a node with
no trusted clock must not drop on expiry, it relays regardless and ages by dwell.
Adding SNTP later replaces this function and nothing else, because the core never
reads a clock itself; time arrives as a parameter on every call.

## Status

🧪 at best, and only once run on real silicon. Nothing here has been flashed to a
board yet — the build is verified, the behaviour is not. See
[Hardware verification](../docs/HARDWARE.md) for what a real run has to record.
