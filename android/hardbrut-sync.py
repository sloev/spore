#!/usr/bin/env python3
"""Update the vendored copy of Hardbrut.kt from the pinned remote.

    python3 android/hardbrut-sync.py

Pulls supernihil/hardbrut's official Compose port and writes
android/app/src/main/kotlin/org/spore/node/vendor/Hardbrut.kt, rewriting only
its placeholder `package com.example.hardbrut` line to fit SPORE's package
structure. The build itself never fetches — Gradle just compiles the
committed file — so CI stays deterministic and offline, the same contract as
web/hardbrut-sync.mjs for the CSS side.

Two mechanical patches (COMPILE_FIXES below) are applied on the way in,
narrowly and loudly, the same way web/hardbrut-import.mjs strips the
Google-Fonts @import: the file as published does not compile against
AndroidX Compose. `TextStyle` has no `textTransform` parameter in this
Compose BOM — `androidx.compose.ui.text.style.TextTransform` does not
resolve at all — and `HardbrutTextField` calls `BasicTextField`/
`onFocusChanged` without importing either. Remove COMPILE_FIXES and re-run
this script once upstream fixes both; if it no longer applies, this script
fails loudly rather than silently vendoring a broken file.
"""
import re
import urllib.request

REMOTE = "https://supernihil.github.io/hardbrut/Hardbrut.kt"
OUT = "android/app/src/main/kotlin/org/spore/node/vendor/Hardbrut.kt"
PACKAGE = "org.spore.node.vendor"
HEADER = (
    f"// Vendored from {REMOTE} — do not hand-edit.\n"
    "// Re-pull with `python3 android/hardbrut-sync.py`; the only changes on the way\n"
    "// in are this header, the `package` line below, and the two compile fixes for\n"
    "// real upstream bugs the script documents and applies (see its own docstring).\n"
    "//\n"
)

# (find, replace, description). Applied in order; each must match exactly
# once, so an upstream fix or reformat makes the sync fail loudly instead of
# silently reverting to the broken original.
COMPILE_FIXES = [
    (
        "import androidx.compose.ui.text.style.TextTransform\n",
        "",
        "TextTransform: androidx.compose.ui.text.style.TextTransform does not exist "
        "in this Compose BOM (TextStyle has no textTransform parameter) — drop the "
        "import",
    ),
    (
        "                textTransform = TextTransform.Uppercase,\n",
        "",
        "TextTransform: drop the TextStyle argument that used it (HardbrutButton no "
        "longer force-uppercases its content; callers uppercase the label text "
        "themselves)",
    ),
    (
        "import androidx.compose.material3.Text\n",
        "import androidx.compose.foundation.text.BasicTextField\n"
        "import androidx.compose.material3.Text\n"
        "import androidx.compose.ui.focus.onFocusChanged\n",
        "HardbrutTextField calls BasicTextField/onFocusChanged without importing "
        "either — add both",
    ),
]


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
    print(f"vendored {OUT} ({len(out)} bytes) from {REMOTE}, with {len(COMPILE_FIXES)} compile fix(es) applied")


if __name__ == "__main__":
    main()
