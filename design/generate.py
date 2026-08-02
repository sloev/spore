#!/usr/bin/env python3
"""Generate the design tokens into every surface that renders them.

    python3 design/generate.py

One palette, four consumers: the docs site's CSS, the standalone node's inlined
CSS, Android's Compose `Palette`, and VISUALDESIGN's contrast tables. Each is
written between marker comments; everything outside the markers is left alone,
because these files are mostly hand-written and only their token block is not.

Contrast ratios are **computed here, never typed**. `tokens.json` declares what
each pairing is supposed to be (`body` ≥4.5:1, `large` ≥3:1, or `forbidden`) and
this script checks the declaration both ways: a pair claimed readable that isn't
fails the build, and so does a pair claimed forbidden that has quietly become
readable. That is the whole point — the palette cannot drift away from its own
accessibility claims without CI noticing.

CI runs this and diffs; see the "design tokens in sync" job.
"""
import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)  # so `import specimen` works from any cwd
ROOT = os.path.dirname(HERE)
T = json.load(open(os.path.join(HERE, "tokens.json")))

DARK, LIGHT = T["dark"], T["light"]
CONTRAST = T["contrast"]
SURFACES = T["surfaces"]
METRICS = T["metrics"]
BODY = CONTRAST["thresholds"]["body"]
LARGE = CONTRAST["thresholds"]["large"]


# ----------------------------------------------------------------- WCAG maths
def _channel(c):
    c /= 255.0
    return c / 12.92 if c <= 0.03928 else ((c + 0.055) / 1.055) ** 2.4


def luminance(hex_):
    h = hex_.lstrip("#")
    r, g, b = (int(h[i : i + 2], 16) for i in (0, 2, 4))
    return 0.2126 * _channel(r) + 0.7152 * _channel(g) + 0.0722 * _channel(b)


def ratio(fg, bg):
    """WCAG 2.x contrast ratio. Symmetric, so argument order is a convention."""
    a, b = luminance(fg), luminance(bg)
    hi, lo = max(a, b), min(a, b)
    return (hi + 0.05) / (lo + 0.05)


def grade_of(r):
    return "body" if r >= BODY else ("large" if r >= LARGE else "forbidden")


# ------------------------------------------------------------- token lookup
def _index(entries):
    return {e["n"]: e for e in entries}


DARK_P, DARK_S = _index(DARK["palette"]), _index(DARK["semantic"])
LIGHT_P, LIGHT_S = _index(LIGHT["palette"]), _index(LIGHT["semantic"])


def resolve(name, theme="dark"):
    """Hex for a token name, following `ref` chains and light-mode overrides."""
    pal, sem = (DARK_P, DARK_S) if theme == "dark" else (LIGHT_P, LIGHT_S)
    if name in pal:
        return pal[name]["hex"]
    if name in sem:
        e = sem[name]
        return e["hex"] if "hex" in e else resolve(e["ref"], theme)
    # A light surface may reuse a dark token it never overrides (e.g. kevlar).
    if theme == "light":
        return resolve(name, "dark")
    raise KeyError(f"unknown token: {name}")


def entry(name, theme="dark"):
    pal, sem = (DARK_P, DARK_S) if theme == "dark" else (LIGHT_P, LIGHT_S)
    return pal.get(name) or sem.get(name) or {}


# ------------------------------------------------------------------- checks
def check_contrast():
    """Every declared grade must match the computed one. Fail loudly if not."""
    problems = []
    bases = CONTRAST["bases"]

    for fg, grades in CONTRAST["grades"].items():
        for base, declared in zip(bases, grades):
            r = ratio(resolve(fg), resolve(base))
            actual = grade_of(r)
            if actual != declared:
                problems.append(
                    f"  {fg} on {base}: {r:.2f}:1 is '{actual}', "
                    f"tokens.json declares '{declared}'"
                )

    for chk in CONTRAST["extra"]:
        theme = chk.get("theme", "dark")
        r = ratio(resolve(chk["fg"], theme), resolve(chk["on"], theme))
        actual = grade_of(r)
        if actual != chk["grade"]:
            problems.append(
                f"  {chk['fg']} on {chk['on']} ({theme}): {r:.2f}:1 is "
                f"'{actual}', tokens.json declares '{chk['grade']}'"
            )
        if chk.get("aaa") and r < 7.0:
            problems.append(
                f"  {chk['fg']} on {chk['on']} ({theme}): {r:.2f}:1 is "
                f"below the 7:1 AAA floor it is documented as clearing"
            )

    if problems:
        sys.stderr.write(
            "design tokens: contrast declarations no longer match the hexes.\n"
            + "\n".join(problems)
            + "\n\nEither fix the colour or update tokens.json's grade — but do\n"
            "not ship a palette whose own accessibility claims are false.\n"
        )
        raise SystemExit(1)


