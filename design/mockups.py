#!/usr/bin/env python3
"""Render the four canonical screens — `docs/spore-screens.png`.

Imported by `design/generate.py`, so every colour, control height, padding,
radius and gap comes from `design/tokens.json`. That is the point: a mockup drawn
by hand shows what someone *imagined*, and drifts the moment a token moves. This
one shows what the tokens actually produce, and the "design tokens in sync" job
fails if it stops matching them.

The four screens are the ones VISUALDESIGN §3 and ROADMAP's *Screen structures*
specify: the site hero, the web node's identity header and empty state, Bridges
as uniform rows, and Advanced as grouped sections.
"""
from PIL import Image, ImageDraw, ImageFont

D = "/usr/share/fonts/truetype/dejavu/"


def _f(n, s):
    try:
        return ImageFont.truetype(D + n, s)
    except OSError:
        return ImageFont.load_default()


def render(T, resolve, M, out_path):
    P = {e["n"]: e["hex"] for e in T["dark"]["palette"]}
    void, asphalt, moss = P["void"], P["asphalt"], P["moss"]
    amber, pink, cyan, phos, copper = P["amber"], P["pink"], P["cyan"], P["phosphor"], P["copper"]
    prose, dim = resolve("prose"), resolve("dim")

    m = {c["n"]: c for c in M["controls"]}
    S = {e["n"]: e["v"] for e in M["space"]}
    RAD = next(e["v"] for e in M["shape"] if e["n"] == "radius")
    BOR = next(e["v"] for e in M["shape"] if e["n"] == "border")
    THROW = next(e["v"] for e in M["shape"] if e["n"] == "throw")

    H1, H2 = _f("DejaVuSans-Bold.ttf", 44), _f("DejaVuSans-Bold.ttf", 19)
    B, R, SM = _f("DejaVuSansMono-Bold.ttf", 14), _f("DejaVuSansMono.ttf", 14), _f("DejaVuSansMono.ttf", 12)
    LB = _f("DejaVuSans-Bold.ttf", 15)

    W = 1500
    im = Image.new("RGB", (W, 1500), void)  # cropped to content at the end
    d = ImageDraw.Draw(im)

    def crate(x0, y0, x1, y1, fill=asphalt, edge=moss, shadow=True):
        if shadow:
            d.rectangle([x0 + THROW, y0 + THROW, x1 + THROW, y1 + THROW], fill="#000000")
        d.rounded_rectangle([x0, y0, x1, y1], RAD, fill=fill, outline=edge, width=BOR)

    def button(x, y, w, label, kind="primary"):
        """A control at its real token height — never a size that looks about right."""
        c = m["control"]
        face = pink if kind == "primary" else (moss if kind == "secondary" else void)
        ink = void if kind == "primary" else amber
        crate(x, y, x + w, y + c["h"], fill=face, edge=pink if kind == "outline" else moss)
        tw = d.textlength(label, font=B)
        d.text((x + (w - tw) / 2, y + (c["h"] - 14) / 2 - 2), label, font=B,
               fill=pink if kind == "outline" else ink)
        return y + c["h"]

    def chip(x, y, label, fill=void, ink=amber, edge=moss):
        c = m["chip"]
        w = d.textlength(label, font=SM) + 2 * c["px"]
        crate(x, y, x + w, y + c["h"], fill=fill, edge=edge, shadow=False)
        d.text((x + c["px"], y + (c["h"] - 12) / 2 - 1), label, font=SM, fill=ink)
        return x + w

    def row(x, y, w, icon_col, title, status=None, status_col=None):
        """The uniform list row — one height everywhere (§3)."""
        h = m["row"]["h"]
        d.rounded_rectangle([x, y, x + w, y + h], RAD, fill=asphalt, outline=moss, width=1)
        d.rectangle([x + S["pad"], y + h / 2 - 7, x + S["pad"] + 14, y + h / 2 + 7], fill=icon_col)
        d.text((x + S["pad"] + 14 + S["gap"], y + h / 2 - 8), title, font=R, fill=prose)
        if status:
            tw = d.textlength(status, font=SM)
            d.text((x + w - S["pad"] - tw, y + h / 2 - 6), status, font=SM, fill=status_col or cyan)
        return y + h + 1

    def cap(x, y, t):
        d.text((x, y), t, font=SM, fill=dim)
        return y + 20

    # ============================ 1. site hero ==============================
    HW, HH = 940, 470
    hx, hy = S["section"] * 2, S["section"] * 2
    crate(hx, hy, hx + HW, hy + HH, fill=void)
    d.text((hx + S["section"] * 2, hy + S["section"] + 4), "SPORE", font=H2, fill=prose)
    nx = hx + 190
    for lbl, on in [("Try it", False), ("How it works", False), ("Get a node", True),
                    ("Spec", False), ("Mission", False)]:
        d.text((nx, hy + S["section"] + 8), lbl, font=R, fill=amber if on else dim)
        w = d.textlength(lbl, font=R)
        if on:
            d.line([nx, hy + S["section"] + 28, nx + w, hy + S["section"] + 28], fill=amber, width=2)
        nx += w + S["section"] + 8
    for i, line in enumerate(["MESSAGES THAT", "STILL GET THROUGH"]):
        tw = d.textlength(line, font=H1)
        d.text((hx + (HW - tw) / 2, hy + 110 + i * 54), line, font=H1, fill=amber)
    sub = "One delivery layer for your own devices. No account, no server."
    tw = d.textlength(sub, font=R)
    d.text((hx + (HW - tw) / 2, hy + 232), sub, font=R, fill=prose)
    bw = 230
    bx = hx + (HW - (bw * 2 + S["section"])) / 2
    button(bx, hy + 280, bw, "OPEN WEB NODE", "primary")
    button(bx + bw + S["section"], hy + 280, bw, "GET A NODE", "secondary")
    cx = hx + S["section"] * 2
    cy = hy + 280 + m["control"]["h"] + S["section"] + 8
    for t, sdesc in [("Postcard", "signed, expires"), ("Any link", "radio to folder"),
                     ("Honest privacy", "sealed, stated")]:
        crate(cx, cy, cx + 280, cy + 76)
        d.text((cx + S["pad"], cy + S["pad"]), t, font=LB, fill=amber)
        d.text((cx + S["pad"], cy + S["pad"] + 26), sdesc, font=SM, fill=dim)
        cx += 280 + S["section"]
    cap(hx, hy + HH + S["gap"], "SITE — hero: one headline, one sentence, one primary. Three crates above the fold.")

    # ======================= 2. web node identity ===========================
    nx0, ny0, NW, NH = hx + HW + S["section"] * 2, hy, 460, HH
    crate(nx0, ny0, nx0 + NW, ny0 + NH, fill=void)
    ix, iy, IW = nx0 + S["pad"], ny0 + S["pad"], NW - 2 * S["pad"]
    crate(ix, iy, ix + IW, iy + 84, shadow=False)
    d.ellipse([ix + S["pad"], iy + 18, ix + S["pad"] + 48, iy + 66], fill=moss, outline=amber, width=2)
    d.text((ix + S["pad"] + 62, iy + 20), "Super Sexy Radio", font=LB, fill=amber)
    d.text((ix + S["pad"] + 62, iy + 44), "a2b2…80fe", font=R, fill=prose)
    st = "alive · 0 peers · 12 stored"
    d.text((ix + S["pad"] + 62, iy + 64), st, font=SM, fill=phos)
    chip(ix + IW - 78, iy + 22, "COPY", ink=amber)
    ey = iy + 84 + S["section"] * 2
    d.ellipse([nx0 + NW / 2 - 34, ey, nx0 + NW / 2 + 34, ey + 68], fill=pink, outline=void, width=2)
    d.ellipse([nx0 + NW / 2 - 16, ey + 22, nx0 + NW / 2 - 4, ey + 34], fill=void)
    d.ellipse([nx0 + NW / 2 + 4, ey + 22, nx0 + NW / 2 + 16, ey + 34], fill=void)
    t = "Baud"
    d.text((nx0 + NW / 2 - d.textlength(t, font=LB) / 2, ey + 76), t, font=LB, fill=prose)
    t = "No one nearby yet"
    d.text((nx0 + NW / 2 - d.textlength(t, font=R) / 2, ey + 100), t, font=R, fill=amber)
    by = ey + 134
    button(ix, by, IW, "ADD BRIDGE", "primary")
    button(ix, by + m["control"]["h"] + S["gap"], IW, "SHARE INVITE", "outline")
    cap(nx0, ny0 + NH + S["gap"], "WEB NODE — persistent identity header; Baud on empty states only.")

    # ============================ 3. bridges ================================
    px, py, PW, PH = hx, hy + HH + 74, 440, 664
    crate(px, py, px + PW, py + PH, fill=void)
    d.text((px + S["pad"], py + S["pad"]), "BRIDGES", font=H2, fill=amber)
    chip(px + PW - 108, py + S["pad"] + 2, "0 peers", ink=phos)
    y = py + S["pad"] + 46
    y = cap(px + S["pad"], y, "NETWORK") + 2
    iw = PW - 2 * S["pad"]
    RH = m["chip"]["h"] * 2 + S["gap"] + S["pad"] * 2
    crate(px + S["pad"], y, px + S["pad"] + iw, y + RH, shadow=False)
    d.ellipse([px + S["pad"] + 12, y + 22, px + S["pad"] + 22, y + 32], fill=phos)
    d.text((px + S["pad"] + 34, y + 16), "UDP broadcast", font=LB, fill=amber)
    d.text((px + S["pad"] + 34, y + 42), "primary subnet → on", font=SM, fill=dim)
    chip(px + S["pad"] + iw - 96, y + S["pad"], "PAUSE", ink=amber)
    chip(px + S["pad"] + iw - 96, y + S["pad"] + m["chip"]["h"] + S["gap"], "REMOVE", ink=pink, edge=pink)
    y += RH + S["gap"]
    y = button(px + S["pad"], y, iw, "ADD A BRIDGE", "secondary") + S["section"]
    y = cap(px + S["pad"], y, "OTHER TRANSPORT") + 2
    for t in ["Audio modem", "Meshtastic", "WebSocket", "Nostr relay"]:
        y = row(px + S["pad"], y, iw, copper, t, "add", amber)
    ty = py + PH - 46
    d.line([px, ty, px + PW, ty], fill=moss, width=1)
    tw = PW / 3
    for i, (t, on) in enumerate([("Chats", False), ("Feed", False), ("Bridges", True)]):
        w = d.textlength(t, font=R)
        d.text((px + i * tw + (tw - w) / 2, ty + 14), t, font=R, fill=amber if on else dim)
        if on:
            d.line([px + i * tw + (tw - w) / 2, ty + 36, px + i * tw + (tw + w) / 2, ty + 36],
                   fill=pink, width=3)
    cap(px, py + PH + S["gap"], "BRIDGES — uniform rows; one ADD, not eight buttons.")

    # ============================ 4. advanced ===============================
    ax = px + PW + S["section"] * 2
    crate(ax, py, ax + PW, py + PH, fill=void)
    t = "ADVANCED"
    d.text((ax + PW / 2 - d.textlength(t, font=H2) / 2, py + S["pad"]), t, font=H2, fill=amber)
    y = py + S["pad"] + 46
    for section, rows in [
        ("IDENTITY", [("Name", "Spore User", prose), ("Address", "a2b2…80fe", prose)]),
        ("SECURITY", [("Seed", "reveal", pink), ("Prekey ring", "export", amber),
                      ("Offline window", "7d", cyan)]),
        ("NODE", [("Peers", "0", cyan), ("Envelopes", "65", cyan), ("Store budget", "0.00", cyan)]),
    ]:
        y = cap(ax + S["pad"], y, section) + 2
        for title, val, col in rows:
            y = row(ax + S["pad"], y, iw, copper, title, val, col)
        y += S["gap"]
    cap(ax, py + PH + S["gap"], "ADVANCED — grouped rows; nothing expanded by default.")

    im = im.crop((0, 0, W, py + PH + S["section"] * 2 + 14))
    im.save(out_path, optimize=True)
    return im.size
