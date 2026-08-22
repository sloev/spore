#!/usr/bin/env python3
"""Update the vendored copy of Hardbrut.kt from the pinned remote.

    python3 android/hardbrut-sync.py

Pulls supernihil/hardbrut's official Compose port and writes
android/app/src/main/kotlin/org/spore/node/vendor/Hardbrut.kt, rewriting only
its placeholder `package com.example.hardbrut` line to fit SPORE's package
structure. The build itself never fetches — Gradle just compiles the
committed file — so CI stays deterministic and offline, the same contract as
web/hardbrut-sync.mjs for the CSS side.

COMPILE_FIXES exists for exactly one reason: the file as published sometimes
doesn't compile against AndroidX Compose (three such bugs — a nonexistent
`TextTransform` type and two missing-import cases — were reported upstream
as github.com/supernihil/hardbrut#4 and fixed there; the list below is empty
again as of this comment). If it ever breaks again, add a narrow
(find, replace, description) patch here the same way
web/hardbrut-import.mjs strips the Google-Fonts @import — and report it
upstream too, so the patch is temporary. Each entry must match exactly once;
an upstream fix or reformat makes the sync fail loudly rather than silently
re-vendoring a file that no longer needs it.
"""
import re
import urllib.request

REMOTE = "https://supernihil.github.io/hardbrut/Hardbrut.kt"
OUT = "android/app/src/main/kotlin/org/spore/node/vendor/Hardbrut.kt"
PACKAGE = "org.spore.node.vendor"
HEADER = (
    f"// Vendored from {REMOTE} — do not hand-edit.\n"
    "// Re-pull with `python3 android/hardbrut-sync.py`; the only changes on the way\n"
    "// in are this header, the `package` line below, and any compile fixes for real\n"
    "// upstream bugs the script documents and applies (see its own docstring).\n"
    "//\n"
)

COMPILE_FIXES = []


def main():
    with urllib.request.urlopen(REMOTE) as resp:
        src = resp.read().decode("utf-8")

    new_src, n = re.subn(r"^package .+$", f"package {PACKAGE}", src, count=1, flags=re.M)
    if n != 1:
        raise SystemExit("hardbrut-sync: could not find a `package` line to rewrite")

    for find, replace, desc in COMPILE_FIXES:
        count = new_src.count(find)
        if count != 1:
            raise SystemExit(
                f"hardbrut-sync: compile fix no longer applies cleanly ({count} "
                f"matches, expected 1): {desc}\n"
                "Upstream may have fixed this already — check by hand, then remove "
                "the matching COMPILE_FIXES entry."
            )
        new_src = new_src.replace(find, replace)

    out = HEADER + new_src
    with open(OUT, "w", encoding="utf-8") as f:
        f.write(out)
    fixes = f", with {len(COMPILE_FIXES)} compile fix(es) applied" if COMPILE_FIXES else ""
    print(f"vendored {OUT} ({len(out)} bytes) from {REMOTE}{fixes}")


if __name__ == "__main__":
    main()
