#!/usr/bin/env python3
"""Guarantee docs and the frozen test can't desync from the code.

The concrete cross-language values live in one generated place —
`reference/vectors.json` (emitted by `cargo run --example gen_vectors`, and
diff-checked in CI). This script asserts that every consumer of those values
reproduces them verbatim:

  * docs/REBUILD.md   — the reimplementation guide's worked hex
  * tests/api_freeze.rs — the frozen 1.0 contract constants

If the code changes the wire format, the vectors change; the frozen test and this
check then fail until the docs are updated to match. Prose is still reviewed by
humans, but no documented *value* can silently rot.

It also keeps the bridge reference honest: every transport in `web/transports/`
and every runnable bridge in `src/bridge/` must be documented in `docs/BRIDGES.md`,
and every file path or `bridge::` module that document cites must exist. A bridge
and its description therefore cannot drift apart without failing the build.

Run: python3 scripts/check_docs_sync.py
"""
import json
import os
import sys

root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
vec = json.load(open(os.path.join(root, "reference", "vectors.json")))


def read(p):
    return open(os.path.join(root, p), encoding="utf-8").read()


rebuild = read("docs/REBUILD.md")
frozen = read("tests/api_freeze.rs")

errors = []


def want(text, keys, where):
    for k in keys:
        if vec[k] not in text:
            errors.append(f"{where}: missing {k} = {vec[k][:32]}… (docs/code out of sync)")


# REBUILD.md shows these worked values verbatim (the armor is line-wrapped for
# display, so check it against a whitespace-stripped copy).
want(rebuild, ["pubkey", "addr", "topic_news", "unsigned_wire", "unsigned_id"], "docs/REBUILD.md")
want("".join(rebuild.split()), ["armor"], "docs/REBUILD.md (armor)")
# The frozen contract test hard-codes the full set (its whole point).
want(frozen, ["pubkey", "addr", "topic_news", "unsigned_wire", "unsigned_id", "signed_wire", "signed_id"], "tests/api_freeze.rs")

# Terminology consistency: "shape" means the FIVE medium bindings (spec Page 2).
# The reference's three driver categories are "forms" (dgram/stream/store) and the
# application taxonomy is "service patterns" — so no doc/comment may say "two/three/
# four shapes". Only "five shapes" is allowed.
import glob
import re

shape_re = re.compile(r"\b(two|three|four)\s+shapes?\b", re.IGNORECASE)
scan = glob.glob(os.path.join(root, "**", "*.md"), recursive=True) + glob.glob(
    os.path.join(root, "src", "**", "*.rs"), recursive=True
)
for f in scan:
    if os.sep + "node_modules" + os.sep in f or os.sep + "_site" + os.sep in f:
        continue
    for i, ln in enumerate(open(f, encoding="utf-8"), 1):
        m = shape_re.search(ln)
        if m:
            rel = os.path.relpath(f, root)
            errors.append(f'{rel}:{i}: "{m.group(0)}" — say "form" (dgram/stream/store) or '
                          f'"service pattern"; "shape" is the five Page-2 medium bindings')

# Bridge ⇄ BRIDGES.md sync: a bridge and its documented spec can't drift.
#   1. every transport in web/transports/ (bar helpers) is documented,
#   2. every runnable bridge in src/bridge/ is documented,
#   3. every path or bridge:: module BRIDGES.md cites actually exists.
bridges = read("docs/BRIDGES.md")
docs_dir = os.path.join(root, "docs")

# 1. web transports must appear in the doc (kiss/loopback are shared helpers).
web_helpers = {"kiss.mjs", "loopback.mjs"}
for f in sorted(glob.glob(os.path.join(root, "web", "transports", "*.mjs"))):
    name = os.path.basename(f)
    ref = "web/transports/" + name
    if name not in web_helpers and ref not in bridges:
        errors.append(f"docs/BRIDGES.md: transport {ref} is implemented but not documented "
                      f"(add an index row + deep-dive section)")

# 2. runnable Rust bridges must appear as bridge::<module> (rest of src/bridge is infra).
bridge_infra = {"driver", "hub", "neighbors", "mod", "csma", "kiss_stream", "foldersync"}
for f in sorted(glob.glob(os.path.join(root, "src", "bridge", "*.rs"))):
    mod = os.path.splitext(os.path.basename(f))[0]
    if mod in bridge_infra:
        continue
    src = open(f, encoding="utf-8").read()
    runnable = "pub fn run" in src or re.search(r"impl .*DatagramTransport", src)
    if runnable and ("bridge::" + mod) not in bridges:
        errors.append(f"docs/BRIDGES.md: bridge::{mod} is a runnable bridge but not documented")

# 3a. every bridge::<module> the doc cites must be a real module file.
for mod in sorted(set(re.findall(r"bridge::([a-z][a-z0-9_]*)", bridges))):
    if not os.path.exists(os.path.join(root, "src", "bridge", mod + ".rs")):
        errors.append(f"docs/BRIDGES.md: cites bridge::{mod} but src/bridge/{mod}.rs is missing")
# 3b. every repo-relative link the doc cites must resolve (external/anchors skipped).
for target in re.findall(r"\]\(([^)]+)\)", bridges):
    if target.startswith(("http", "#", "mailto:")):
        continue
    path = target.split("#", 1)[0]
    if path and not os.path.exists(os.path.normpath(os.path.join(docs_dir, path))):
        errors.append(f"docs/BRIDGES.md: broken link to {target} (file not found)")

if errors:
    print("DOCS-SYNC FAIL — regenerate vectors and/or fix terminology:")
    print("  cargo run --example gen_vectors > reference/vectors.json")
    for e in errors:
        print("  -", e)
    sys.exit(1)

print("DOCS-SYNC OK — REBUILD.md and the frozen test match the generated vectors")
