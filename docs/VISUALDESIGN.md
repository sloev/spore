# Visual design — Neo-Tokyo Tactical Wasteland

One visual language for every SPORE surface: the web node, the Pages site, the
Android app, and whatever comes next. **Post-apocalyptic survivalist bunker meets
hyper-cute anime cyberpunk** — a ruggedised field terminal that someone has covered
in stickers.

This document is normative. It defines tokens, not moods: every value here is meant
to be pasted into a stylesheet or a Compose theme, and every rule is one an
implementer can check they followed.

<p align="center"><em>The mascot is <strong>Baud</strong> — a pastel chibi in
tactical web-gear and an eye-patch, holding an oversized LoRa antenna, throwing a
peace sign. Baud appears at empty states and completions, never in the way of
work.</em></p>

## Implementation status

This document is normative, and it is only worth anything where code follows it.
Which surfaces actually consume these tokens, so nobody has to guess:

| Surface | Tokens | Chrome (scanlines, crate shadows, reduced-motion) |
|---|---|---|
| `site/style.css` — the Pages site | ✅ | ✅ |
| `web/spore-standalone.html` — the browser node | ✅ inherits the stylesheet | ✅ |
| Android — `Chrome.kt` + `MainActivity.kt` | ✅ | ✅ crate, Toughbook input, radio switch, segmented LED, stickers, scanlines, reduced motion |

**Three places Android cannot match this document exactly.** Recorded here rather
than left for someone to discover as a bug:

- **No Impact.** §2 names Impact/Haettenschweiler; neither ships on Android and
  constraint 1 forbids downloading one. `FontFamily.SansSerif` at
  `FontWeight.Black` stands in. The weight and the tracking carry over; the
  condensed width does not.
- **The hard shadow is drawn by hand.** Compose's `Modifier.shadow` is a *blurred*
  elevation shadow — the Material look this language exists to replace. `Chrome.kt`
  paints the 4 dp offset rect itself, into reserved padding so a crate never bleeds
  over its neighbour.
- **Reduced motion is inferred.** Android has no `prefers-reduced-motion`. The
  platform signal is `ANIMATOR_DURATION_SCALE == 0`, which the accessibility
  "remove animations" toggle sets. Scanlines, the CRT bloom and the mascot sparkle
  are all gated on it.

The **clack and particle burst** in §3 are deliberately absent: §7 requires sound
off until the user enables it, there is no such setting yet, and shipping it
on-by-default is not a thing to get wrong once.

Keep this table honest. A design language whose spec is ahead of its
implementation is a document describing an appearance nothing has, and this
repository has enough findings about claims with no code behind them.

## 0. Three constraints that outrank taste

These are not preferences. Break them and CI fails, or someone cannot use the app.

1. **Zero network requests.** `web/spore-standalone.html` is a single file that must
   make no external requests — CI greps for `src=`/`href=` pointing at `http`, and
   the whole continuity story rests on it. **So: no Google Fonts, no CDN, no remote
   images.** VT323 and Fira Code cannot be linked. Use the system stacks in §2, or
   embed a subset as a `data:` URI and accept the bytes.
2. **Motion is opt-out by default.** Scanlines, glitch, chromatic aberration and
   particle bursts are all gated behind `prefers-reduced-motion: no-preference`.
   Under `reduce`, the interface must be *completely* static — no shimmer, no
   flicker. A CRT flicker is a photosensitivity trigger, not a decoration.
3. **Contrast is measured, not eyeballed.** §1 carries real WCAG ratios. One pairing
   is forbidden outright.

## 1. Colour — Toxic Bubblegum & Rust

### Tokens

| Token | Hex | Role |
|---|---|---|
| `--void` | `#0a0a0c` | CRT Black — page base, the dark of a powered-down screen |
| `--asphalt` | `#1a1c20` | Worn Asphalt — panels, cards, raised surfaces |
| `--kevlar` | `#4b5320` | Kevlar Olive — crate fills, inert chrome, disabled states |
| `--amber` | `#ffb000` | Tactical Amber — primary text, the default signal |
| `--phosphor` | `#39ff14` | Phosphor Green — success, live links, "it worked" |
| `--pink` | `#ff2a85` | Radioactive Pink — the kawaii accent, cursor, primary action |
| `--cyan` | `#00ffff` | Pastel Cyan — secondary accent, focus rings, selection |

### Measured contrast

Ratios against each base. **OK** = passes 4.5:1 for body text; **lg** = 3:1, large
text and UI chrome only; **XX** = fails both.

| | on `--void` | on `--asphalt` | on `--kevlar` |
|---|---|---|---|
| `--amber` | **OK** 10.80 | **OK** 9.31 | lg 4.48 |
| `--phosphor` | **OK** 14.59 | **OK** 12.58 | **OK** 6.06 |
| `--pink` | **OK** 5.58 | **OK** 4.81 | **XX** 2.32 |
| `--cyan` | **OK** 15.78 | **OK** 13.61 | **OK** 6.55 |

