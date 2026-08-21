#!/usr/bin/env python3
"""Generate Android's Compose `Palette` from HARDBRUT's real, vendored CSS.

    python3 design/generate.py

HARDBRUT (`supernihil/hardbrut`) is vendored at build time
(`web/vendor/hardbrut/hardbrut.css`) and trusted as-is — the docs site and the
standalone node both import that real CSS directly (`web/hardbrut-import.mjs`);
there is no SPORE-authored design document restating it, and no contrast table
of our own to keep in sync with it. Android's Compose `Palette` is the one
place a colour still needs a Kotlin `Color(...)` rather than a CSS custom
property, so this script parses the vendored CSS's `:root` (light) and
`[data-theme="dark"]` blocks and regenerates `Chrome.kt`'s marked region from
them — not a second hand-typed copy of the same hexes.

The three interactive control sizes (control/chip/row) and the touch-target
floor are SPORE's own Android product decision, not a HARDBRUT concept — a
desktop-first CSS framework has no opinion on touch targets — so
`design/tokens.json` keeps just that table, and this script still enforces
"exactly three sizes, each in range" and "nothing under the touch floor
without saying why."

CI runs this and diffs; see the "design tokens in sync" job.
"""
import json
import os
import re

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
T = json.load(open(os.path.join(HERE, "tokens.json")))
METRICS = T["metrics"]

HARDBRUT_CSS = os.path.join(ROOT, "web/vendor/hardbrut/hardbrut.css")
CHROME_KT = "android/app/src/main/kotlin/org/spore/node/Chrome.kt"


# --------------------------------------------------------- parse HARDBRUT css
def _hex6(h):
    """#abc -> #aabbcc; already-6-digit hexes pass through unchanged."""
    h = h.strip()
    m = re.fullmatch(r"#([0-9a-fA-F])([0-9a-fA-F])([0-9a-fA-F])", h)
    return "#" + "".join(c * 2 for c in m.groups()) if m else h


def _px(value):
    """The leading length in a declaration ('3px solid var(--ink)' -> 3.0)."""
    token = value.strip().split()[0]
    m = re.match(r"([\d.]+)px", token)
    if m:
        return float(m.group(1))
    m = re.match(r"([\d.]+)rem", token)
    if m:
        return float(m.group(1)) * 16  # HARDBRUT's root font-size is the 16px default
    raise SystemExit(f"design tokens: cannot parse a length from HARDBRUT's {value!r}")


def _num(v):
    """Emit '48' rather than '48.0' when a parsed length is a whole number."""
    return int(v) if float(v).is_integer() else v


def _vars(block):
    """Parse `--name: value;` declarations. HARDBRUT's minified CSS sometimes
    omits the trailing `;` on the last declaration in a block, so this splits
    on `;` rather than requiring one after every declaration."""
    out = {}
    for decl in block.split(";"):
        decl = decl.strip()
        if not decl.startswith("--") or ":" not in decl:
            continue
        k, v = decl[2:].split(":", 1)
        out[k.strip()] = v.strip()
    return out


def parse_hardbrut():
    """The light (`:root`) and dark (`[data-theme="dark"]`) custom-property sets."""
    css = open(HARDBRUT_CSS, encoding="utf-8").read()
    light_m = re.search(r":root\s*\{([^}]*)\}", css)
    dark_m = re.search(r'\[data-theme="dark"\]\s*\{([^}]*)\}', css)
    if not light_m or not dark_m:
        raise SystemExit(
            "design tokens: could not find HARDBRUT's :root / [data-theme=\"dark\"] "
            f"blocks in {HARDBRUT_CSS} — did the upstream file's format change? "
            "Re-vendor with `node web/hardbrut-sync.mjs` and check by hand."
        )
    light, dark = _vars(light_m.group(1)), _vars(dark_m.group(1))
    for k in ("ink", "paper", "bg", "accent", "accent-ink", "muted", "border", "shadow"):
        if k not in light:
            raise SystemExit(f"design tokens: HARDBRUT's :root is missing --{k}")
    return light, dark


LIGHT, DARK = parse_hardbrut()


def hex_of(name, theme="light"):
    src = DARK if theme == "dark" and name in DARK else LIGHT
    return _hex6(src[name])


def px_of(name, theme="light"):
    src = DARK if theme == "dark" and name in DARK else LIGHT
    return _px(src[name])


# ---------------------------------------------------------------- metrics
#
# `control`/`chip`/`row` (and the touch floor) are SPORE's own Android sizing
# decisions — HARDBRUT ships no touch-target concept to source them from.
# `radius`/`border`/`throw` and the spacing steps *are* HARDBRUT tokens
# (`--border`, `--shadow`, `--space*`) and are parsed live rather than typed
# a second time, for the same reason a hex is not typed twice.


