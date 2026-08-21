#!/usr/bin/env python3
"""Update the vendored copy of Hardbrut.kt from the pinned remote.

    python3 android/hardbrut-sync.py

Pulls supernihil/hardbrut's official Compose port and writes
android/app/src/main/kotlin/org/spore/node/vendor/Hardbrut.kt, rewriting only
its placeholder `package com.example.hardbrut` line to fit SPORE's package
structure. The build itself never fetches — Gradle just compiles the
committed file — so CI stays deterministic and offline, the same contract as
web/hardbrut-sync.mjs for the CSS side.
"""
import re
import urllib.request

REMOTE = "https://supernihil.github.io/hardbrut/Hardbrut.kt"
OUT = "android/app/src/main/kotlin/org/spore/node/vendor/Hardbrut.kt"
PACKAGE = "org.spore.node.vendor"
HEADER = (
    f"// Vendored, byte-for-byte from {REMOTE} —\n"
    "// do not hand-edit. Re-pull with `python3 android/hardbrut-sync.py`; the only\n"
    "// change on the way in is this header and the `package` line below.\n"
    "//\n"
)


def main():
    with urllib.request.urlopen(REMOTE) as resp:
        src = resp.read().decode("utf-8")

    new_src, n = re.subn(r"^package .+$", f"package {PACKAGE}", src, count=1, flags=re.M)
    if n != 1:
        raise SystemExit("hardbrut-sync: could not find a `package` line to rewrite")

    out = HEADER + new_src
    with open(OUT, "w", encoding="utf-8") as f:
        f.write(out)
    print(f"vendored {OUT} ({len(out)} bytes) from {REMOTE}")


if __name__ == "__main__":
    main()