**Never put pink on olive.** 2.32:1 is unreadable, and it is the one combination the
palette invites — hot pink stickers on an ammo crate. Put the sticker on a `--void`
or `--asphalt` patch instead, or outline it.

Amber on olive is large-text only: headings and buttons, never body copy.

### Semantic mapping

Surfaces name their *role*, so a screen can be re-skinned without hunting hex codes:

```
--bg          = --void        page
--panel       = --asphalt     cards, crates, inputs
--edge        = #2a2f1c       borders — olive shifted dark, reads as machined metal
--ink         = --amber       body text
--dim         = #8a7a4a       de-emphasised text — amber desaturated, 4.6:1 on void
--accent      = --pink        primary action, cursor, the kawaii
--accent2     = --cyan        focus, selection, secondary action
--ok          = --phosphor    success, verified, delivered
--warn        = --amber       caution
--bad         = --pink        failure — pink does double duty; rely on the icon too
```

`--bad` and `--accent` share a hue on purpose: this palette has no red. Failure is
therefore **never signalled by colour alone** — pair it with an icon and words.

### Light mode

The bunker is dark. There is no light variant of a CRT in a ruined basement, and
inventing one produces neither aesthetic. Where a light theme is required — the
Pages site honours `prefers-color-scheme` today — use the **Field Notes** variant:
paper base `#f4f1e8`, ink `#1a1c20`, and the same four accents darkened to hold 4.5:1
(`--amber` → `#8a5f00`, `--phosphor` → `#1f7a0c`, `--pink` → `#c2185b`, `--cyan` →
`#00707a`). No scanlines, no vignette. It reads as the printed manual rather than
the terminal, which is a coherent second voice for the same project.

## 2. Typography

**System stacks only** — see constraint 1.

```css
--font-mono: ui-monospace, "Cascadia Mono", "Fira Code", Menlo, Consolas, monospace;
--font-display: "Impact", "Haettenschweiler", "Arial Narrow Bold", system-ui, sans-serif;
```

- **Body and code:** `--font-mono`. Everything is monospace; this is a terminal.
- **Headers:** `--font-display`, uppercase, `letter-spacing: -0.02em`, heavy.
- **CRT bloom** on amber/phosphor text, and only there:
  `text-shadow: 0 0 2px currentColor;` — 2px. More turns body copy to mush.
  Drop it entirely under `prefers-reduced-motion: reduce`, where it reads as blur.

If a project wants true VT323, embed a subsetted WOFF2 as a `data:` URI and measure
the cost. Do not link it.

### Micro-copy

Tactical jargon fused with kaomoji: `[UPLINK_SECURE] (ﾉ◕ヮ◕)ﾉ*:･ﾟ✧`.

**Tone zones — this is the part that needs discipline.** The register-and-kaomoji
voice is excellent on surfaces where the reader has already opted in, and actively
harmful on the one surface designed to explain SPORE to someone who has never heard
of it.

| Zone | Voice |
|---|---|
| Node UI, Android app, logs, status | Full flavour. `[PEER_ACQUIRED] ヽ(・∀・)ﾉ` |
| Builder docs — Spec, Bridges, Rebuild | Restrained. Styling yes, jargon no; these are read under pressure |
| **Pages front page** | **Plain language.** `site/home.md` is deliberately written for people who do not build software. It gets the palette, the crates and Baud — it does not get `TACTICAL REPO-SQUISH INITIATED` |

Kaomoji are punctuation, not content: they follow a message that already made sense
without them. Never put one where a screen reader will read it aloud as garbage —
`aria-hidden="true"` on the decorative ones.

## 3. Components

**Container — the ammo crate.** `--panel` fill, 2px `--edge` border, 2px hard offset
shadow (`4px 4px 0 rgba(0,0,0,.6)`), no blur, no rounding beyond 2px. Optional
hex-mesh background at ≤4% opacity. Stickers — anarchy sigils crossed with Sanrio
shapes — sit at corners, rotated 3–8°, never overlapping text.

**Input — the Toughbook.** Inset `--void` field, 2px `--edge`, four 3px "screw" dots
at the corners in `--kevlar`. Focus raises a 2px `--cyan` ring; do not remove the
outline, thicken it.

**Cursor.** Blinking `--pink` block, 1.06 s cycle. Where a glyph fits, a pixel skull
💀 or heart ♥. Under `reduce`, solid, no blink.

**Button — the radio switch.** Chunky, `--kevlar` face, 3px hard drop-shadow,
`--amber` uppercase label. Active state translates 3px down/right and drops the
shadow to 0 — a physical throw. A short *clack* (≤80 ms, ≤ −20 dBFS) and a burst of
6–10 pastel particles fire on press. Sound is **off until the user enables it**, and
particles vanish under `reduce`.

**Progress — segmented LED.** Discrete blocks, `--phosphor` filled, `--kevlar`
empty. Never a smooth bar; this machine counts. Long operations may show the 8-bit
cyber-kitty chewing files into blocks — decorative, `aria-hidden`, with the real
percentage in text beside it.

## 4. Ambient VFX

