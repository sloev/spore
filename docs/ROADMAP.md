# SPORE roadmap — milestones

**Project:** `sloev/spore` · **Version:** 0.6.0 (`Cargo.toml`).

This is the single forward-looking plan, organised as **milestones** rather than
a flat PR map. Each milestone is a coherent body of work with a clear definition
of done; PRs are the merge units inside a milestone, not the plan itself.

"What shipped" lives in exactly one place — [`CHANGELOG.md`](CHANGELOG.md)
`## Unreleased` and the **Status** column in each milestone — so no second
progress table can drift. Shipped work keeps its CHANGELOG entry and loses its
spec here; the code is the truth.

**Read order for agents:** [`MISSION.md`](MISSION.md) → this file →
[`CHANGELOG.md`](CHANGELOG.md) → SPEC/CONTINUITY only as needed. See
[`DEV_GUIDE.md`](DEV_GUIDE.md) for the full repo map.

---

## Hard rules (do not violate)

- **Frozen:** wire format, C ABI (`bindings/spore.h`), `reference/vectors.json`, and
  the API surface in `tests/api_freeze.rs`. No change without the `allow-frozen-change`
  label.
- **Honesty over polish:** 🧪 markers, "Still open", served-vs-fetching language, and
  **no fake UI** — never a control whose backend is missing.
- **HARDBRUT upstream (`supernihil/hardbrut`) is normative** for colour, contrast,
  motion and components — SPORE no longer maintains its own design-language document;
  `web/vendor/hardbrut/hardbrut.css` is vendored at build time and trusted as-is. The
  one SPORE-specific rule HARDBRUT has no opinion on: the icon system is Antenna +
  Seed. Never signal failure by colour alone.
- **Zero external network requests** in `web/spore-standalone.html` (CI greps for it).
- Motion fully static under reduced motion / `ANIMATOR_DURATION_SCALE == 0`. Sound and
  particle bursts stay **off** until the user enables them.
- One concern per PR (CI-green alone). Security and design-language work are orthogonal
  when independent.
- Distinguish **Verified** (code/CHANGELOG) vs **Reasoned** vs **Needs device run**. Do
  not claim hardware verification that was not run, or invent protocol features.
- **No mushrooms.** The only brand icon is Antenna + Seed.

---

## Milestone 0 — Repo & docs hygiene ✅

**Goal:** a clean root and a docs/ folder that holds every doc except `README.md`.

| Task | Status |
|---|---|
| Move `MISSION.md`, `SECURITY.md`, `CHANGELOG.md`, `CONTRIBUTING.md` into `docs/` | ✅ shipped |
| Update all internal links, CI workflows (release, android, pages), site generator, CODEOWNERS | ✅ shipped |
| Root holds only `README.md`, source, build files, licenses, config | ✅ shipped |
| Rewrite `VISUALDESIGN.md` for the new design language (Antenna + Seed, three sizes, density, screen structures) | ✅ shipped |
| Rewrite this ROADMAP into milestone form | ✅ shipped |

**Definition of done:** `ls` at repo root shows only `README.md`, source dirs,
build files (`Cargo.*`, `deny.toml`, `rustfmt.toml`), `LICENSE`, `spore.example.yaml`,
and essential config (`.github/`, `.gitignore`). Site build + token generation green.

---

## Milestone 1 — Security & correctness

**Goal:** close the last forward-secrecy and store-bounds gaps; make every spec
claim the code actually honours.

