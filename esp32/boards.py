#!/usr/bin/env python3
"""Query boards.toml — the supported-device manifest.

Used by build.sh (so the shell never parses TOML) and by
scripts/check_docs_sync.py (so the README table cannot drift from the manifest).

    python3 boards.py --list
    python3 boards.py --resolve s2mini        -> esp32s2mini
    python3 boards.py --field s2mini target   -> xtensa-esp32s2-espidf
    python3 boards.py --table                 -> the README's markdown table
"""
import argparse
import pathlib
import sys
import tomllib

MANIFEST = pathlib.Path(__file__).with_name("boards.toml")


def load():
    with MANIFEST.open("rb") as fh:
        return tomllib.load(fh)


def resolve(doc, want):
    """A board key, one of its aliases, or empty for the default."""
    boards = doc["boards"]
    if not want:
        for key, board in boards.items():
            if board.get("default"):
                return key
        sys.exit("boards.toml declares no default board")
    for key, board in boards.items():
        if want == key or want in board.get("aliases", []):
            return key
    sys.exit(f"unknown board {want!r} — try --list")


def fmt_bt(value):
    return "**none**" if value == "none" else value


def table(doc):
    """The markdown table in README.md, generated so it cannot go stale."""
    rows = [
        "| Board | Target | Wi-Fi | Bluetooth | Native USB | SRAM |",
        "|---|---|---|---|---|---|",
    ]
    for board in doc["boards"].values():
        rows.append(
            "| {name} | `{target}` | {wifi} | {bt} | {usb} | {sram} KB |".format(
                name=board["name"],
                target=board["target"],
                wifi="yes" if board["wifi"] else "no",
                bt=fmt_bt(board["bluetooth"]),
                usb="yes" if board["native_usb"] else "no",
                sram=board["sram_kb"],
            )
        )
    return "\n".join(rows)


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--list", action="store_true")
    ap.add_argument("--table", action="store_true")
    ap.add_argument("--resolve", metavar="BOARD", nargs="?", const="")
    ap.add_argument("--field", nargs=2, metavar=("BOARD", "KEY"))
    args = ap.parse_args()
    doc = load()

    if args.table:
        print(table(doc))
    elif args.list:
        print(f"ESP-IDF {doc['meta']['esp_idf_version']}\n")
        for key, board in doc["boards"].items():
            star = "  (default)" if board.get("default") else ""
            aliases = ", ".join(board.get("aliases", [])) or "-"
            print(f"{key}{star}")
            print(f"  {board['name']} — {board['target']}")
            print(f"  wifi={board['wifi']}  bluetooth={board['bluetooth']}  usb={board['native_usb']}")
            print(f"  aliases: {aliases}")
            if board.get("notes"):
                print(f"  {board['notes']}")
            print()
    elif args.resolve is not None:
        print(resolve(doc, args.resolve))
    elif args.field:
        want, key = args.field
        board = doc["boards"][resolve(doc, want)]
        value = board.get(key, "")
        print(value if not isinstance(value, bool) else str(value).lower())
    else:
        ap.print_help()


if __name__ == "__main__":
    main()