def _metric_rows():
    """(name, value, kt-name, comment) for every metric, in emit order."""
    rows = []
    for c in METRICS["controls"]:
        rows.append((f"{c['n']}-h", c["h"], f"{c['kt']}H", c["role"]))
        rows.append((f"{c['n']}-px", c["px"], f"{c['kt']}PX", "horizontal padding"))
        rows.append((f"{c['n']}-py", c["py"], f"{c['kt']}PY", "vertical padding"))
    rows.append((
        "radius", 0, "Radius",
        "corner radius — zero, always. HARDBRUT has no --radius token; it simply "
        "never sets one (true circles are the only exception)",
    ))
    rows.append((
        "border", _num(px_of("border")), "Border",
        "border width — HARDBRUT's --border",
    ))
    rows.append((
        "throw", _num(px_of("shadow")), "Throw",
        "the hard no-blur drop-shadow offset — HARDBRUT's --shadow",
    ))
    rows.append((
        "space-tight", _num(px_of("space-sm")), "Tight",
        "tighter internal padding — HARDBRUT's --space-sm",
    ))
    rows.append((
        "space-gap", _num(px_of("space-sm")), "Gap",
        "between controls — HARDBRUT's --space-sm",
    ))
    rows.append((
        "space-pad", _num(px_of("space")), "Pad",
        "inside a card / crate — HARDBRUT's --space",
    ))
    rows.append((
        "space-section", _num(px_of("space-lg")), "Section",
        "between sections — HARDBRUT's --space-lg",
    ))
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

    The whole point of this system is that a fourth height is how a design
    stops being a system. Enforcing it here rather than in review means adding
    one is a deliberate edit to this list with a reason attached, instead of a
    number that slipped into a screen and was never noticed again.
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
                f"{lo}–{hi}dp SPORE's Android sizing allows for it."
            )


def gen_metrics_kt():
    L = ["", "/**", " * Sizing, spacing and shape — control/chip/row are SPORE's own Android",
         " * decision; radius/border/throw/spacing are parsed from HARDBRUT's vendored CSS.",
         " * Hand-typing a button height or a shadow offset here is a CI failure for the",
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
    ".kt": ("// >>> design tokens: generated by design/generate.py — do not edit <<<",
            "// >>> end design tokens <<<"),
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


def _align_kt(decls, comments):
    width = max(len(d) for d in decls) + 1
    return [
        f"{d.ljust(width)}// {c}" if c else d for d, c in zip(decls, comments)
    ]


# ---------------------------------------------------------------- Chrome.kt
def _kt_color(hex_):
    return f"Color(0xFF{hex_.lstrip('#').upper()})"


# (HARDBRUT var, light Kotlin name, role comment). `accent` maps to `Yellow`
# for continuity with the pre-HARDBRUT palette Android already had; every
# other name matches the CSS variable it comes from.
PALETTE_FIELDS = [
    ("ink", "Ink", "text, borders, chrome"),
    ("paper", "Paper", "cards on paper"),
    ("accent", "Yellow", "primary actions, highlights — the one colour shared by both themes"),
    ("muted", "Muted", "de-emphasised text"),
    ("bg", "Bg", "page base"),
]


def gen_android_kt():
    L = []
    L.append("/**")
    L.append(" * HARDBRUT's palette, parsed live from the vendored")
    L.append(" * `web/vendor/hardbrut/hardbrut.css` by design/generate.py — not a second")
    L.append(" * hand-typed copy of the same hexes.")
    L.append(" *")
    L.append(" * HARDBRUT is light-first: the primary members are the light theme, and the")
    L.append(" * identical names + `Dark` are the dark theme. Two button kinds (Yellow default,")
    L.append(" * Paper cancel); black ink; zero radius; hard no-blur shadows.")
    L.append(" */")
    L.append("internal object Palette {")

    decls, comments = [], []
    for var, kt, role in PALETTE_FIELDS:
        decls.append(f"    val {kt} = {_kt_color(hex_of(var, 'light'))}")
        comments.append(role)
    L += _align_kt(decls, comments)

    L.append("")
    L.append("    // Dark mode — inverted ink/paper; the yellow accent is unchanged (HARDBRUT")
    L.append("    // does not override --accent for [data-theme=\"dark\"]).")
    decls, comments = [], []
    for var, kt, role in PALETTE_FIELDS:
        decls.append(f"    val {kt}Dark = {_kt_color(hex_of(var, 'dark'))}")
        comments.append(role)
    decls.append(f"    val OnYellow = {_kt_color(hex_of('accent-ink', 'light'))}")
    comments.append(
        "text sitting on the yellow face — HARDBRUT's --accent-ink, unchanged between themes"
    )
    L += _align_kt(decls, comments)
    L.append("}")
    L += gen_metrics_kt()
    return "\n".join(L)


def main():
    check_touch_targets()
    check_control_sizes()
    write_region(CHROME_KT, gen_android_kt())


if __name__ == "__main__":
    main()
