#!/usr/bin/env python3
"""Draw the annotated specimen sheet — `docs/spore-specimen.png`.

Imported by `design/generate.py`, so the picture is generated from
`design/tokens.json` like every other surface and cannot drift from it. The
"design tokens in sync" CI job guards it: change a hex, forget to regenerate, red
PR. A design guideline whose image is drawn by hand is a design guideline that is
wrong within a month.

Everything annotated here is *measured*, not typed — the contrast numbers come
from the same WCAG functions that grade the palette, and the control boxes are
drawn at the token's real pixel size rather than at a size that looks about right.
"""
from PIL import Image, ImageDraw, ImageFont

W, PAD = 1400, 40
MONO = "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf"
MONOB = "/usr/share/fonts/truetype/dejavu/DejaVuSansMono-Bold.ttf"


def _f(path, size):
    try:
        return ImageFont.truetype(path, size)
    except OSError:
        return ImageFont.load_default()


def draw(tokens, resolve, ratio, grade_of, metrics, out_path):
    F, FB = _f(MONO, 17), _f(MONOB, 17)
    FS, FH = _f(MONO, 14), _f(MONOB, 30)

    void, asphalt = resolve("void"), resolve("asphalt")
    moss, edge = resolve("moss"), resolve("moss")
    amber, pink, dim = resolve("amber"), resolve("pink"), resolve("dim")
    prose = resolve("prose")

    im = Image.new("RGB", (W, 1750), void)
    d = ImageDraw.Draw(im)
    y = PAD

    def head(t, sub=None):
        nonlocal y
        d.text((PAD, y), t, font=FH, fill=amber)
        y += 40
        if sub:
            d.text((PAD, y), sub, font=FS, fill=dim)
            y += 24
        y += 10

    def crate(x0, y0, x1, y1, fill):
        """The container rule, drawn: 2px edge, 4px hard offset shadow, no blur."""
        d.rectangle([x0 + 4, y0 + 4, x1 + 4, y1 + 4], fill="#000000")
        d.rectangle([x0, y0, x1, y1], fill=fill, outline=edge, width=2)

    head("SPORE — visual specimen",
         "Generated from design/tokens.json by design/generate.py. Every number measured, never typed.")

    # ---- palette, with every measured grade on every base ---------------------
    head("Palette")
    bases = [("void", void), ("asphalt", asphalt), ("moss", moss)]
    sw, gap = 210, 14
    x = PAD
    for e in tokens["dark"]["palette"]:
        hexv = e["hex"]
        crate(x, y, x + sw, y + 96, hexv)
        # Label inside the swatch only where it is legible against it.
        r_on = ratio("#ffffff", hexv)
        d.text((x + 12, y + 12), e["kt"], font=FB, fill="#ffffff" if r_on >= 4.5 else "#000000")
        d.text((x + 12, y + 40), hexv, font=FS, fill="#ffffff" if r_on >= 4.5 else "#000000")
        yy = y + 108
        if e["n"] in [b[0] for b in bases]:
            # A base is something to put ink *on*; grading it against itself
            # would print 1.00 and mean nothing.
            d.text((x + 12, yy), "base — a surface, not", font=FS, fill=dim)
            yy += 20
            d.text((x + 12, yy), "a foreground", font=FS, fill=dim)
            yy += 40
        else:
            for bn, bh in bases:
                r = ratio(hexv, bh)
                g = grade_of(r)
                mark = {"body": "OK", "large": "lg", "forbidden": "XX"}[g]
                col = {"body": resolve("phosphor"), "large": amber, "forbidden": pink}[g]
                d.text((x + 12, yy), f"{mark} {r:5.2f} on {bn}", font=FS, fill=col)
                yy += 20
        x += sw + gap
        if x + sw > W - PAD:
            x, y = PAD, yy + 24
    y += 190

    # ---- the one forbidden pairing, shown rather than described ---------------
    head("The one forbidden pairing")
    crate(PAD, y, PAD + 420, y + 84, moss)
    d.text((PAD + 20, y + 30), "PINK ON MOSS", font=FB, fill=pink)
    r = ratio(pink, moss)
    d.text((PAD + 450, y + 20), f"{r:.2f}:1 — unreadable, and the one", font=F, fill=prose)
    d.text((PAD + 450, y + 44), "combination the palette invites.", font=F, fill=prose)
    y += 120
    crate(PAD, y, PAD + 420, y + 84, void)
    d.text((PAD + 20, y + 30), "PINK ON VOID", font=FB, fill=pink)
    d.text((PAD + 450, y + 32), f"{ratio(pink, void):.2f}:1 — do this instead.", font=F, fill=prose)
    y += 130

    # ---- the three control sizes, drawn at their real heights -----------------
    head("Three interactive sizes, and no fourth",
         "Boxes are drawn at the token's real pixel height. generate.py fails the build on a fourth.")
    x = PAD
    touch = metrics["touch"]["min"]
    for c in metrics["controls"]:
        h, px, py = c["h"], c["px"], c["py"]
        wid = 300
        face = pink if c["n"] == "control" else moss
        ink = void if c["n"] == "control" else amber
        crate(x, y + (touch - h if h < touch else 0), x + wid, y + (touch - h if h < touch else 0) + h, face)
        d.text((x + px, y + (touch - h if h < touch else 0) + py), c["n"].upper(), font=FB, fill=ink)
        yy = y + touch + 16
        d.text((x, yy), f"{c['n']}-h  {h}px", font=FS, fill=amber); yy += 20
        d.text((x, yy), f"pad   {px} x {py}", font=FS, fill=dim); yy += 20
        if h < touch:
            d.text((x, yy), f"under the {touch}px floor —", font=FS, fill=pink); yy += 18
            d.text((x, yy), "pad out to reach it", font=FS, fill=pink)
        else:
            d.text((x, yy), f"clears the {touch}px floor", font=FS, fill=resolve("phosphor"))
        x += wid + 60
    y += touch + 130

    # ---- spacing scale, drawn to scale ---------------------------------------
    head("Spacing — four steps")
    x = PAD
    for e in metrics["space"]:
        v = e["v"]
        d.rectangle([x, y, x + v, y + 60], fill=resolve("copper"))
        d.text((x, y + 70), f"{e['n']} {v}", font=FS, fill=amber)
        d.text((x, y + 90), e["role"].split("—")[0].strip()[:22], font=FS, fill=dim)
        x += max(v, 60) + 230
    y += 150

    im = im.crop((0, 0, W, y + PAD))
    im.save(out_path, optimize=True)
    return im.size