# ---------------------------------------------------------------- metrics (C5)
#
# Colour was single-sourced first (C3). These are the other half of the same
# argument: a button height hand-typed in three files drifts exactly the way a
# hex hand-typed in three files drifts, and the drift job cannot see it unless
# the value is generated. Every number here already existed somewhere in the
# tree — `chip` and `row` are the two exceptions, and they name controls that do
# not exist yet, for C5 and C6 to build against.


def _metric_rows():
    """(name, value, kt-name, comment) for every metric, in emit order."""
    rows = []
    for c in METRICS["controls"]:
        rows.append((f"{c['n']}-h", c["h"], f"{c['kt']}H", c["role"]))
        rows.append((f"{c['n']}-px", c["px"], f"{c['kt']}PX", "horizontal padding"))
        rows.append((f"{c['n']}-py", c["py"], f"{c['kt']}PY", "vertical padding"))
    for e in METRICS["shape"]:
        rows.append((e["n"], e["v"], e["kt"], e["role"]))
    for e in METRICS["space"]:
        rows.append((f"space-{e['n']}", e["v"], e["kt"], e["role"]))
    t = METRICS["touch"]
    rows.append(("touch-min", t["min"], t["kt"], t["role"]))
    return rows


def check_touch_targets():
    """A control shorter than the touch floor must say how it still clears it.

    The chip is 32dp on purpose — it is a preset, not a button — and B7 put a
    48dp floor under every touch target. A generator that emitted the height and
    stayed quiet about the gap would be handing the next person a comfortable
    way to ship an unreachable control.
    """
    floor = METRICS["touch"]["min"]
    for c in METRICS["controls"]:
        if c["h"] < floor and "touch target" not in c["role"]:
            raise SystemExit(
                f"design tokens: control `{c['n']}` is {c['h']}dp, under the "
                f"{floor}dp touch floor, and its role does not say how it still "
                f"clears it. Say so in tokens.json or raise the height."
            )


# The three sizes VISUALDESIGN §3 declares, and their permitted ranges. A range
# rather than a number because the design brief gives one ("48–52 dp", "32–36 dp")
# — the exact value is a judgement, the *count* is not.
CONTROL_SIZES = {
    "control": (48, 52),
    "chip": (32, 36),
    "row": (52, 60),
}


def check_control_sizes():
    """Exactly three interactive sizes, no more, no fewer, each in its range.

    The whole point of C5 is that a fourth height is how a design stops being a
    system. Enforcing it here rather than in review means adding one is a
    deliberate edit to this list with a reason attached, instead of a number that
    slipped into a screen and was never noticed again.
    """
    got = {c["n"]: c["h"] for c in METRICS["controls"]}
    if set(got) != set(CONTROL_SIZES):
        extra = sorted(set(got) - set(CONTROL_SIZES))
        missing = sorted(set(CONTROL_SIZES) - set(got))
        raise SystemExit(
            "design tokens: there are exactly three interactive sizes "
            f"({', '.join(sorted(CONTROL_SIZES))}), and tokens.json disagrees.\n"
            + (f"  unexpected: {', '.join(extra)}\n" if extra else "")
            + (f"  missing:    {', '.join(missing)}\n" if missing else "")
            + "A fourth size is how a design stops being a system. If you mean it,\n"
            "change CONTROL_SIZES here and say why in VISUALDESIGN §3."
        )
    for name, h in sorted(got.items()):
        lo, hi = CONTROL_SIZES[name]
        if not lo <= h <= hi:
            raise SystemExit(
                f"design tokens: control `{name}` is {h}dp, outside the "
                f"{lo}–{hi}dp the design language allows for it."
            )


def gen_metrics_css():
    L = ["  /* Controls, spacing and shape — C5. Values in px; the Android side emits",
         "     the same numbers as dp, which agree at 1x. */"]
    decls, comments = [], []
    for name, v, _kt, comment in _metric_rows():
        decls.append(f"  --{name}: {v}px;")
        comments.append(comment)
    L += _align(decls, comments)
    return L


