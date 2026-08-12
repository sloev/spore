**Icon locked in: Antenna + Seed**

A short, sturdy radio antenna rising from a simple seed/soil form. Clean, bold, readable at small sizes, and directly tied to mesh + living network.

Here’s a larger set of screenshots + expanded guidelines using the new mark.

---

### New Mockups (Antenna + Seed icon)

**1. Android – Chats / Home**  


**2. Android – Bridges**  


**3. Android – Feed**  


**4. Android – Advanced**  


**5. Webnode – Main view**  


**6. Webnode – Empty state**  


**7. Website – Homepage hero**  


**8. Website – Get a Node**  


**9. App icon + favicon set**  


---

### Expanded Design Guidelines Text  
(Copy-paste ready for `design-language.html`)

```markdown
## Icon System — Antenna + Seed (normative)

Primary mark: a short, sturdy radio antenna rising directly from a simplified seed/soil form.  
- Reads as “mesh + living network” at a glance.  
- Geometric enough to stay sharp at 16 px.  
- Use phosphor green + amber on dark surfaces.  
- Mono version (single colour) required for favicon and status bar.  
- Never replace with a mushroom, spore-cap, or literal fungus.

Secondary mark (optional, very small sizes only): condensed “S” monogram that can incorporate a tiny antenna detail if needed.

### Placement rules
- App icon / PWA icon / site favicon → Antenna + Seed
- Top bar / header of Android & webnode → Antenna + Seed + wordmark “SPORE”
- Empty states and loading → can appear small beside Baud, never as the main character
- Do not decorate random UI elements with the icon

## Solarpunk Cyberdeck Execution Rules (expanded)

### Colour hierarchy in practice
1. Phosphor green → live status, peer counts, active nav, success, “alive”
2. Amber → all primary text, headings, labels
3. Pink → only primary actions and critical focus moments
4. Moss / Kevlar → surfaces, secondary buttons, inert chrome
5. Copper → rare, reserved for continuity / seed / offline-window moments

### Density law (non-negotiable)
- Working screens: ≤ 1 short instructional sentence visible by default
- Status chrome: compact only (“3 peers · 28 stored”)
- Empty states: Baud + 1 line + 1–2 actions
- Lists: uniform ROW height (56 px) inside crates. No mixed button stacks

### Core screen structures

**Android / Webnode header (persistent)**
```
[Antenna+Seed] SPORE                    [3 peers]
[avatar] Petname   address…  [COPY]
alive · 3 peers · 28 stored
```

**Bridges**
```
NETWORK
┌────────────────────────────────────┐
│ ● UDP broadcast   primary · on     │
│               [PAUSE] [REMOVE]     │
└────────────────────────────────────┘
[ ADD A BRIDGE ]   ← secondary CONTROL
(uniform rows for other transports)
```

**Advanced**
- Three grouped crates only: IDENTITY · SECURITY · NODE
- Uniform rows, chips for offline window (7D / 14D / 30D)
- Long “About / security model” text behind a single expander row

**Site homepage**
- Persistent nav with Antenna+Seed + wordmark
- One bold amber headline
- One plain-language sentence
- Pink primary + kevlar secondary
- Max three story crates above the fold

### Component sizes (locked)
- CONTROL: 48 px height (primary & secondary buttons)
- CHIP: 32 px height (topics, offline-window presets, status pills)
- ROW: 56 px height (lists, settings rows)

### Motion & sound
- Reduced-motion: fully static
- Sound: off until explicitly enabled
- Button press: hard shadow drop + optional short clack (user-gated)

### Tone zones
- Node UI / status: tactical flavour allowed
- Website front page & first-run: plain language only
- Baud: empty states and completions only
```

---

This set fully replaces the mushroom with the **Antenna + Seed** mark and pushes the solarpunk-cyberdeck feeling further while staying inside the existing token and component system.

