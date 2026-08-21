#!/usr/bin/env python3
"""Generate the design tokens into every surface that renders them.

    python3 design/generate.py

One palette, one remaining consumer: Android's Compose `Palette`. (The docs
site and the standalone node no longer take a generated token block — both
import HARDBRUT's real CSS at build time instead; see
`web/hardbrut-import.mjs`. There is no more SPORE-authored design document —
HARDBRUT upstream is normative for colour, contrast and components; this
script only computes and checks.) Written between marker comments;
everything outside the markers is left alone, because the file is mostly
hand-written and only its token block is not.

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
    # HARDBRUT is light-first: the grades matrix is evaluated against the light
    # theme (the default), not the dark one.
    matrix_theme = CONTRAST.get("matrix_theme", "light")

    for fg, grades in CONTRAST["grades"].items():
        for base, declared in zip(bases, grades):
            r = ratio(resolve(fg, matrix_theme), resolve(base, matrix_theme))
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


# The three interactive sizes SPORE settled on for Android, and their permitted
# ranges. A range rather than a number because the original brief gave one
# ("48–52 dp", "32–36 dp") — the exact value is a judgement, the *count* is not.
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
            "change CONTROL_SIZES here and say why in the commit message."
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
    L.append(" * HARDBRUT is light-first: the primary members are the light theme, and the")
    L.append(" * identical names + `Dark` are the dark theme. Two button kinds (Yellow default,")
    L.append(" * Paper cancel); black ink; zero radius; hard no-blur shadows.")
    L.append(" */")
    L.append("internal object Palette {")

    # Light (primary) — flat names, no suffix.
    decls, comments = [], []
    names_light = []
    for e in LIGHT["palette"]:
        decls.append(f"    val {e['kt']} = {_kt_color(e['hex'])}")
        names_light.append(e['n'])
        if e["n"] in CONTRAST["grades"]:
            rs = [ratio(e["hex"], resolve(b, 'light')) for b in bases]
            c = " / ".join(f"{r:.2f}" for r in rs)
            for base, g in zip(bases, CONTRAST["grades"][e["n"]]):
                if g == "forbidden":
                    c += f" ← never on {_kt_name(base)}"
        else:
            c = f"{e['label']} — {e['role']}"
        comments.append(c)
    # Semantic roles only light theme needs (e.g. bg as a flat name).
    for n in cfg["semantic"]:
        if n in names_light:
            continue
        e = LIGHT_S.get(n) or entry(n, "light")
        decls.append(f"    val {_kt_name(n, 'light')} = {_kt_color(resolve(n, 'light'))}")
        comments.append(e.get("role", ""))
    L += _align_kt(decls, comments)

    L.append("")
    L.append("    // Dark mode — inverted ink/paper; the yellow accent is unchanged.")
    decls, comments = [], []
    seen = set()
    for n in cfg["light_raw"] + cfg["light_semantic"]:
        if n in seen:
            continue
        seen.add(n)
        decls.append(f"    val {_kt_name(n, 'dark')} = {_kt_color(resolve(n, 'dark'))}")
        comments.append(entry(n, 'dark').get("role", ""))
    L += _align_kt(decls, comments)
    L.append("}")
    L += gen_metrics_kt()
    return "\n".join(L)


def _align_kt(decls, comments):
    width = max(len(d) for d in decls) + 1
    return [
        f"{d.ljust(width)}// {c}" if c else d for d, c in zip(decls, comments)
    ]


def main():
    check_contrast()
    check_touch_targets()
    check_control_sizes()
    # M7: the docs site and the standalone node no longer carry a generated
    # token region — both import the real HARDBRUT CSS at build time (see
    # web/hardbrut-import.mjs), so there is nothing for the generator to emit
    # for either. Android still consumes a generated `Palette` until it moves
    # to the same vendored source (M7 task 4).
    write_region(SURFACES["android"]["file"], gen_android_kt())


if __name__ == "__main__":
    main()
