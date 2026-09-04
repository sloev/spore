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
import subprocess
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
    for i, ln in enumerate(open(f, encoding="utf-8", errors="replace"), 1):
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
bridge_infra = {"driver", "hub", "neighbors", "mod", "csma", "kiss_stream", "foldersync",
                "stream_link", "serial"}
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

# 4. the supported-board table in esp32/README.md is generated from
# esp32/boards.toml, which is the single source of truth for target triples,
# Docker images and radio capability. A board added to the manifest but not the
# README (or edited in the README by hand) drifts silently otherwise, and the
# thing it drifts about — which chip has Bluetooth — is exactly the sort of
# detail someone plans a milestone around.
boards_py = os.path.join(root, "esp32", "boards.py")
readme_path = os.path.join(root, "esp32", "README.md")
if os.path.exists(boards_py) and os.path.exists(readme_path):
    generated = subprocess.run(
        [sys.executable, boards_py, "--table"], capture_output=True, text=True, check=True
    ).stdout.strip()
    readme = open(readme_path, encoding="utf-8").read()
    if generated not in readme:
        errors.append(
            "esp32/README.md: board table does not match esp32/boards.toml — "
            "regenerate with: python3 esp32/boards.py --table"
        )

# 5. every bridge the Android app offers is documented, and none of them is
# still marked planned. The app puts a real, tappable control on screen for each
# of these, so a label with no BRIDGES.md entry means the phone advertises a
# capability nothing describes, and a label whose entry says "⚪ planned" is the
# fake UI the hard rules forbid. Found the TCP bridge shipping in the app with no
# entry of its own — only passing mentions elsewhere.
#
# The map is the point: adding a bridge to the app without adding it here fails,
# which is the moment to ask whether it is documented at all.
ANDROID_BRIDGES = {
    "Audio modem": "audio",
    "Meshtastic BLE": "meshtastic",
    "RNode BLE": "reticulum",
    "TCP": "tcp",
    "UDP broadcast": "udp",
    "Web": "websocket",  # hosts WebSocket / Nostr / WebTorrent in one WebView
    "Wi-Fi Direct": "wifi-direct",
}
controller = os.path.join(root, "android/app/src/main/kotlin/org/spore/node/NodeController.kt")
if os.path.exists(controller):
    kt = open(controller, encoding="utf-8").read()
    offered = set(
        re.findall(r'(?:upsertBridgeRow\(existingId, |addBridgeState\()"([^"]+)"', kt)
    )
    for label in sorted(offered - set(ANDROID_BRIDGES)):
        errors.append(
            f"NodeController.kt offers a {label!r} bridge that ANDROID_BRIDGES "
            "does not map — add it there and document it in docs/BRIDGES.md"
        )
    # Split on the anchors so each entry's own Status row is what gets read.
    sections = re.split(r'<a id="([a-z0-9-]+)"></a>', bridges)
    status = {}
    for i in range(1, len(sections), 2):
        m = re.search(r"\|\s*Status\s*\|([^|]*)\|", sections[i + 1])
        if m:
            status[sections[i]] = m.group(1).strip()
    for label in sorted(offered & set(ANDROID_BRIDGES)):
        anchor_id = ANDROID_BRIDGES[label]
        if anchor_id not in status:
            errors.append(
                f"docs/BRIDGES.md: the app offers {label!r} but has no <a id=\"{anchor_id}\"> entry with a Status row"
            )
        elif status[anchor_id].startswith("⚪"):
            errors.append(
                f"docs/BRIDGES.md: the app offers {label!r} but {anchor_id} is '⚪ planned' — "
                "a control with no backend is the fake UI the hard rules forbid"
            )