def gen_metrics_kt():
    L = ["", "/**", " * Sizing, spacing and shape — the C5 control system, generated from",
         " * design/tokens.json. Hand-typing a button height here is a CI failure for the",
         " * same reason hand-typing a hex is.", " */", "internal object Metrics {"]
    decls, comments = [], []
    for _name, v, kt, comment in _metric_rows():
        decls.append(f"    val {kt} = {v}.dp")
        comments.append(comment)
    L += _align_kt(decls, comments)
    L.append("}")
    return L


def gen_metrics_md():
    L = ["", "### Controls, spacing and shape (C5)", "",
         "| Token | Value | CSS | Kotlin | Role |", "|---|---|---|---|---|"]
    for name, v, kt, comment in _metric_rows():
        L.append(f"| `{name}` | {v} | `var(--{name})` | `Metrics.{kt}` | {comment} |")
    return L


# ------------------------------------------------------------ region writing
MARKERS = {
    ".css": ("/* >>> design tokens: generated by design/generate.py — do not edit <<< */",
             "/* >>> end design tokens <<< */"),
    ".mjs": ("  /* >>> design tokens: generated by design/generate.py — do not edit <<< */",
             "  /* >>> end design tokens <<< */"),
    ".kt": ("// >>> design tokens: generated by design/generate.py — do not edit <<<",
            "// >>> end design tokens <<<"),
    ".md": ("<!-- >>> design tokens: generated by design/generate.py — do not edit <<< -->",
            "<!-- >>> end design tokens <<< -->"),
}


def write_region(relpath, body):
    """Replace the text between this file's markers, leaving the rest alone."""
    path = os.path.join(ROOT, relpath)
    ext = os.path.splitext(path)[1]
    start, end = MARKERS[ext]
    src = open(path, encoding="utf-8").read()

    if start not in src or end not in src:
        raise SystemExit(
            f"{relpath}: missing generated-region markers.\n"
            f"  expected: {start}\n"
            f"        and: {end}"
        )
    pre = src.split(start)[0]
    post = src.split(end, 1)[1]
    out = pre + start + "\n" + body.rstrip("\n") + "\n" + end + post
    if out != src:
        open(path, "w", encoding="utf-8").write(out)
        print(f"wrote {relpath}")
    else:
        print(f"  ok  {relpath}")


def css_hex(name, theme="dark"):
    """Hex as the CSS should spell it (#fff, not #ffffff, where declared)."""
    e = entry(name, theme)
    return e.get("css") or resolve(name, theme)


# --------------------------------------------------------------- site/style.css
def gen_site_css():
    cfg = SURFACES["site"]
    void = resolve("void")
    L = []
    L.append("/* Neo-Tokyo Tactical Wasteland — see docs/VISUALDESIGN.md, which is normative")
    L.append("   for the reasoning. The values come from design/tokens.json; change a colour")
    L.append("   there and run design/generate.py. Editing this block by hand is a CI failure. */")
    L.append(":root {")
    L.append("  /* Raw palette */")

    decls, comments = [], []
    label_w = max(len(e["label"]) for e in DARK["palette"])
    for e in DARK["palette"]:
        decls.append(f"  --{e['n']}: {e['hex']};")
        c = e["label"]
        if e["n"] in CONTRAST["foregrounds"]:
            c = f"{c.ljust(label_w)}  {ratio(e['hex'], void):>5.2f}:1 on void"
        for base, g in zip(CONTRAST["bases"], CONTRAST["grades"].get(e["n"], [])):
            if g == "forbidden":
                c += f" — NEVER on {base} ({ratio(e['hex'], resolve(base)):.2f}:1)"
        comments.append(c)
    L += _align(decls, comments)

    L.append("")
    L.append("  /* Semantic roles */")
    decls, comments = [], []
    for n in cfg["semantic"]:
        e = DARK_S[n]
        if "ref" in e:
            decls.append(f"  --{n}: var(--{e['ref']});")
            comments.append("")
        else:
            decls.append(f"  --{n}: {css_hex(n)};")
            c = e["role"]
            if n in ("prose", "dim"):
                c += f", {ratio(resolve(n), void):.2f}:1 on void"
            comments.append(c)
    L += _align(decls, comments)

    L.append("")
    for f in T["fonts"]:
        if f["n"] not in cfg["fonts"]:
            continue
        if f.get("note"):
            wrapped = _wrap(f["note"], 74)
            for i, line in enumerate(wrapped):
                prefix = "  /* " if i == 0 else "     "
                suffix = " */" if i == len(wrapped) - 1 else ""
                L.append(f"{prefix}{line}{suffix}")
        L.append(f"  --{f['n']}: {f['stack']};")
    L.append("")
    L += gen_metrics_css()
    L.append("}")

    L.append("")
    L.append("/* Field Notes — the printed-manual voice. There is no light variant of a CRT in")
    L.append("   a ruined basement, so light mode is a different artefact rather than a wash of")
    L.append("   the same one. Every colour re-checked to clear 4.5:1 on paper. */")
    L.append("@media (prefers-color-scheme: light) {")
    L.append("  :root {")
    for n in cfg["light_raw"]:
        L.append(f"    --{n}: {css_hex(n, 'light')};")
    for n in cfg["light_semantic"]:
        e = LIGHT_S[n]
        if "ref" in e:
            L.append(f"    --{n}: var(--{e['ref']});")
        else:
            L.append(f"    --{n}: {css_hex(n, 'light')};")
    L.append("  }")
    L.append("}")
    return "\n".join(L)


