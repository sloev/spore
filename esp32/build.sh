#!/usr/bin/env bash
# Build (and optionally flash) SPORE firmware for a board listed in boards.toml.
#
#   ./build.sh                  # the default board
#   ./build.sh s2mini           # by name or alias
#   ./build.sh s2mini --flash   # build, then flash and monitor over USB
#   ./build.sh --list           # what is supported
#
# The point of going through this script rather than calling cargo directly:
# the target triple, the MCU env var and the Docker image all have to agree, and
# getting them out of step produces failures that look like something else — a
# mismatched image fails at *link* with a missing-linker error long after the
# crate compiled, and a mismatched MCU produces a binary that flashes happily
# and then crashes on boot.
set -euo pipefail

cd "$(dirname "$0")"
MANIFEST=boards.toml

B=./boards.py

field() { python3 "$B" --field "$1" "$2"; }
resolve() { python3 "$B" --resolve "$1"; }

if [ "${1:-}" = "--list" ]; then
  python3 "$B" --list
  exit 0
fi

BOARD=$(resolve "${1:-}")
[ $# -gt 0 ] && shift || true

TARGET=$(field "$BOARD" target)
MCU=$(field "$BOARD" mcu)
IMAGE=$(field "$BOARD" image)
BIN="target/$TARGET/release/spore-esp32"

echo "board=$BOARD target=$TARGET mcu=$MCU"
echo "image=$IMAGE"

# Run as the invoking user so build output is not left root-owned. HOME is
# redirected because the image's own /home/esp is not writable as another uid.
docker_run() {
  docker run --rm -v "$PWD/..":/work -w /work/esp32 \
    -u "$(id -u):$(id -g)" -e HOME=/tmp/esphome \
    -e RUSTUP_HOME=/home/esp/.rustup -e CARGO_HOME=/tmp/esphome/.cargo \
    -e RUSTFLAGS='--cfg espidf_time64' -e CARGO_TERM_COLOR=always \
    -e MCU="$MCU" "$@"
}

if [ "${1:-}" = "--flash" ]; then
  PORT="${SPORE_PORT:-/dev/ttyACM0}"
  [ -e "$PORT" ] || { echo "no board at $PORT — set SPORE_PORT, or check it is in bootloader mode" >&2; exit 1; }

  # The port is usually root:dialout 660 and a desktop login is often not in
  # dialout, so hand the container the device node's *actual* group rather than
  # assuming the name — it is `uucp` on some distributions. This is why flashing
  # does not need sudo.
  DEV_GID="$(stat -c '%g' "$PORT")"

  # --no-stub on chips with native USB (S2, S3, C3): the ROM bootloader is
  # talking to us over USB-CDC provided by the chip itself, and loading the
  # flash stub resets that USB stack. The device re-enumerates, the host makes a
  # new node, and the `--device` passthrough this container started with is now
  # stale — which surfaces as "Communication error while flashing device"
  # immediately after "Using flash stub". Flashing straight from ROM is slower
  # and avoids the reset entirely.
  STUB=""
  case "$(python3 "$B" --field "$BOARD" native_usb)" in
    true) STUB="--no-stub" ;;
  esac

  docker_run -e ESPFLASH_PORT="$PORT" --device="$PORT" --group-add "$DEV_GID" "$IMAGE" \
    bash -lc "export PATH=/home/esp/.cargo/bin:\$PATH; mkdir -p /tmp/esphome/.cargo
              cargo build --release --target $TARGET && espflash flash $STUB --monitor $BIN"
elif [ "${1:-}" = "--image" ]; then
  # Build a flashable .bin instead of flashing directly, for when espflash in a
  # container cannot drive the board — which is the normal case on chips with
  # native USB, since the ROM bootloader resets its own USB stack mid-flash and
  # the container's --device binding goes stale. Flash this from the host, where
  # a re-enumerating device is not a problem:
  #
  #   pip3 install --user esptool
  #   esptool.py --chip esp32s2 --port /dev/ttyACM0 write_flash 0x0 esp32/spore-<board>.bin
  #
  # --merge puts bootloader, partition table and app in one file written at 0x0;
  # --skip-padding keeps it ~600 KB instead of padding out to the full 4 MB.
  OUT="spore-$BOARD.bin"
  docker_run "$IMAGE" \
    bash -lc "export PATH=/home/esp/.cargo/bin:\$PATH; mkdir -p /tmp/esphome/.cargo
              cargo build --release --target $TARGET \
              && espflash save-image --chip $MCU --merge --skip-padding $BIN $OUT"
  echo
  echo "wrote esp32/$OUT — flash it from the host with:"
  echo "  esptool.py --chip $MCU --port ${SPORE_PORT:-/dev/ttyACM0} write_flash 0x0 esp32/$OUT"
else
  docker_run "$IMAGE" \
    bash -lc "export PATH=/home/esp/.cargo/bin:\$PATH; mkdir -p /tmp/esphome/.cargo
              cargo build --release --target $TARGET"
  echo
  docker_run "$IMAGE" sh size-report.sh "$BIN"
fi