| Task | Status | Notes |
|---|---|---|
| Ratchet skipped-key cache age-bounded (7 d) + zeroized on drop | ✅ shipped (#40) | S-024a |
| Offline crypto lifetime knobs (prekey + ratchet skip TTL) + Android UI + decrypt-failure messaging | ✅ shipped (#71) | FS/DTN honesty; default 7 d, configurable to 14/30/custom with warning |
| Ratchet wired into real DM traffic (`send_direct`/`open_dm`) | ✅ shipped (#70) | Bootstrap from ANNOUNCE; deterministic initiator |
| Store spilled-id verify (content-addressed integrity on spill) | ✅ shipped (#47) | C-ST4 |
| Paths `purge 7 d` + `Paths::trim(MAX_PEERS)` backstop | ✅ shipped (#113) | The one peer-keyed map `enforce_bounds` missed |
| Store horizon clamp to 30 d at the single choke point (`store_put`) | ✅ shipped (#113) | Matching clamp on dedup retain |
| Field-verify the offline window end-to-end on a device | ⬜ deferred to hardware QA | Unit tests prove deadline/clamping; needs a real clock/delivery run (M4) |
| Backup exclusion + migration tested on hardware | ⬜ deferred to hardware QA | No device in CI; tracked in `android/TESTING.md` |

**Definition of done:** every SPEC claim about forward secrecy and store bounds is
backed by a test; the `## Unreleased` SECURITY_FINDINGS Still-open list has no P0
items that are not either fixed or honestly marked deferred-to-hardware.

---

## Milestone 2 — Core functionality & reliability

**Goal:** the phone node and daemon are credible daily drivers on the transports
that are verified, with honest limits on the ones that aren't.

| Task | Status | Notes |
|---|---|---|
| Hub `unregister` + bridge stop/remove (all bridge kinds) | ✅ shipped (#42, #74, #75) | Audio, BLE (Meshtastic/RNode), Wi-Fi Direct, Web, core-owned UDP/TCP |
| Bridge enable/disable toggle distinct from Remove | ✅ shipped (#74) | Audio + BLE radios; Wi-Fi Direct/Web deliberately Remove-only (documented) |
| Service / Audio / BLE lifecycle | ✅ shipped (#44) | A-S1, A-A2, A-B1/B2/B6, A-W1 |
| Chat attachments: stage → one bubble → preview → FileProvider Open | 🟡 partial (#41) | Core shipped; carried: multi-file, ExoPlayer, edit-after-send, public-file single bubble |
| Name others see + local avatar + mesh profile pull | ✅ shipped (#45, #46) | 4a local, 4b mesh pull |
| Bridge status enum + permission recovery | ✅ shipped (#68) | B6 |
| Send/error feedback (no silent no-op) | ✅ shipped (#61) | B2 |
| Empty states + PUBLIC/broadcast confirm | ✅ shipped (#62) | B3 |
| Notifications + transfers overflow | ✅ shipped (#66) | B4 |
| Ring health + cautious export | ✅ shipped (#67) | B5 |
| Accessibility + density pass | ✅ shipped (#69) | B7 |
| Feed polish | ✅ shipped (#72) | B8 |
| SPORE Direct: negotiated E2E pipe (core + UDP/TCP adapters) | ✅ shipped (#50–#52) | SPDR codec, key schedule, AEAD record; LAN-scoped |
| Direct NAT traversal: reflexive (STUN) + punch + iroh relay + global IPv6 | ✅ shipped (#114) | Punch proven on loopback only; two-real-NATs procedure in `HARDWARE.md` row 19 |
| Direct wired into daemon + Android (signalling glue PR8c) | ✅ shipped | LAN-scoped until NAT step 2 |
| Iroh bridge (QUIC p2p + relay fallback) | ✅ shipped (#53) | MSRV→1.85, feature-gated, own CI job |
| Runtime storage nutrient (`SpillBackend` trait) | ✅ shipped (#87) | Unblocks browser/ESP spill |
| Runtime scheduling nutrient | ✅ shipped (#90) | Tick contract |
| demod_out cap (unbounded audio-output queue) | ✅ shipped | Bounded at 64, drops oldest |
| Conformance: browser↔native over QUIC/WebTransport (reuses iroh path) | ⬜ open — **spike validated** | Spike `spikes/001-webtransport-native` confirms feasible: a feature-gated `wtransport`+`quinn` native server + a browser `web/transports/webtransport.mjs` shim, mapping onto `DatagramPort` like `IrohPort`. Constraint: iroh's `noq` QUIC ≠ HTTP/3 WebTransport, so the native side is a *new* QUIC listener, not a reuse of iroh's endpoint — only the `DatagramPort` abstraction and Direct signalling are reused. `rustls`/`ring` already in tree via iroh; `quinn` is net-new (second QUIC stack) — feature-gate like `bridge-iroh` |

**Carried-forward functional gaps (still real, not regressions):**

- [ ] Multiple files per send (v1 is one attachment per message)
- [ ] ExoPlayer audio/video inline preview + playback
- [ ] Edit / remove an attachment after send
- [ ] Merge the bubble for public/unsealed files (needs `nativeEnvId` at `route()` time)
- [ ] Edit a bridge in place (today Remove + re-add; needs a native mutate helper)

**Definition of done:** a minimum-credible phone node — M1 + attachments usable
end-to-end + bridges stoppable/removable + one device-matrix pass (backup
exclusion + migration). Direct connects on a LAN and degrades honestly on a WAN.

---

## Milestone 3 — Design language implementation

**Goal:** every surface adopts Antenna + Seed, the three control sizes, the density
rules, and the screen structures the design language called for at the time.
Superseded by **Milestone 6** (HARDBRUT); kept here as a historical record — the
old SPORE-authored design document this milestone shipped is retired.

This is a **first-class milestone**, not scattered "nice-to-have" items. The
tokens already exist and are generated (C3/C5-token half shipped #118/#119); the
work below is the code half that changes what is on screen.

| Task | Status | Notes |
|---|---|---|
| Design tokens single-sourced + generated into all surfaces (C3) | ✅ shipped | `design/tokens.json` → `generate.py`; CI drift job |
| Control metrics generated (CONTROL 48 / CHIP 32 / ROW 56) (C5 token half) | ✅ shipped (#118) | Heights/paddings/radii/spacing guarded by drift job |
| Usage matrix — what each control is *for* (C5 matrix) | ✅ shipped (#119) | Generator enforces the count and the touch-floor rule |
| Android `Chip` + `ListRow` primitives; route ad-hoc sizes through them (C5 Kotlin half) | ✅ shipped (#133) | Chip (32dp preset) + ListRow (56dp row) |
| Density & type hierarchy pass (C4) — ≤1 instructional sentence, progressive disclosure | ✅ shipped (#134) | Compact status, details disclosure, Mail h2 removed |
| Bridges & Advanced information architecture (C6) — uniform rows, grouped sections | ✅ shipped (#136) | ListRow-based BridgeRow, Chip toggles |
| Empty-state & status-line diet (B11) | ✅ shipped (#135) | Baud mascot on all panels, compact status |
| Replace mushroom icon with Antenna + Seed on Android | ✅ done | `ic_spore.xml` now Antenna + Seed |
| Replace mushroom icon with Antenna + Seed on web node | ✅ done | Favicon (data URI) + header mark in `build-standalone.mjs` |
| Replace mushroom icon with Antenna + Seed on site (favicon, hero, nav) | ✅ done | `site/antenna-seed.svg` + brand mark in `build.mjs` + `style.css` |
| Persistent identity + status header on web node (WV0) | ✅ shipped (#130) | Tokens, identity header, Baud empty states |
| Web node IA — distinct surfaces Mail / Feed / Bridges / Seed (WV1) | ✅ shipped (#132) | Tabbed navigation with 5 panels |
| Site design-language execution (Site-2) — usage matrix + density everywhere | ✅ shipped (#138) | Hard edges everywhere (2px radius) |
| Site navigation chrome + human/builder paths (Site-3) | ✅ shipped (#137) | 5 nav items: Try it, How it works, Get a node, Spec, Web node |

**Acceptance (across the milestone):**

- [ ] Only three interactive control heights exist, in the tokens and in the code.
- [ ] Bridges uses uniform list rows; no heterogeneous full-width button stack.
- [ ] Advanced is sectioned rows, not a tall card stack.
- [ ] No working screen opens with more than one short instructional sentence.
- [ ] Status chrome is compact — `0 peers · 65 stored`.
- [ ] The site has persistent, clear navigation.
- [ ] The web node has a persistent identity + status header.
- [ ] Reduced motion is fully static; the standalone still makes zero external requests.
- [ ] Baud appears only on empty states and completions.
- [ ] The only brand icon is Antenna + Seed; no mushroom anywhere.

**Definition of done:** every surface passes visual review and the mushroom icon is
gone from the repo's rendered assets. (Historical: at the time this milestone shipped,
the checklist lived in the now-retired `docs/VISUALDESIGN.md` §8; superseded by M6/M7,
which hold HARDBRUT upstream normative instead.)

---

## Milestone 4 — Webnode as daily driver

**Goal:** the browser is a full daily-driver peer (Chats / Feed / Files /
Bridges / Seed), not a transport demo. The first runtime to consume M1's storage
seam and the communicator-as-façade pattern.

**Surfaces (locked IA):** **Chats** (unified: 1:1 DMs + open groups + private
groups) · **Feed** (personal microblog + subscribed feeds) · **Files** ·
**Bridges** · **Seed**. The old Mail / Topics / Sealed-Topics panels are merged
into Chats; the old shared `spore/feed` topic is replaced by per-address feeds.

> **Guardrail:** this stays a *reference* client, not "the" SPORE app. No feature
> here requires the standalone HTML specifically; a bridge, a Python script, or
> the daemon CLI must remain equally capable.

**Terminology (locked):** the communication surfaces map to known idioms, not
protocol jargon. The protocol primitives are unchanged — this is IA and UI only.

| Surface | Protocol primitive | Encryption |
|---|---|---|
| One-to-one chat | `send_direct` / `open_dm` | Sealed (prekey / ratchet) |
| Open group | `publish` + `subscribe` | None (public) |
| **Private group** (authorized channel) | `topic_seal` / `topic_open` + `subscribe` | PSK (sealed topic); "member" = key holder |
| Microblogging | `publish(feed::<addr>)` + `poll_feed` | None (public) |

Two communication surfaces: **Chats** — a unified list of all conversations
(1:1, open groups, **private groups** — the authorized channel) with type badges
and a new-conversation picker — and **Feed** — your personal microblog
(`feed::<your_addr>`) plus subscribed feeds. Files, Bridges, and Seed remain
separate surfaces.

**Private group vs. public microblog — locked.** A private group *is* an
authorized feed: posts are sealed with a shared PSK, so "member" means "holds
the key," never "on a verified roster." There is no separate "authorized feed"
surface — the private group *in the Chats list* is the authorized channel.
Revocation is by key rotation and is forward-only (SPEC §7.1): rotation denies
going-forward reads but cannot recall a copied key, and SPORE holds no member
list, so a "revoked member" is never claimed. The invite flow shares the key
blob safely; this and the documented revoke limit are W7.

| Task | Status | Notes |
|---|---|---|
| Encrypted DM — wasm exports (announce, send_direct, send_direct_sealed, open_dm, env_flags, env_src) | ✅ shipped (#116) | ABI half; sealed on the wire, sender authenticated |
| Encrypted DM — thread list, compose, delivery honesty (no read receipts) | ✅ shipped (#131) | UI half; thread list, DM compose, honest decrypt |
| Open group chat: join/create + public shout, clearly labeled public (W2) | ✅ shipped (#139) | Topic list, per-topic log, PUBLIC badge |
| Private group (the authorized channel): shared-key/invite-blob room, "anyone with the key can post" banner (W3) | ✅ shipped (#141) | spore_topic_seal/open in wasm; resides in the Chats list, not a separate surface |
| Microblog: publish to `feed::<addr>`, follow = subscribe (W4) | ✅ shipped (#142) | spore_node_publish/poll_feed, Feed tab with live poll |
| Files: publish → magnet, fetch by magnet, progress UI, local search (W5) | ✅ shipped | spore_node_publish_file/fetch_file/list_files, Files tab |
| Chat IA — unified conversation list: 1:1 + open groups + private groups in one list, type badges (1:1 / OPEN / PRIVATE), new-conversation picker, merge Mail + Topics + Sealed panels (W9) | ✅ shipped | Web node: 6 tabs → 5 (Chats, Feed, Files, Bridges, Seed). No protocol change |
| Microblog IA — personal feed (`feed::<your_addr>`), subscribe by address (not shared `spore/feed` topic), merged subscribed-feeds timeline (W10) | ✅ shipped | Per-address feed naming; poll_feed now returns the authenticated `from`; groups + feeds demux on the topic hash |
| Formatting + attachments — markdown (bold/italic/code/link) + file embed (magnet reference) in both chats and microblog (W11) | ✅ shipped | Client-side markdown (web/ui/markdown.mjs), XSS-safe (escape-before-markup); magnet:<> renders a download link |
| WYSIWYG everywhere — a formatting toolbar (bold / italic / code / link) over every writer: 1:1, open group, private group, microblog (W12) | ⬜ todo | Shared toolbar feeding the single chat composer + feed composer; Android parity |
| W9–W11 Android parity — Chats list adds private groups; Feed adds per-address subscribe; formatting in chats | ⬜ todo | Android Chats already mixes DMs + PUBLIC; add private-group rows + per-address feed |
| Public folder + `spore://` resolver (W6) | ⬜ todo | Sandbox foreign HTML (XSS) |
| Private-group invite flow + documented revoke-by-rotation limit (W7) | ⬜ todo | Private group *is* the authorized feed; invite = safe key-share; revoke = rotation, forward-only |
| Continuity polish: export seed from new UI, docs updates (W8) | ⬜ todo | |

**UI across runtimes (locked decision):** two UI implementations over three shared
layers — browser/desktop share the web UI (in-process wasm vs localhost HTTP to
daemon); Android stays Compose. Desktop is `daemon + web UI`, optionally wrapped in
Wry (not Tauri). Do not wrap the standalone in a window and call it desktop.

**Definition of done:** two browsers on a LAN can DM, open group chat, private
group chat (with key), microblog-post (to their own `feed::<addr>`) and
subscribe to each other's feeds, publish + fetch a file — all through the
standalone HTML, no daemon involved — and still be a full node (store, bridges,
same envelopes). Chats is one unified list; Feed is personal + subscribed.

---

## Milestone 5 — Polish & hardening

**Goal:** the sweep-up after the above. **Only start this milestone once M1–M4 are
done.** Nothing here is load-bearing for a credible node.

| Task | Status | Notes |
|---|---|---|
| Ring health UI + Export with FS warning | 🟡 ring health shipped (#67); export polish open | |
| Private group `key_id` divergence badge | ⬜ todo | Warn on mismatch in a sealed group chat; never claim roster consensus |
| Boot receiver (optional, default off) | ⬜ todo | |
| Sound + particles behind a setting, default off | ⬜ todo | Gated by §0.2/§8 |
| Android bridge list ⊆ BRIDGES.md sync check | ⬜ todo | Honesty check |
| `with_node` reentrancy guard | ⬜ todo | Low; documented, not prevented |
| Beacon duty-cycle measurement | ⬜ todo | HARDWARE.md procedure |
| Two-real-NATs Direct punch verification | ⬜ todo | `HARDWARE.md` row 19; loopback-only today |
| Hardware matrix pass (backup exclusion + migration + 7-day FS) | ⬜ todo | Needs a device; `android/TESTING.md` checklist exists |

---

## Milestone 6 — HARDBRUT visual language

**Goal:** replace the Neo-Tokyo Tactical Wasteland design language with
**HARDBRUT** (`supernihil/hardbrut`, v0.6) across all three surfaces — the web
node, the Pages site, and the Android app. HARDBRUT is a light-first
neubrutalist system: cream paper `#fdfaf2`, black ink, yellow `#ffd23f` actions,
**zero border-radius** (except true circles), and hard offset shadows
(`5px 5px 0 #000`, no blur) that stay on every element and vanish only during a
press. Two button kinds — default (yellow) and cancel (white). Auto dark mode.

**Locked decisions (so the three surfaces cannot drift):**

| Question | Decision |
|---|---|
| Does HARDBRUT replace the Neo-Tokyo palette? | **Yes, entirely.** `--void`/`--phosphor`/`--pink-on-olive` and the CRT look are retired. `design/tokens.json` is rewritten to HARDBRUT tokens and regenerated into all three surfaces. |
| Antenna + Seed icon | **Kept.** It is brand identity, orthogonal to palette; HARDBRUT has no logo opinion. Rendered ink-on-paper (mono) rather than phosphor-on-dark. |
| Baud mascot | **Kept**, restyled to HARDBRUT (flat black ink, yellow accents, hard outline) — still empty-state/completion only. |
| Zero external requests / reduced motion | **Unchanged — CI-enforced hard rules.** HARDBRUT already gates motion on `prefers-reduced-motion`; the standalone must stay self-contained (no webfonts, no CDN). |
| Impact display face | HARDBRUT's `--font-display` is `Impact, "Arial Narrow Bold", Haettenschweiler` — a system stack, no webfont, which satisfies constraint 1 exactly as the old stack did. |
| The `--prose` long-read token | **Dropped.** HARDBRUT body copy is full ink on paper — already the most readable pairing, no desaturation needed. |
| "Never pink on olive", the old contrast table | **Retired with the palette.** Replaced by HARDBRUT's own measured pairs (black on `#fdfaf2` ≈ 18.64:1; yellow `#ffd23f` on black ≈ 12.74:1). The generator is updated to assert *these*. |

**Tasks** (each a PR; tokens first, then surfaces, then the spec):

| Task | Status | Notes |
|---|---|---|
| Rewrite `design/tokens.json` to HARDBRUT values + regenerate `site/style.css`, `web/build-standalone.mjs`, Android `Chrome.kt`, and VISUALDESIGN's contrast table | ✅ shipped | `design/generate.py` inverted to light-first; `--ink #000`, `--paper #fff`, `--bg #fdfaf2`, `--yellow #ffd23f`, `--muted #666`, radius 0, border 3px, throw 5px, plus an `--onyellow` dark-mode token. CI drift job keeps them in sync |
| Web node → HARDBRUT (css tokens + components: two buttons, zero radius, hard shadows, restyled header/mascot) | ✅ shipped | Inline `<style>` in `build-standalone.mjs`; zero external requests + reduced-motion kept; Baud restyled flat; Antenna+Seed recoloured |
| Site (`site/style.css` + `build.mjs` + `home.md`) → HARDBRUT | ✅ shipped | Solid paper header + 4px ink bottom border; zero radius; hard `var(--shadow)`; CRT VFX removed; SVG illustrations recoloured |
| Android (`Chrome.kt` + all Compose screens) → HARDBRUT | ✅ shipped | Flat two-theme Palette (suffixless light + `Dark`-suffixed dark); scanslines/bloom removed; crate = zero-radius paper + hard shadow; two button kinds via `CrateButton` face |
| Rewrite `docs/VISUALDESIGN.md` to the HARDBRUT language (new tokens, components, contrast, screen structures) | ✅ shipped | Intro, §1 heading, §3 components and §4 VFX rewritten; the old Neo-Tokyo §1/§3/§4 content superseded |
| Android adaptation guide committed into the repo | ✅ shipped | `docs/HARDBRUT-ANDROID.md` (token mapping, hard-shadow workaround, two button kinds, typography) |

**Definition of done:** all three surfaces render HARDBRUT (cream paper, black
ink, yellow primary / white cancel, zero radius, hard no-blur shadows held on
every element); Antenna + Seed persists ink-on-paper; Baud is restyled; the
standalone still makes zero external requests and is fully static under reduced
motion; the drift job regenerates HARDBRUT tokens into all three surfaces and
passes.

---

## Milestone 7 — HARDBRUT as the framework (build-time import), not a copy ✅

**Goal:** stop maintaining a *forked copy* of HARDBRUT inside SPORE's own CSS.
Today `design/tokens.json` + `generate.py` re-emit a subset of HARDBRUT tokens
into `site/style.css`, the standalone's inline `<style>`, and Android's
`Chrome.kt` — a hand-maintained clone that forks the moment `supernihil/hardbrut`
moves. M7 replaces all of it with the **real `hardbrut.css` vendored at build
time**, and rebuilds both web surfaces' markup around HARDBRUT's actual classes
(`.navbar`, `.hero`, `section`, `button`, markdown, `data-accent`, `data-theme`).

**Build-time import (locked).** The web build pulls `hardbrut.css` from
`supernihil/hardbrut` *during the build* and inlines it into `site/style.css`
and the standalone. A change to the HARDBRUT repo is reflected on the next
rebuild — no runtime `@import`, so the standalone keeps its **zero-external-
request** CI guarantee. The vendoring dir and the remote/ref are pinned and
documented so the import is reproducible, not a silent network dependency of
every CI run.

**Android (locked).** The Android app uses HARDBRUT only as the *foundation* of
the existing Compose theme — it already consumes the tokens; M7 makes the
Android side regenerate from the same vendored source rather than a hand-edited
copy, but keeps the Compose primitives (`Chrome.kt`, hard-shadow workaround, two
button kinds) that XML cannot express.

**Tasks** (each a PR):

| Task | Status | Notes |
|---|---|---|
| Vendor `hardbrut.css` into the repo at build time (pinned remote + ref, inlined by `build-standalone.mjs` and `site/build.mjs`) | ✅ shipped (#146) | Delete the SPORE-authored token/CSS fork; keep Antenna+Seed + Baud as assets, now styled by HARDBRUT classes. `ref: 'main'` — HARDBRUT latest is always the source of truth; `node web/hardbrut-sync.mjs` re-pulls the committed vendored copy on demand (build itself never fetches live, so CI stays deterministic and the standalone stays zero-request) |
| Scrape the standalone HTML down to barebones markup and rebuild it on HARDBRUT classes (`section`, `navbar`, `button`, `.card`, markdown) | ✅ shipped (#146) | `build-standalone.mjs`'s inline `<style>` is HARDBRUT + a minimal app-shell adapter (tab bar, log, WYSIWYG toolbar — concepts HARDBRUT has no equivalent for); the SPI/WYSIWYG/(W12) logic is unchanged, presentation only |
| Rebuild the Pages site on HARDBRUT classes; remove `gen_site_css` hand CSS | ✅ shipped (#147 + this pass) | `site/style.css` deleted outright (not kept as an `@import` shell); `site/build.mjs` inlines vendored `hardbrut.css` + a thin adapter (doc reading width, code-copy button, print). Markup rebuilt on `.navbar`/`.hero`/`.grid`/`.card`/`.btn`/`.cluster`; a working `.navbar-toggle` + `.open` toggle script makes the nav responsive on mobile. All hand-drawn `<svg>` story-card illustrations (home, Apps, Continuity) removed — cards are plain HARDBRUT `.card`s, text only. Antenna+Seed brand mark and the Baud mascot are not illustrations and stay |
| Android regenerates its palette from the vendored source; drop the copied token table in `design/generate.py` | ✅ shipped | `design/generate.py` now parses `web/vendor/hardbrut/hardbrut.css`'s `:root`/`[data-theme="dark"]` blocks directly and emits `Chrome.kt`'s `Palette` from them — no hand-typed second copy. Caught a real drift in the process: the old hand-typed `OnYellow` (`#121210`) didn't match HARDBRUT's actual `--accent-ink` (`#000`). `Chrome.kt` keeps its hard-shadow / two-button primitives; no XML rewrite |
| Remove the now-redundant `design/tokens.json` + `gen_site_css` token emission; the drift job becomes "vendored css is in sync with the pinned ref" | ✅ shipped | `gen_site_css`, `gen_standalone_css`, `gen_visualdesign_md`, the WCAG contrast-checking machinery, and the `site`/`standalone` `tokens.json` surface entries are all gone — there's no SPORE-authored contrast claim left to protect. `tokens.json` keeps only the Android-only control-size table (control/chip/row heights, touch floor), which has no HARDBRUT source to regenerate from. The "design tokens in sync" CI job now has two steps: `node web/hardbrut-sync.mjs` verifies the vendored copy matches the pinned `main` ref (the one job allowed to touch the network), then `design/generate.py` verifies Android's `Palette` matches that vendored copy |

**Definition of done:** `site/build.mjs` and the standalone's CSS are the vendored
`hardbrut.css` (plus a thin SPORE-asset layer), not a fork; editing
`supernihil/hardbrut` and rebuilding SPORE changes both web surfaces; the
standalone still makes zero external requests; Antenna + Seed and Baud persist;
Android keeps its Compose primitives on the same foundation.

---

## Explicitly out of scope / non-goals (locked)

| Item | Decision |
|---|---|
| **iOS** | Not a target, ever. State this on `APPS.md` so the expectation dies early. |
| **Instant delivery with no path** | Impossible under store-and-forward. UI says "no path yet" / fails closed for live media; async fallback (voice note) is fine. |
| **Group membership consensus protocol** | A shared-key private group chat is shippable now; a Signal-style roster is a deliberate future protocol project, not a UI feature to fake. |
| **Tor / global anonymity by default** | Optional mix modes only; never silent. Anonymity is an explicit, non-default toggle (mix-preferred / mix-only). |
| **Multi-file attach, in-app video, post-send edit** | v1 non-goals; tracked in M2 carried-forward. |
| **Wire / C ABI changes** | Frozen; `allow-frozen-change` for a 2.0 only. |
| **Routing Direct records through store-and-forward relays** | Direct is non-routed by definition. |
| **Compile-time `max_core` gating (C0–C8 cargo features)** | Declined — ratchet session map is inline on `Node`; revisit only if a real MCU target proves it necessary. |

---

## Product decisions (locked, not proposals)

### Anonymity — an explicit, non-default toggle

| Mode | Behaviour |
|---|---|
| Normal (default) | Seal/ratchet content as today; Direct allowed; underlay metadata as today |
| Mix-preferred | Prefer `mix` onions when mixes are known; warn if none; discourage Direct for that send |
| Mix-only | Refuse to send unless an onion path is available |

The primitives exist (`src/mix.rs`); the toggle + a runnable mix-operator example
(`P-Mix-Runner`) make the path operable. Clearnet exit is a separate, off-by-default
convenience feature, never described as anonymity.

### Direct NAT traversal — settled

iroh is the NAT answer; the hand-rolled punch is demoted to an optimisation. The
ladder, each rung only where a runtime can supply it: LAN → global IPv6 → overlay →
reflexive (punch) → iroh relay/fallback → fail honestly. CGNAT-to-CGNAT with no
working relay still fails sometimes; a relay is the permanent escape hatch, not a
cleverer punch. Claim exactly what the ladder covers, never "arbitrary NAT traversal."

### Priority compass (when picking up new work)

1. **M1 — Security & correctness** (no P0 stays open)
2. **M2 — Core functionality** (credible phone + daemon node)
3. **M3 — Design language** (Antenna + Seed, three sizes, density, screen structures)
4. **M4 — Webnode as daily driver** (first runtime on the storage seam)
5. **M5 — Polish & hardening** (only after the above)
6. **M6 — HARDBRUT visual language** (replaces M3's language across all surfaces; tokens first, then surfaces, then the spec)
7. **M7 — HARDBRUT as the framework** (vendored at build time, not a copy; all three surfaces done)

Hardware/community work (the former "Track H" — lived-in prototype, solar cyberdeck,
wear language, community harvest, maintainer culture) is deliberately **not** a
milestone: every row is `⬜ concept`, nothing in the compass depends on it, and no
row earns a 🧪 until something exists a person could hold or run. It was written up
as inspiration in the now-retired `docs/VISUALDESIGN.md` §6b, not in this plan as a
promise; no replacement doc is planned — HARDBRUT upstream has no opinion on
hardware/community concepts, so there is nothing for it to be normative about.

---

## Tasks removed or heavily changed from the old PR-map

| Old item | What happened | Reason |
|---|---|---|
| PR0 (ratchet TTL + zeroize + offline knobs) | Folded into **M1**, marked ✅ | Already shipped; spec deleted, code is the truth |
| PR1–PR9 detailed specs | Consolidated into **M2** status rows | Shipped PRs lose their specs (code = truth); open items are carried-forward gaps |
| Docs-1/2/3, D1 editorial review | Removed | Already shipped (#55–#57, #111/#112); this rewrite is the last of that work |
| C1/C3 token parity + generation | Removed (shipped) | Superseded by M3's token/usage-matrix rows |
| C4/C5/C6/B11 as separate PRs | Consolidated into **M3** | One design-language milestone, not four overlapping tracks |
| Site-2/Site-3/WV0/WV1 as separate tracks | Consolidated into **M3** | All are design-language implementation work |
| W0 (wasm API audit) | Removed (shipped) | W1 ABI half shipped (#116); the audit is done |
| W1–W8 phased PRs | Consolidated into **M4** | One webnode milestone with status rows |
| P-Runtime-1/2 | Folded into **M2**, marked ✅ | Shipped (#87, #90) |
| P-Direct-NAT | Folded into **M2**, marked ✅ | Shipped (#114); punch 🧪 until two-real-NATs test |
| P-Mix-Runner | Kept, moved to **M5** | Anonymity toggle + example operator; not load-bearing for a credible node |
| P-Group-Roster | Out of scope (locked) | Sealed topic + honest UX is the shippable answer; real roster is a future protocol project |
| Track H (H1–H7 hardware/community) | Removed from the plan | `⬜ concept` with no software dependency; lives in VISUALDESIGN §6b as inspiration |
| Suggested calendar / branch-naming sections | Removed | A milestone plan does not carry a week-by-week calendar that is immediately stale |
| PR write-up template | Removed | Shipped PRs don't need it; open PRs inherit the milestone's acceptance criteria |
| Conformance gaps section | Folded into **M2** | One row (browser↔native over QUIC/WebTransport) remains open and is now in M2 |
| Carried-forward (detailed per-PR) | Folded into **M2** carried-forward list | One list, not three per-PR subsections |
| Plan health check / audit ID index | Removed | Process artefacts of the old PR-map; the milestone structure is the health check |
| "North star" narrative | Removed | The priority compass + milestone definitions of done say the same thing in less prose |

---

*Actionable plan derived from the 0.6.0 tree, the audit tour, and the new design
language. Update when work lands or hardware results arrive.*
