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
  docker_run -e ESPFLASH_PORT="$PORT" --device="$PORT" "$IMAGE" \
    bash -lc "export PATH=/home/esp/.cargo/bin:\$PATH; mkdir -p /tmp/esphome/.cargo
              cargo build --release --target $TARGET && espflash flash --monitor $BIN"
else
  docker_run "$IMAGE" \
    bash -lc "export PATH=/home/esp/.cargo/bin:\$PATH; mkdir -p /tmp/esphome/.cargo
              cargo build --release --target $TARGET"
  echo
  docker_run "$IMAGE" sh size-report.sh "$BIN"
fi
