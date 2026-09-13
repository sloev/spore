#!/usr/bin/env python3
"""Every Kotlin `external fun` has a matching JNI symbol, and vice versa.

    python3 scripts/check_jni_symbols.py

**Why this exists.** A Kotlin `external fun nativeFoo(...)` with no
`Java_org_spore_node_SporeNative_nativeFoo` on the Rust side **compiles
perfectly**. Both halves build, the APK builds, every CI job passes, and the app
throws `UnsatisfiedLinkError` the first time that function is called — on a
device, which CI does not have.

The reverse is milder but still worth knowing: a JNI symbol nothing declares is
dead code that looks load-bearing.

This is a text check and deliberately so. The real check would be loading the
library on a device and resolving every symbol, which is `docs/HARDWARE.md`'s
job; this catches the typo before it gets that far, in about a millisecond.
"""
import re
import sys
import pathlib

ROOT = pathlib.Path(__file__).resolve().parent.parent
KOTLIN = ROOT / "android/app/src/main/kotlin/org/spore/node/SporeNative.kt"
RUST = ROOT / "android/jni/src/lib.rs"
PREFIX = "Java_org_spore_node_SporeNative_"


def main():
    for p in (KOTLIN, RUST):
        if not p.exists():
            sys.exit(f"{p.relative_to(ROOT)} is missing — has the Android layer moved?")

    # `external fun name(` — the declaration, not a mention in a doc comment.
    # Comment bodies are stripped first, so prose may name `external fun` freely.
    kt_src = "\n".join(
        line.split("//")[0] for line in KOTLIN.read_text().splitlines()
    )
    kt_src = re.sub(r"/\*.*?\*/", "", kt_src, flags=re.S)
    declared = set(re.findall(r"\bexternal\s+fun\s+(\w+)\s*\(", kt_src))

    defined = set(re.findall(rf"\b{re.escape(PREFIX)}(\w+)\s*\(", RUST.read_text()))

    missing = sorted(declared - defined)
    orphan = sorted(defined - declared)

    if missing:
        print("Kotlin declares these with no JNI symbol behind them.")
        print("Each is an UnsatisfiedLinkError the first time it is called, on a device:")
        for n in missing:
            print(f"  external fun {n}  ->  expected {PREFIX}{n} in android/jni/src/lib.rs")
    if orphan:
        print("\nJNI exports these and nothing declares them — dead code that looks live:")
        for n in orphan:
            print(f"  {PREFIX}{n}")

    if missing or orphan:
        sys.exit(1)

    print(f"JNI SYMBOLS OK — {len(declared)} declared, all matched")


if __name__ == "__main__":
    main()