# ---------------------------------------------------------------------------
# 6. Every §-reference resolves to a section SPEC.md actually has.
#
# 305 of these are scattered across docs/ and src/ doc comments, and nothing
# used to check them: renaming or renumbering a spec section left every citation
# silently wrong, since a §ref is prose, not a link. Six were already dangling
# when this check was written — §10.3 (three times, including in
# src/congestion.rs), §10c and §10d cited a lettered subsection of §10 that has
# never existed, and §0.2 pointed into the retired VISUALDESIGN.md's numbering.
#
# The valid set is derived from SPEC.md rather than hardcoded, so it stays
# correct as the spec changes: `## N.` and `### N.M` headings, plus the ordered
# rules inside a section (§5.1..§5.7) and their lettered sub-rules (§5.4a..d).
spec = read("docs/SPEC.md")
valid, cur, rule = set(), None, None
for line in spec.split("\n"):
    h = re.match(r"^## (\d+)\.", line)
    if h:
        cur = h.group(1); valid.add(f"§{cur}"); continue
    sub = re.match(r"^### (\d+)\.(\d+)", line)
    if sub:
        valid.add(f"§{sub.group(1)}.{sub.group(2)}"); continue
    if cur:
        item = re.match(r"^(\d+)\. ", line)
        if item:
            rule = item.group(1)
            valid.add(f"§{cur}.{rule}")
        # Lettered sub-rules wrap onto continuation lines, so keep attributing
        # them to the rule most recently opened rather than to one line of it.
        if rule:
            for letter in re.findall(r"\*\*\(([a-z])\)\*\*", line):
                valid.add(f"§{cur}.{rule}{letter}")

scan = sorted(glob.glob(os.path.join(root, "docs", "*.md"))
              + glob.glob(os.path.join(root, "src", "**", "*.rs"), recursive=True))
for f in scan:
    rel = os.path.relpath(f, root)
    for n, line in enumerate(open(f, encoding="utf-8", errors="ignore"), 1):
        # A §ref explicitly attributed to another document is that document's
        # numbering, not SPEC's — VISUALDESIGN.md is retired but still cited by
        # name in the roadmap's history.
        if "VISUALDESIGN" in line:
            continue
        for ref in re.findall(r"§\d+(?:\.\d+[a-z]?|[a-z])?", line):
            if ref not in valid:
                errors.append(f"{rel}:{n}: cites {ref}, which docs/SPEC.md does not have")

# ---------------------------------------------------------------------------
# 7. A bridge's index-table emoji agrees with its own Status row.
#
# BRIDGES.md states each bridge's maturity twice — once in the index tables near
# the top, once in the deep dive's `| Status |` row — and nothing kept them in
# step. Nostr was advertised 🟡 in the index while its own entry said "🧪
# implemented (JS)", and Wi-Fi Direct likewise. The index is what a reader scans
# first, so a stale emoji there oversells or undersells the bridge before they
# reach the detail.
#
# An anchor may legitimately carry several emoji when one entry documents several
# pipes (Meshtastic is ✅ over Wi-Fi-UDP and 🧪 over serial/BLE), so the rule is
# containment, not equality: every emoji the index uses for an anchor must appear
# in that anchor's Status row.
EMOJI = "✅🧪🟡⚪"
index_use = {}
for m in re.finditer(r"\[\s*[^\]\[]*?\s*([%s])\s*\]\(#([a-z0-9-]+)\)" % EMOJI, bridges):
    index_use.setdefault(m.group(2), set()).add(m.group(1))
for m in re.finditer(r'<a id="([a-z0-9-]+)"></a>(.*?)(?=<a id="|\Z)', bridges, re.S):
    anchor_id, body = m.group(1), m.group(2)
    row = re.search(r"\|\s*Status\s*\|([^|]*)\|", body)
    if not row:
        continue
    declared = set(re.findall("[%s]" % EMOJI, row.group(1)))
    for used in sorted(index_use.get(anchor_id, set())):
        if used not in declared:
            errors.append(
                f"docs/BRIDGES.md: the index shows {used} for #{anchor_id}, but its "
                f"Status row says {''.join(sorted(declared)) or '(no emoji)'}"
            )

if errors:
    print("DOCS-SYNC FAIL — regenerate vectors and/or fix terminology:")
    print("  cargo run --example gen_vectors > reference/vectors.json")
    for e in errors:
        print("  -", e)
    sys.exit(1)

print("DOCS-SYNC OK — REBUILD.md and the frozen test match the generated vectors")
