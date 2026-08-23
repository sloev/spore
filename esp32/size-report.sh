#!/bin/sh
# Print the firmware's flash and static-RAM footprint — the numbers M8's E1
# checkpoint turns on (see docs/ROADMAP.md). Run inside the Espressif image,
# where readelf understands Xtensa.
#
#   sh esp32/size-report.sh [path-to-elf]
#
# Emits two "key<TAB>bytes" lines, so a caller can read it without parsing prose.
set -eu

BIN="${1:-target/xtensa-esp32s3-espidf/release/spore-esp32}"
[ -f "$BIN" ] || { echo "no such binary: $BIN" >&2; exit 1; }
readelf -h "$BIN" | grep -q Xtensa || { echo "not an Xtensa binary: $BIN" >&2; exit 1; }

# Exact section-name match, not a substring one: .flash.rodata_noload and
# .iram0.text_end would both match a looser pattern and silently inflate the
# totals. The image ships mawk (no strtonum), so hex is converted in the shell.
sz() {
  hex=$(readelf -W -S "$BIN" | sed 's/^ *\[[ 0-9]*\] *//' | awk -v n="$1" '$1 == n { print $5; exit }')
  [ -n "$hex" ] || { echo "section not found: $1" >&2; exit 1; }
  printf '%d' "0x$hex"
}

# Flash carries .flash.text + .flash.rodata + the initialisers for .dram0.data;
# internal SRAM carries .iram0.text + .dram0.data + .bss.
flash=$(( $(sz .flash.text) + $(sz .flash.rodata) + $(sz .dram0.data) ))
ram=$(( $(sz .iram0.text) + $(sz .dram0.data) + $(sz .dram0.bss) ))

printf 'flash\t%d\n' "$flash"
printf 'ram\t%d\n' "$ram"
