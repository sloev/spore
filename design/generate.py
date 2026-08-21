#!/usr/bin/env python3
"""Regenerate Chrome.kt's `Palette`/`Metrics` from SPORE's two vendored HARDBRUT sources.

    python3 design/generate.py

Two vendored copies of the same upstream design system, each authoritative
for what it covers:

- `android/app/src/main/kotlin/org/spore/node/vendor/Hardbrut.kt` — the
  official Compose port (`android/hardbrut-sync.py` re-pulls it). Authoritative
  for the light palette and every border/shadow/spacing metric; Chrome.kt's
  generated region just aliases its `HardbrutTokens` object — not a copied
  colour.
- `web/vendor/hardbrut/hardbrut.css` (`web/hardbrut-sync.mjs`) — the real CSS,
  used everywhere on the web. `Hardbrut.kt` has no dark-mode variant at all, so
  this script parses the CSS's `[data-theme="dark"]` block for the four dark
  hexes Android needs and the Compose port does not provide — the one gap
  between the two vendored sources.

The three interactive control sizes (control/chip/row) and the touch-target
floor are SPORE's own Android product decision, not a HARDBRUT concept — a
desktop-first design system has no touch-target opinion — so
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


# ------------------------------------------- parse the four dark hexes (CSS)
def _hex6(h):
    """#abc -> #aabbcc; already-6-digit hexes pass through unchanged."""
    h = h.strip()
    m = re.fullmatch(r"#([0-9a-fA-F])([0-9a-fA-F])([0-9a-fA-F])", h)
    return "#" + "".join(c * 2 for c in m.groups()) if m else h


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


def parse_hardbrut_dark():
    """The four dark-mode hexes HARDBRUT's Compose port doesn't define."""
    css = open(HARDBRUT_CSS, encoding="utf-8").read()
    m = re.search(r'\[data-theme="dark"\]\s*\{([^}]*)\}', css)
    if not m:
        raise SystemExit(
            "design tokens: could not find HARDBRUT's [data-theme=\"dark\"] block "
            f"in {HARDBRUT_CSS} — did the upstream file's format change? Re-vendor "
            "with `node web/hardbrut-sync.mjs` and check by hand."
        )
    dark = _vars(m.group(1))
    for k in ("ink", "paper", "bg", "muted"):
        if k not in dark:
            raise SystemExit(f'design tokens: HARDBRUT\'s [data-theme="dark"] is missing --{k}')
    return {k: _hex6(v) for k, v in dark.items()}


DARK = parse_hardbrut_dark()


def _kt_color(hex_):
    return f"Color(0xFF{hex_.lstrip('#').upper()})"


# ---------------------------------------------------------------- metrics
#
# `control`/`chip`/`row` (and the touch floor) are SPORE's own Android sizing
# decisions — HARDBRUT ships no touch-target concept to source them from.
# `radius`/`border`/`throw` and the spacing steps *are* HARDBRUT tokens, and
# the vendored Compose port already types them as `Dp` — so those rows alias
# `HardbrutTokens` directly rather than parsing anything.


