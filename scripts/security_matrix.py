#!/usr/bin/env python3
"""Generate `docs/SECURITY_MATRIX.md` — what has actually been tested, per component.

    python3 scripts/security_matrix.py            # write the page
    python3 scripts/security_matrix.py --check    # fail if it is stale

**Why this exists.** "The parser is audited" and "the protocol is audited" are
very different claims, and a reader with only the first cannot tell them apart.
The parsers here have had real adversarial attention — fuzz targets with seeded
corpora, and bugs they found. Other components have had none. Without somewhere that says
which is which, the strongest claim in the repo silently becomes the claim about
the whole repo.

**Why it is generated.** A hand-maintained maturity table is a claim nobody
re-checks, and it fails in the direction that flatters: a fuzz target is deleted
and its row still says "fuzzed" for a year. Three of the four columns are derived
from the tree:

  - **fuzzed** — each target under `fuzz/fuzz_targets/` declares what it covers in
    a `//! fuzz-covers:` line. The script verifies every named component exists in
    the component list, so a rename breaks the build rather than the table.
  - **property-tested** — a `mod properties` (or `mod unauthenticated_input`) test
    module in the component's source file.
  - **hardware** — rows in `docs/HARDWARE.md`, which is the checklist someone
    actually works through with a radio in their hand.

The fourth, **independently reviewed**, cannot be derived from anything and is
hand-declared below. It is currently "no" for every component, and writing that
down once is worth more than a table that leaves it blank.
"""
import re
import sys
import pathlib
import subprocess

ROOT = pathlib.Path(__file__).resolve().parent.parent
OUT = ROOT / "docs" / "SECURITY_MATRIX.md"

# The components worth reasoning about separately, in rough order of how much of
# an attacker's input each one sees. `src` is the file whose test modules count
# for the property-tested column; `exposure` is the honest one-line answer to
# "what can a stranger make this do?".
COMPONENTS = [
    ("envelope parser", "src/envelope.rs",
     "every byte of every frame, from anyone in range, before any check has run"),
    ("armor + KISS framing", "src/armor.rs",
     "text and serial input, including whatever a user pastes"),
    ("link fragmentation", "src/linkfrag.rs",
     "attacker-chosen set ids, indices and counts, and it allocates to reassemble"),
    ("fountain reassembly", "src/fountain.rs",
     "a stream of chunks whose count and index come straight off the wire"),
    ("node ingest / forwarding", "src/node/ingest.rs",
     "whole envelopes plus the store and interest tables they grow"),
    ("radio codecs", "src/bridge/audio.rs",
     "demodulated symbols, which are noise-shaped input by definition"),
    ("seal / open", "src/seal.rs",
     "ciphertext the attacker chose, against a key they did not"),
    ("topic key schedule", "src/topic.rs",
     "group roster and rekey material from anyone already in the group"),
    ("double ratchet", "src/ratchet.rs",
     "headers and ciphertext on an established session"),
    ("mix / batching", "src/mix.rs",
     "timing and volume, which is the whole of what it defends"),
    ("store + eviction", "src/store.rs",
     "what to keep under pressure, which is what an attacker attacks by filling it"),
    ("hub / interface table", "src/bridge/hub.rs",
     "concurrent traffic across links, where the failure is a deadlock not a panic"),
]

# Hand-declared, because nothing in the tree can tell us. Keep it honest: a name
# here is a person or firm who looked, not a person who was told about it.
INDEPENDENT_REVIEW = {}  # component -> "who, when"

# Which HARDWARE.md rows exercise which component. Hand-mapped because the
# checklist is written for a human with a radio, not for a parser.
HARDWARE_ROWS = {
    "armor + KISS framing": "4, 5b",
    "radio codecs": "2, 3, 5, 5b",
    "node ingest / forwarding": "1, 2, 3, 4, 5",
}


def fuzz_coverage():
    """component -> [target names], from each target's `fuzz-covers:` line."""
    cov = {}
    tdir = ROOT / "fuzz" / "fuzz_targets"
    targets = sorted(tdir.glob("*.rs"))
    if not targets:
        sys.exit("no fuzz targets found — has fuzz/fuzz_targets moved?")
    for t in targets:
        text = t.read_text()
        m = re.search(r"^//! fuzz-covers:(.+)$", text, re.M)
        if not m:
            sys.exit(
                f"{t.relative_to(ROOT)} has no `//! fuzz-covers:` line.\n"
                "Every target must declare which components it exercises, or the\n"
                "matrix silently under-reports and the gap it exists to show is hidden."
            )
        for name in [c.strip() for c in m.group(1).split(",") if c.strip()]:
            if name not in [c[0] for c in COMPONENTS]:
                sys.exit(
                    f"{t.relative_to(ROOT)} declares coverage of unknown component "
                    f"{name!r}.\nAdd it to COMPONENTS in this script, or fix the typo — "
                    "an unmatched name would otherwise read as 'not fuzzed'."
                )
            cov.setdefault(name, []).append(t.stem)
    return cov