def _align(decls, comments):
    """Pad declarations so their trailing comments line up."""
    width = max(len(d) for d in decls) + 1
    out = []
    for d, c in zip(decls, comments):
        out.append(f"{d.ljust(width)}/* {c} */" if c else d)
    return out


def _wrap(text, width):
    words, lines, cur = text.split(), [], ""
    for w in words:
        if cur and len(cur) + 1 + len(w) > width:
            lines.append(cur)
            cur = w
        else:
            cur = f"{cur} {w}".strip()
    if cur:
        lines.append(cur)
    return lines


# ------------------------------------------------- web/build-standalone.mjs
def gen_standalone_css():
    cfg = SURFACES["standalone"]
    L = []
    L.append("  :root {")
    L.append("    /* Neo-Tokyo Tactical Wasteland (docs/VISUALDESIGN.md §1) — same values as")
    L.append("       site/style.css, so the docs site and this standalone node read as one")
    L.append("       design language rather than two unrelated apps sharing a repo. Resolved")
    L.append("       hexes rather than a raw-plus-semantic indirection: this file ships alone. */")
    for n in cfg["semantic"]:
        L.append(f"    --{n}:{css_hex(n)};")
    for f in T["fonts"]:
        if f["n"] in cfg["fonts"]:
            L.append(f"    --{f['n']}: {f['stack']};")
    for name, v, _kt, _c in _metric_rows():
        L.append(f"    --{name}:{v}px;")
    L.append("  }")
    L.append("  @media (prefers-color-scheme: light) {")
    L.append("    :root {")
    L.append("      /* Field Notes (VISUALDESIGN §1 \"Light mode\") — every colour re-checked")
    L.append("         to clear 4.5:1 on paper, same as site/style.css's light variant. */")
    for n in cfg["light_semantic"]:
        L.append(f"      --{n}:{css_hex(n, 'light')};")
    L.append("    }")
    L.append("  }")
    return "\n".join(L)


# ---------------------------------------------------------------- Chrome.kt
def _kt_name(name, theme="dark"):
    e = entry(name, theme)
    if "kt" in e:
        return e["kt"]
    return "".join(p.capitalize() for p in name.split("-"))


def _kt_color(hex_):
    return f"Color(0xFF{hex_.lstrip('#').upper()})"


