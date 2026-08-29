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

save_image() {
  OUT="spore-$BOARD.bin"
  docker_run "$IMAGE" \
    bash -lc "export PATH=/home/esp/.cargo/bin:\$PATH; mkdir -p /tmp/esphome/.cargo
              cargo build --release --target $TARGET \
              && espflash save-image --chip $MCU --merge --skip-padding --partition-table partitions.csv $BIN $OUT"
}

if [ "${1:-}" = "--flash" ]; then
  PORT="${SPORE_PORT:-/dev/ttyACM0}"

  # Flashing runs on the host, not in the container. A chip with native USB is
  # providing the very USB device the flasher talks over, so its ROM bootloader
  # resets that stack partway through the write; the host handles the
  # re-enumeration, while a container's --device binding was resolved once at
  # start and is left pointing at a node that no longer exists.
  # Find esptool, or explain the options and stop. Deliberately does not install
  # anything outside esp32/.venv: distributions that follow PEP 668 (current
  # Debian and Ubuntu) refuse pip into the system Python, and quietly overriding
  # that on someone's behalf is not this script's call to make.
  VENV="$PWD/.venv"
  # Resolved as "<python> -m esptool" rather than a bare `esptool` on PATH,
  # because a pip --user install lives in ~/.local and disappears under sudo —
  # and sudo is exactly what an unwritable port drives you to. Carrying the
  # interpreter and its site directory explicitly survives that.
  ESPTOOL_PY=""
  if [ -x "$VENV/bin/python" ] && "$VENV/bin/python" -c "import esptool" 2>/dev/null; then
    ESPTOOL_PY="$VENV/bin/python"
  elif python3 -c "import esptool" 2>/dev/null; then
    ESPTOOL_PY="$(command -v python3)"
  elif python3 -m venv "$VENV" >/dev/null 2>&1 && "$VENV/bin/pip" install --quiet esptool >/dev/null 2>&1; then
    echo "installed esptool into esp32/.venv"
    ESPTOOL_PY="$VENV/bin/python"
  else
    rm -rf "$VENV"
    cat >&2 <<'MSG'
esptool is needed to flash, and could not be installed automatically.
A venv needs python3-venv, which is packaged separately on Debian/Ubuntu.
Pick whichever you prefer:

  sudo apt install python3-venv        # then re-run this script; it does the rest
  pipx install esptool                 # if you have pipx
  pip3 install --user --break-system-packages esptool

The last one is what the PEP 668 error is warning about. For esptool it is
fairly harmless — nothing in the OS depends on it — but it is your call, which
is why this script will not do it for you.
MSG
    exit 1
  fi

  save_image
  SIZE=$(du -h "$OUT" | cut -f1)
  echo
  echo "wrote esp32/$OUT ($SIZE)"

  # The board sits in download mode only while you hold it there, and a board
  # that is merely running looks identical to one that failed to flash — so say
  # exactly which buttons, per board, rather than leaving it to be guessed.
  echo
  echo "── Put the board in download mode ─────────────────────────"
  echo "   $(python3 "$B" --field "$BOARD" name) has two buttons: $(python3 "$B" --field "$BOARD" reset_buttons)"
  echo "   $(python3 "$B" --field "$BOARD" bootloader_steps)"
  echo

  # Waiting beats failing: the port disappears while the board resets and comes
  # back a moment later, so a flash started too eagerly dies on a missing node.
  printf "Waiting for %s " "$PORT"
  for _ in $(seq 1 60); do
    [ -e "$PORT" ] && break
    printf "."; sleep 1
  done
  echo
  [ -e "$PORT" ] || { echo "still nothing at $PORT — set SPORE_PORT if it enumerates elsewhere" >&2; exit 1; }

  # 660 root:dialout is the usual mode, and a desktop login is often not in that
  # group. Say so before esptool fails with a bare permission error.
  # A port at root:dialout 660 that your login is not in is the normal case on a
  # fresh machine, not an error to work around. Say how to fix it and stop —
  # reaching for sudo here would mean sudo forever, and would drag in the whole
  # mess of a pip --user esptool that root cannot see.
  if [ ! -w "$PORT" ]; then
    PORT_GROUP="$(stat -c '%G' "$PORT")"
    cat >&2 <<MSG

$PORT is $(stat -c '%U:%G mode %a' "$PORT") and you are not in '$PORT_GROUP'.

Add yourself to it — newgrp applies it to this shell, so no logout:

  sudo usermod -aG $PORT_GROUP $USER && newgrp $PORT_GROUP

Then re-run this, and flashing will not need root again.
MSG
    exit 1
  fi

  echo
  "$ESPTOOL_PY" -m esptool --chip "$MCU" --port "$PORT" write-flash 0x0 "$OUT"

  echo
  echo "── Done ───────────────────────────────────────────────────"
  echo "   Tap RST to leave download mode and start it, then check it:"
  echo
  echo "     ./esp32/diagnose.py --reset"
  echo
  echo "   That verifies identity, signing, ticking and heap, and prints a"
  echo "   verdict. Add --monitor to stay attached as a terminal afterwards."
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
              && espflash save-image --chip $MCU --merge --skip-padding --partition-table partitions.csv $BIN $OUT"
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