def property_tested(src):
    p = ROOT / src
    if not p.exists():
        sys.exit(f"component source {src} does not exist — the matrix names a file that is gone")
    text = p.read_text()
    mods = re.findall(r"^\s*mod (properties|unauthenticated_input)\b", text, re.M)
    return sorted(set(mods))


def hardware_row_count():
    text = (ROOT / "docs" / "HARDWARE.md").read_text()
    return len(re.findall(r"^\| *[0-9]+[a-z]? *\|", text, re.M))


def build():
    cov = fuzz_coverage()
    n_targets = len(sorted((ROOT / "fuzz" / "fuzz_targets").glob("*.rs")))
    rows = []
    gaps = []
    n_props = 0
    for name, src, exposure in COMPONENTS:
        fz = cov.get(name, [])
        pt = property_tested(src)
        hw = HARDWARE_ROWS.get(name)
        rev = INDEPENDENT_REVIEW.get(name)
        rows.append(
            "| **{n}** | {f} | {p} | {r} | {h} | {e} |".format(
                n=name,
                f="✅ " + ", ".join(f"`{t}`" for t in sorted(fz)) if fz else "⬜",
                p="✅ `" + "`, `".join(pt) + "`" if pt else "⬜",
                r="✅ " + rev if rev else "⬜",
                h="✅ rows " + hw if hw else "⬜",
                e=exposure,
            )
        )
        if pt:
            n_props += 1
        if not fz and not pt:
            gaps.append((name, exposure))

    lines = [
        "<!-- Generated by scripts/security_matrix.py — do not edit by hand. -->",
        "",
        "# Security maturity",
        "",
        "What has actually been tested, per component, and by what. Generated from",
        "the tree rather than maintained by hand: a deleted fuzz target removes its",
        "tick here on the next CI run, which is the failure mode a hand-written",
        "table has no way to catch.",
        "",
        '"The parser is audited" and "the protocol is audited" are very different',
        "claims. This page exists so a reader can tell which one they are being",
        "offered.",
        "",
        "| component | fuzzed | property-tested | independently reviewed | hardware | what an attacker controls |",
        "|---|---|---|---|---|---|",
        *rows,
        "",
        "## How to read a ⬜",
        "",
        "It means *not done*, not *not needed*. Some are deliberate — the mix's",
        "defence is statistical, so a fuzzer that cannot crash it proves little —",
        "but none of them are covered by something else on this page. Where a gap",
        "matters, it is a roadmap row, not a footnote here.",
        "",
        "## The column that cannot be generated",
        "",
        "**Independently reviewed is ⬜ for every component**, and that is the most",
        "important cell in the table. Nobody outside this repository has audited any",
        "of it. Every fault listed in the git history was found by the same people",
        "who wrote the thing — fuzzing, property tests, and a simulator — which is a",
        "real method with a known blind spot: it finds what its authors thought to",
        "look for.",
        "",
        "A name appears in that column when a person or firm has looked, not when",
        "one has been told about it.",
        "",
        "## What this is counted from",
        "",
        f"- **{n_targets} fuzz targets** under `fuzz/fuzz_targets/`, each declaring its",
        "  coverage in a `//! fuzz-covers:` line. A target naming a component that does",
        "  not exist fails this script rather than quietly reading as *not fuzzed*.",
        "- **Property tests** are `mod properties` / `mod unauthenticated_input` in the",
        f"  component's own source file — {n_props} components have one. These were written for",
        "  #278, and the ratchet's and the mix's each turned up a live fault on the way in.",
        f"- **Hardware** is [the manual verification checklist](HARDWARE.md), {hardware_row_count()} rows,",
        "  worked through with real radios and real devices. CI has none of either.",
    ]
    if not gaps:
        lines += [
            "",
            "## Every component has at least one form of coverage",
            "",
            "That is a floor, not a finish. The two columns it is built from find",
            "different things — a fuzzer finds what crashes, a property test finds what",
            "is quietly wrong — and several components have only one of the two. The",
            "empty column is still the whole of the right-hand side of this table.",
        ]
    if gaps:
        lines += [
            "",
            "## Untested components, in the order they worry me",
            "",
            "Neither fuzzed nor property-tested. Listed rather than summarised, because",
            "a count is easy to read past:",
            "",
        ]
        lines += [f"- **{n}** — {e}" for n, e in gaps]
    return "\n".join(lines) + "\n"


def main():
    page = build()
    if "--check" in sys.argv:
        if not OUT.exists() or OUT.read_text() != page:
            print("docs/SECURITY_MATRIX.md is stale — run: python3 scripts/security_matrix.py")
            if OUT.exists():
                subprocess.run(["diff", "-u", str(OUT), "-"], input=page, text=True)
            sys.exit(1)
        print(f"SECURITY MATRIX OK — {len(COMPONENTS)} components, generated page matches the tree")
        return
    OUT.write_text(page)
    print(f"wrote {OUT.relative_to(ROOT)} — {len(COMPONENTS)} components")


if __name__ == "__main__":
    main()