def _metric_rows():
    """(name, kt-expression, kt-name, comment) for every metric, in emit order."""
    rows = []
    for c in METRICS["controls"]:
        rows.append((f"{c['n']}-h", f"{c['h']}.dp", f"{c['kt']}H", c["role"]))
        rows.append((f"{c['n']}-px", f"{c['px']}.dp", f"{c['kt']}PX", "horizontal padding"))
        rows.append((f"{c['n']}-py", f"{c['py']}.dp", f"{c['kt']}PY", "vertical padding"))
    rows.append((
        "radius", "0.dp", "Radius",
        "corner radius — zero, always. Neither vendored HARDBRUT source defines "
        "one; it simply never sets one (true circles are the only exception)",
    ))
    rows.append((
        "border", "HardbrutTokens.Border", "Border",
        "border width — HARDBRUT's Border",
    ))
    rows.append((
        "throw", "HardbrutTokens.Shadow", "Throw",
        "the hard no-blur drop-shadow offset — HARDBRUT's Shadow",
    ))
    rows.append((
        "space-tight", "HardbrutTokens.SpaceSm", "Tight",
        "tighter internal padding — HARDBRUT's SpaceSm",
    ))
    rows.append((
        "space-gap", "HardbrutTokens.SpaceSm", "Gap",
        "between controls — HARDBRUT's SpaceSm",
    ))
    rows.append((
        "space-pad", "HardbrutTokens.Space", "Pad",
        "inside a card / crate — HARDBRUT's Space",
    ))
    rows.append((
        "space-section", "HardbrutTokens.SpaceLg", "Section",
        "between sections — HARDBRUT's SpaceLg",
    ))
    t = METRICS["touch"]
    rows.append(("touch-min", f"{t['min']}.dp", t["kt"], t["role"]))
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
         " * decision (design/tokens.json); radius/border/throw/spacing alias the vendored",
         " * `vendor/Hardbrut.kt`'s `HardbrutTokens`. Hand-typing a button height or a",
         " * shadow offset here is a CI failure for the same reason hand-typing a hex is.",
         " */", "internal object Metrics {"]
    decls, comments = [], []
    for _name, expr, kt, comment in _metric_rows():
        decls.append(f"    val {kt} = {expr}")
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
# (light Kotlin name, kt-expression aliasing HardbrutTokens, role comment).
# `AccentYellow` maps to `Yellow` for continuity with the palette Android
# already had; every other name matches the HardbrutTokens field it aliases.
PALETTE_FIELDS = [
    ("Ink", "HardbrutTokens.Ink", "text, borders, chrome"),
    ("Paper", "HardbrutTokens.Paper", "cards on paper"),
    ("Yellow", "HardbrutTokens.AccentYellow.first",
     "primary actions, highlights — the one colour shared by both themes"),
    ("Muted", "HardbrutTokens.Muted", "de-emphasised text"),
    ("Bg", "HardbrutTokens.Background", "page base"),
]


def gen_android_kt():
    L = []
    L.append("/**")
    L.append(" * HARDBRUT's palette. The light half aliases the vendored Compose port")
    L.append(" * (`vendor/Hardbrut.kt`'s `HardbrutTokens`) directly — not a copied colour.")
    L.append(" * The dark half has no equivalent there (that file defines no dark-mode")
    L.append(" * variant), so design/generate.py parses it from the vendored `hardbrut.css`")
    L.append(" * instead — the one gap between SPORE's two vendored HARDBRUT sources.")
    L.append(" *")
    L.append(" * HARDBRUT is light-first: the primary members are the light theme, and the")
    L.append(" * identical names + `Dark` are the dark theme. Two button kinds (Yellow default,")
    L.append(" * Paper cancel); black ink; zero radius; hard no-blur shadows.")
    L.append(" */")
    L.append("internal object Palette {")

    decls, comments = [], []
    for kt, expr, role in PALETTE_FIELDS:
        decls.append(f"    val {kt} = {expr}")
        comments.append(role)
    L += _align_kt(decls, comments)

    L.append("")
    L.append("    // Dark mode — inverted ink/paper; the yellow accent is unchanged. Ink/")
    L.append("    // Paper/Muted/Bg come from the vendored hardbrut.css's [data-theme=\"dark\"]")
    L.append("    // block (HardbrutTokens has no dark-mode variant to alias here).")
    decls, comments = [], []
    decls.append(f"    val InkDark = {_kt_color(DARK['ink'])}")
    comments.append("text, borders, chrome")
    decls.append(f"    val PaperDark = {_kt_color(DARK['paper'])}")
    comments.append("cards on paper")
    decls.append("    val YellowDark = HardbrutTokens.AccentYellow.first")
    comments.append("primary actions, highlights — the one colour shared by both themes")
    decls.append(f"    val MutedDark = {_kt_color(DARK['muted'])}")
    comments.append("de-emphasised text")
    decls.append(f"    val BgDark = {_kt_color(DARK['bg'])}")
    comments.append("page base")
    decls.append("    val OnYellow = HardbrutTokens.AccentYellow.second")
    comments.append("text sitting on the yellow face — unchanged between themes")
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
