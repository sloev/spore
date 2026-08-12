# MISSION

**SPORE is personal infrastructure.**

Move messages, files, and live sessions between **devices you control**, over
**whatever path exists**, and plug in the **tools you already use** — without
a company server and **without lying** about offline, NAT, or anonymity.

The protocol stays **small enough to hold**: a **two-sided A4** (or one clear
diagram + short pages — see [`docs/spore-v1.png`](spore-v1.png)) that
still matches the wire; **continuity** so a seed and a cold machine can
rejoin the mesh; **rebuild paths** that do not depend on us existing as a
company.

---

## Decision test (use on every PR, feature, and doc)

> Does this help people run their **own delivery path** and use **familiar
> tools** on it — **honestly** — without breaking **portability, continuity,
> or the small-spec promise**?

| Answer | Action |
|---|---|
| **Yes** | Consider |
| **Only if we fake online / anonymity / membership** | Reject or redesign |
| **Only for a lifestyle app, not the substrate** | App layer, not core |
| **Bloats the mental model past "two pages + continuity"** | Split, defer, or reject |
| **No** | Do not ship |

---

## We are building

1. **A delivery substrate** — signed envelopes, store-and-forward mesh,
   multi-bridge underlays, content-addressed files, RPC, topics, seal/ratchet.
2. **A live plane (Direct)** — multi-transport datagrams; signaling in
   envelopes; NAT traversal solved **once** (reflexive candidates, hole-punch,
   explicit relay). See [`docs/DIRECT.md`](DIRECT.md) and
   [`docs/ROADMAP.md`](ROADMAP.md)'s P-Direct-NAT track.
3. **Façades** — localhost bridges so browsers, mail clients, softphones,
   XMPP, folders, and `spore://` keep working on top of SPORE.
4. **Nodes people can run** — Android, desktop, browser/wasm, daemon,
   ESP/home router. **Not iOS.** One **core**, many **runtimes** — a language
   binding, an OS process, a browser worker, an MCU firmware — each supplying
   the same four **nutrients** (randomness, time, storage, scheduling) across
   the transport boundary. Runtimes vary; the nutrients do not. See
   [`docs/DESIGN.md`](DESIGN.md)'s "The spore and the soil" for the model
   and the word legend these docs hold to.
5. **A holdable protocol story** — two-sided A4 / `spore-v1` one-pager that
   stays true to [`docs/SPEC.md`](SPEC.md); not a 200-page religion to
   *use* the system.
6. **Continuity** — seeds, Seed Sheet / paper paths, offline standalone node,
   cold-start without our servers; [`docs/CONTINUITY.md`](CONTINUITY.md)
   is product, not a side essay.
7. **Rebuild without us** — public domain, vectors, reference T0 decoders,
   [`docs/REBUILD.md`](REBUILD.md)/vendor paths so the mesh outlives any
   single maintainer.

Pillars 6 and 7 answer different questions on purpose — see
[`docs/CONTINUITY.md`](CONTINUITY.md) for how they and the reference
decoders and every release's own offline bundle fit together.

Chat UI is **one client**, not the product definition.

---

## We are not building

| Non-goal | Why |
|---|---|
| A company account / cloud inbox | Personal infrastructure |
| iOS targets | Locked out of scope |
| Instant delivery with no path | Physics; honest UX only |
| Tor / global anonymity by default | Optional mix modes only; never silent |
| Signal-grade groups without membership crypto | No fake roster UI |
| "SPORE is the only app you need" | Façades and existing protocols preferred |
| Spec sprawl as the user-facing product | Two-page / diagram honesty; depth stays in SPEC for implementers |
| Continuity that requires our website forever | Offline HTML, paper, USB, local daemon |

---

## Honesty contract

- **Offline is first-class**, not a failure mode.
- **No path** ⇒ say so; live media fails closed or falls back to async.
- **🧪 / Still open / Needs device** — no marketing past evidence.
- **Confidential ≠ anonymous.** Anonymity is an **opt-in** mix preference,
  with limits.
- **Clearnet exit** ≠ anonymity; default off; operator IP is exposed.
- **Shared family key** ⇒ shared write power until real membership exists.
- **Spec sheet matches wire** — if the A4/diagram lies, fix the doc or the
  claim, not the user's expectations.
- **Continuity is tested** — seed → new device/browser paths are real, not
  brochure-only.
- **A runtime declares what it cannot do.** No disk, no background life once
  the tab closes, no radio — the runtime says so and the feature is absent,
  never a control with nothing behind it. A thin runtime is a profile, not a
  degraded build.
- Freeze: wire, C ABI, vectors — change only with explicit process (see
  [`CONTRIBUTING.md`](CONTRIBUTING.md)).

---

## Portability & continuity (steering detail)

| Asset | Role |
|---|---|
| **Two-sided A4 / spore-v1 diagram** | Whole protocol in one holdable artifact; keep updated when v1 surface changes |
| **[SPEC](SPEC.md) + [DESIGN](DESIGN.md)** | Implementer truth; not required reading to *send a note* |
| **[CONTINUITY](CONTINUITY.md)** | Seed, paper, cold start, standalone node, "network without us" |
| **[REBUILD](REBUILD.md) / [reference](../reference/README.md) / vectors** | Independent reimplementation and verify |
| **[Standalone HTML](../web/README.md) / offline bundle** | Run a node with no app store and no CDN |
| **Public domain** | No license cliff; no corporate kill switch |

Features that cannot be explained without abandoning "small protocol +
continuity" need a **hard justification** or they stay out of core.

---

## Steering priorities (when conflicts arise)

1. **Truth** — correct model over pretty UX.
2. **Substrate + façades** — leverage existing apps before new lifestyles.
3. **Small-spec + continuity** — holdable protocol story and cold-start paths
   stay viable.
4. **Proof** — device/hardware evidence over green CI alone.
5. **Direct that works on real NATs** — one shared stack, not per-app ICE.
6. **Nodes at the edge** — daemon/ESP/home on when phones sleep.
7. **Findability** — protocol name SPORE; public subtitle so we are not the
   EA game.

---

## One-sentence forms (reuse)

| Context | Line |
|---|---|
| Mission | Personal infrastructure: your devices, any path, existing tools, no company server, no lies — small enough to hold, strong enough to continue offline. |
| Decision | Own path + familiar tools + honesty + continuity/small-spec? |
| Anti-mission | Not an account platform, not Tor, not iOS, not fake-online chat, not a protocol you must rent or relearn from a corporate wiki. |
| Continuity | A seed and a cold machine can rejoin; the mesh does not depend on us. |

---

## For implementers and agents

Every change must answer, in the PR or commit notes:

1. Which mission clause does this serve?
2. Core substrate, façade, or single app?
3. What could we be over-claiming (path, NAT, anonymity, groups, hardware)?
4. Does this **break or bloat** the two-page/A4 story or **continuity**
   cold-start? If yes, justify or split out.

If the answer is fuzzy, **stop** and tighten the design.

**This file outranks feature enthusiasm.** Roadmap items that fail the
decision test get cut or rewritten.

**Read order for agents:** `MISSION.md` → handoff (if any) →
[`ROADMAP.md`](ROADMAP.md) + [`CHANGELOG.md`](CHANGELOG.md) →
SPEC/CONTINUITY only as needed. See also
[`DEV_GUIDE.md`](DEV_GUIDE.md) for the full repo map.
