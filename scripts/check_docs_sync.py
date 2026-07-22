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

if errors:
    print("DOCS-SYNC FAIL — regenerate vectors and update the consumers:")
    print("  cargo run --example gen_vectors > reference/vectors.json")
    for e in errors:
        print("  -", e)
    sys.exit(1)

print("DOCS-SYNC OK — REBUILD.md and the frozen test match the generated vectors")
