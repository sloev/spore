#!/usr/bin/env python3
"""Inventory every `unsafe` in the tree, as a diffable snapshot.

    python3 scripts/unsafe_snapshot.py            # print the snapshot
    python3 scripts/unsafe_snapshot.py --check    # compare against the committed one

**Why a snapshot and not a count.** A count tells you something changed; it does
not tell you what, and a reviewer looking at "73 -> 74" has to go and find it.
This keys each site on its file and its enclosing symbol, so the diff *is* the
review: a new line is a new place that can cause undefined behaviour, named.

**Why not line numbers.** They churn on every unrelated edit above them, so a
snapshot keyed on them would be re-blessed constantly and would stop being read —
which is the failure mode that makes a guard worse than nothing.

Three kinds are distinguished, because they carry very different risk:

  * `extern-decl` — a declaration of something the host provides. Cannot itself
    misbehave; the obligation is on the host, and the count matters mainly as a
    measure of the ABI surface.
  * `unsafe-fn` — a function whose *callers* must uphold something. The
    interesting number, because the obligation is spread out.
  * `block` — an `unsafe { }` inside a safe function. The classic one: this is
    where a wrong assumption becomes undefined behaviour.
"""
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SNAPSHOT = os.path.join(ROOT, "audits", "unsafe-snapshot.txt")

# Directories that are their own crates or vendored; each has its own budget and
# is inventoried separately if it ever grows one.
SCAN = ["src"]

FN_RE = re.compile(r"\bfn\s+([A-Za-z_][A-Za-z0-9_]*)")


def enclosing_fn(lines, i):
    """The nearest `fn` at or above line `i` — the symbol a reader would look up."""
    for j in range(i, -1, -1):
        m = FN_RE.search(lines[j])
        if m:
            return m.group(1)
    return "<module>"


def collect():
    out = []
    for base in SCAN:
        for dirpath, _, files in os.walk(os.path.join(ROOT, base)):
            for f in sorted(files):
                if not f.endswith(".rs"):
                    continue
                path = os.path.join(dirpath, f)
                rel = os.path.relpath(path, ROOT)
                lines = open(path, encoding="utf-8").read().split("\n")
                for i, ln in enumerate(lines):
                    if "unsafe" not in ln:
                        continue
                    # A string or a comment mentioning unsafe is not unsafe code.
                    code = ln.split("//")[0]
                    if "unsafe" not in code:
                        continue
                    if "unsafe extern" in code:
                        kind = "extern-decl"
                    elif re.search(r"\bunsafe\s+fn\b", code):
                        kind = "unsafe-fn"
                    elif re.search(r"\bunsafe\s*\{", code):
                        kind = "block"
                    else:
                        kind = "other"
                    out.append(f"{rel}\t{kind}\t{enclosing_fn(lines, i)}")
    # Sorted and de-duplicated: several `unsafe {}` in one function are one fact
    # about that function, and counting them separately would make the snapshot
    # churn on refactors that move nothing across the boundary.
    return sorted(set(out))


def render(rows):
    kinds = {}
    for r in rows:
        kinds[r.split("\t")[1]] = kinds.get(r.split("\t")[1], 0) + 1
    head = [
        "# Every `unsafe` in src/, keyed on file and enclosing symbol.",
        "# Regenerate: python3 scripts/unsafe_snapshot.py > audits/unsafe-snapshot.txt",
        "#",
        "# A new line here is a new place that can cause undefined behaviour. It is",
        "# meant to be reviewed, not re-blessed.",
        "#",
        f"# totals: " + ", ".join(f"{k}={v}" for k, v in sorted(kinds.items())),
        "",
    ]
    return "\n".join(head + rows) + "\n"


def main():
    rows = collect()
    text = render(rows)
    if "--check" not in sys.argv:
        sys.stdout.write(text)
        return 0
    if not os.path.exists(SNAPSHOT):
        print("UNSAFE-SNAPSHOT FAIL — audits/unsafe-snapshot.txt is missing")
        return 1
    have = open(SNAPSHOT, encoding="utf-8").read()
    if have == text:
        n = len(rows)
        print(f"UNSAFE-SNAPSHOT OK — {n} sites, unchanged")
        return 0
    import difflib

    print("UNSAFE-SNAPSHOT FAIL — the inventory moved. Review each line, then:")
    print("  python3 scripts/unsafe_snapshot.py > audits/unsafe-snapshot.txt")
    print()
    for line in difflib.unified_diff(
        have.split("\n"), text.split("\n"), "committed", "working tree", lineterm=""
    ):
        print(f"  {line}")
    return 1


if __name__ == "__main__":
    sys.exit(main())