def gen_android_kt():
    cfg = SURFACES["android"]
    bases = CONTRAST["bases"]
    L = []
    L.append("/**")
    L.append(" * The §1 tokens, generated from design/tokens.json by design/generate.py.")
    L.append(" *")
    L.append(" * Ratios are measured, not estimated, and are regenerated with the colour, so")
    L.append(" * they cannot fall out of step with it. Three numbers per foreground: contrast")
    L.append(f" * on {', '.join(_kt_name(b) for b in bases)}, in that order.")
    L.append(" */")
    L.append("internal object Palette {")

    decls, comments = [], []
    for e in DARK["palette"]:
        decls.append(f"    val {_kt_name(e['n'])} = {_kt_color(e['hex'])}")
        if e["n"] in CONTRAST["grades"]:
            rs = [ratio(e["hex"], resolve(b)) for b in bases]
            c = " / ".join(f"{r:.2f}" for r in rs)
            for base, g in zip(bases, CONTRAST["grades"][e["n"]]):
                if g == "forbidden":
                    c += f" ← never on {_kt_name(base)}"
                elif g == "large":
                    c += f" ({_kt_name(base)} is large-text only)"
        else:
            c = f"{e['label']} — {e['role']}"
        comments.append(c)
    for n in cfg["semantic"]:
        e = DARK_S[n]
        decls.append(f"    val {_kt_name(n)} = {_kt_color(resolve(n))}")
        c = e["role"]
        if n == "dim":
            c += f", {ratio(resolve(n), resolve('void')):.2f}:1 on Void"
        comments.append(c)
    L += _align_kt(decls, comments)

    L.append("")
    L.append("    // Field Notes (light) — each re-checked to clear 4.5:1 on paper")
    decls, comments = [], []
    for n in cfg["light_semantic"]:
        decls.append(f"    val {_kt_name(n, 'light')} = {_kt_color(resolve(n, 'light'))}")
        comments.append(LIGHT_S[n].get("role", ""))
    for n in cfg["light_raw"]:
        r = ratio(resolve(n, "light"), resolve("bg", "light"))
        decls.append(f"    val {_kt_name(n, 'light')} = {_kt_color(resolve(n, 'light'))}")
        comments.append(f"{r:.2f}:1 on Paper")
    L += _align_kt(decls, comments)
    L.append("}")
    L += gen_metrics_kt()
    return "\n".join(L)


def _align_kt(decls, comments):
    width = max(len(d) for d in decls) + 1
    return [
        f"{d.ljust(width)}// {c}" if c else d for d, c in zip(decls, comments)
    ]


# ----------------------------------------------------------- VISUALDESIGN.md
def gen_visualdesign_md():
    bases = CONTRAST["bases"]
    L = []
    L.append("### Tokens")
    L.append("")
    L.append("| Token | Hex | Role |")
    L.append("|---|---|---|")
    for e in DARK["palette"]:
        L.append(f"| `--{e['n']}` | `{e['hex']}` | {e['label']} — {e['role']} |")
    L.append("")
    L.append("### Measured contrast")
    L.append("")
    L.append("Ratios against each base. **OK** = passes 4.5:1 for body text; **lg** = 3:1, large")
    L.append("text and UI chrome only; **XX** = fails both. Computed from the hexes above by")
    L.append("`design/generate.py`, which fails the build if any of them stops matching the")
    L.append("grade `design/tokens.json` claims for it.")
    L.append("")
    L.append("| | " + " | ".join(f"on `--{b}`" for b in bases) + " |")
    L.append("|---|" + "---|" * len(bases))
    label = {"body": "**OK**", "large": "lg", "forbidden": "**XX**"}
    for fg in CONTRAST["foregrounds"]:
        e = entry(fg)
        name = f"`--{fg}` `{e['hex']}`" if fg not in DARK_P else f"`--{fg}`"
        cells = []
        for b in bases:
            r = ratio(resolve(fg), resolve(b))
            cells.append(f"{label[grade_of(r)]} {r:.2f}")
        L.append(f"| {name} | " + " | ".join(cells) + " |")
    # Authored wording, computed numbers: the prose is human ("olive" reads better
    # than the token name), but every ratio in it is substituted from the hexes, so
    # a sentence cannot outlive the palette it describes.
    subs = {
        f"{fg}_on_{b}": f"{ratio(resolve(fg), resolve(b)):.2f}"
        for fg in CONTRAST["foregrounds"]
        for b in bases
    }
    for note in CONTRAST["prose_notes"]:
        L.append("")
        L.append(note.format(**subs))
    L += gen_metrics_md()
    return "\n".join(L)


def main():
    check_contrast()
    check_touch_targets()
    check_control_sizes()
    write_region(SURFACES["site"]["file"], gen_site_css())
    write_region(SURFACES["standalone"]["file"], gen_standalone_css())
    write_region(SURFACES["android"]["file"], gen_android_kt())
    write_region("docs/VISUALDESIGN.md", gen_visualdesign_md())

    # The annotated specimen sheet. Generated like everything else, so the picture
    # in the design guideline cannot drift from the values it illustrates — a
    # hand-drawn guideline is wrong within a month and nobody notices.
    try:
        import specimen
    except ImportError:
        print("  --  specimen sheet skipped (no Pillow); install it to regenerate")
    else:
        size = specimen.draw(T, resolve, ratio, grade_of, METRICS,
                             os.path.join(ROOT, "docs/spore-specimen.png"))
        print(f"  ok  docs/spore-specimen.png {size[0]}x{size[1]}")


if __name__ == "__main__":
    main()