- **Scanlines:** `repeating-linear-gradient` 2px period, ≤6% black, fixed overlay,
  `pointer-events: none`.
- **Vignette:** radial, transparent centre to `rgba(0,0,0,.55)` at the edge.
- **Hover glitch:** ≤120 ms chromatic aberration — a ±1px red/cyan horizontal split —
  settling into a neon highlight. Once per hover, never looping.

All three sit behind:

```css
@media (prefers-reduced-motion: reduce) {
  .scanlines, .vignette { display: none; }
  * { animation: none !important; transition: none !important; text-shadow: none; }
}
```

## 5. Platform mapping

One source of truth, three consumers.

| Surface | How it consumes the tokens |
|---|---|
| `site/style.css` | CSS custom properties on `:root`, names exactly as §1 |
| `web/spore-standalone.html` | Same properties, inlined — no external stylesheet |
| Android (Compose) | `Color.kt` mirroring the tokens; `SporeTheme` maps them to `ColorScheme`. `--edge` → `outline`, `--panel` → `surfaceVariant`, `--accent` → `primary`, `--accent2` → `secondary` |

When a token changes, it changes in all three or in none. A screenshot in one place
and a hex code in another is how design languages rot.

## 6. Feature vocabulary

Consistent names for recurring ideas, so the same thing is called the same thing
everywhere:

| Feature | Name | Visual |
|---|---|---|
| Fragmentation / fountain | **Micro-Packing Protocol** | A pastel hydraulic press crushing data into glowing rations |
| Sealed mail, encrypted topics | **Stealth-Camo Encryption** | A steel blast door with a glowing pink padlock |
| Dedup, expiry, quotas | **Targeting: TRASH_FILES** | A cute, heavily-armed sentry turret zapping bloat |
| Prekey ring, forward secrecy | **Burn-After-Reading Keys** | Key cards dissolving into pink ash on a seven-day timer |

Keep these to UI and marketing. `SPEC.md` says "fountain code"; it is a
specification and other people implement from it.

## 7. Checklist

Before shipping a screen:

- [ ] Every colour pair checked against §1. No pink on olive.
- [ ] No failure signalled by colour alone.
- [ ] `prefers-reduced-motion: reduce` renders it completely static.
- [ ] No external font, image or stylesheet request.
- [ ] Focus visible on every interactive element, 2px `--cyan` minimum.
- [ ] Decorative kaomoji and mascots are `aria-hidden`.
- [ ] Sound off by default.
- [ ] Voice matches the zone in §2 — plain language on the front page.

## Appendix A — Android chat attachments

A UX convention that isn't obvious from the code, kept here so a later change doesn't
undo it by accident (absorbed from the retired `android/UX-ISSUES.md`).

**The problem it fixed.** The release APK published a file the instant it was picked —
no staging, so a mis-tap sent a file with no message; it then arrived as its own
contextless bubble with no preview and no way to open it.

**The marker (application convention).** An attachment travels as two envelopes: the
file's manifest+chunks (the existing publish path, sealed to the peer when known), and a
normal DATA body whose **last line** is the canonical marker:

```
📎 <filename> | spore:<hex-magnet> | <mime>
```

- Matched by `Markdown.parseAttach`, regex **`(?m)^📎 (.+) \| spore:([0-9a-fA-F]{16,}) \| (\S+)$`**.
- Application-level only: relays and non-SPORE clients see opaque UTF-8, and a client
  that doesn't parse it just shows the marker text — a reasonable fallback.
- Distinct from the feed's image form `![name](spore:<magnet>)`, which has nowhere to
  carry a mime type; chat needs the mime to choose image-preview vs file-chip.

**One bubble, both sides.** The sender's `sendTextWithAttachment` publishes the file then
sends the marker body via the shared `sendBody`, stamping `magnet`+`mime` onto its own
`Msg` (local bytes are cached immediately — our own file never comes back through the
mesh). The receiver's `route()` parses the marker and stamps `magnet`+`mime` onto the
received `Msg`; the manifest envelope's "incoming file…" bubble is suppressed for sealed
files, and `pumpFiles`' "received…" bubble is suppressed when a message already
references the magnet — so the attachment is **one** bubble, not three.

**Preview & Open.** Images decode with `inSampleSize` (cap 1080 px) on `Dispatchers.IO`
via `produceState` — a phone photo decoded whole for a 220 dp row costs ~100 MB of heap.
Open copies the bytes into `cacheDir/attachments/<magnet>` (reclaimable) and shares a
`FileProvider` `content://` URI with a one-shot read grant — **never** a `file://` path
and never the private store directly (`res/xml/file_paths.xml` lists exactly
`attachments/`).

**Scope.** Only sealed DM attachments get the single merged bubble, because only they are
guaranteed a marker sender; a **public/unsealed** file with no marker still shows the
legacy "incoming/received" status bubbles. Non-goals (v1): multiple files per send,
ExoPlayer audio/video playback, editing after send. Opening an *arbitrary received file*
(vs a cached attachment) is still open — see the ROADMAP.
