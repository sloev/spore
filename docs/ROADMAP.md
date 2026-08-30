# SPORE roadmap — milestones

**Project:** `sloev/spore` · **Version:** 0.6.0 (`Cargo.toml`).

This is the single forward-looking plan, organised as **milestones** rather than
a flat PR map. Each milestone is a coherent body of work with a clear definition
of done; PRs are the merge units inside a milestone, not the plan itself.

"What shipped" lives in exactly one place — [Changelog](CHANGELOG.md)
`## Unreleased` and the **Status** column in each milestone — so no second
progress table can drift. Shipped work keeps its CHANGELOG entry and loses its
spec here; the code is the truth.

**Read order for agents:** [Mission](MISSION.md) → this file →
[Changelog](CHANGELOG.md) → SPEC/CONTINUITY only as needed. See
[Dev guide](DEV_GUIDE.md) for the full repo map.

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
- **No icon, no mascot.** SPORE's brand is the wordmark — plain "SPORE" text — on
  every surface. Nothing stands in for it, not even a monogram; Android's
  `ic_spore.xml` is a plain HARDBRUT accent swatch because the platform
  requires an icon file to exist, not because it represents anything.

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
- [x] ~~Baud appears only on empty states and completions.~~ Superseded: Baud is removed entirely — brand is the SPORE wordmark only, no mascot (see the hard rules).
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
| Android bridge list ⊆ BRIDGES.md sync check | ✅ shipped | In `check_docs_sync.py`. Fails if the app offers a bridge with no BRIDGES.md entry, or one still marked ⚪ planned — a control with no backend. Found the **TCP** bridge shipping in the app undocumented; entry added |
| `with_node` reentrancy guard | ✅ shipped | Was a silent permanent deadlock — no panic, no log, a thread parked on a lock it held itself. Now panics naming the bug. Per-hub and per-thread, so contention between bridge threads and nesting across two hubs both stay legal |
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
`supernihil/hardbrut` *during the build* and inlines it into the Pages site
(`site/build.mjs`) and the standalone (`build-standalone.mjs`) — there is no
`site/style.css` anymore. A change to the HARDBRUT repo is reflected on the next
rebuild — no runtime `@import`, so the standalone keeps its **zero-external-
request** CI guarantee. The vendoring dir and the remote/ref are pinned and
documented so the import is reproducible, not a silent network dependency of
every CI run.

**Android (locked).** Compose has no CSS to `@import`, so Android gets its own
vendored source instead: `android/app/src/main/kotlin/org/spore/node/vendor/
Hardbrut.kt`, `supernihil/hardbrut`'s official Compose port, pulled live by
`android/hardbrut-sync.py` — same "always latest, pinned ref, no runtime fetch"
contract as the CSS side. `Chrome.kt`'s tokens alias that file's `HardbrutTokens`
directly; only dark mode (which that file doesn't define) still comes from the
vendored CSS. `Chrome.kt` keeps the Compose primitives that XML cannot express
and that the drop-in file doesn't provide — press-feedback shadows, touch
targets, `Chip`/`ListRow`/`ToughbookField`/`CrateSwitch`/`SegmentedLed` — but
now builds their static shadow-drawing on the vendored `hardShadow()` rather
than a third hand-rolled copy of the same offset-rect math.

**Tasks** (each a PR):

| Task | Status | Notes |
|---|---|---|
| Vendor `hardbrut.css` into the repo at build time (pinned remote + ref, inlined by `build-standalone.mjs` and `site/build.mjs`) | ✅ shipped (#146) | Delete the SPORE-authored token/CSS fork; keep Antenna+Seed + Baud as assets, now styled by HARDBRUT classes. `ref: 'main'` — HARDBRUT latest is always the source of truth; `node web/hardbrut-sync.mjs` re-pulls the committed vendored copy on demand (build itself never fetches live, so CI stays deterministic and the standalone stays zero-request) |
| Scrape the standalone HTML down to barebones markup and rebuild it on HARDBRUT classes (`section`, `navbar`, `button`, `.card`, markdown) | ✅ shipped (#146) | `build-standalone.mjs`'s inline `<style>` is HARDBRUT + a minimal app-shell adapter (tab bar, log, WYSIWYG toolbar — concepts HARDBRUT has no equivalent for); the SPI/WYSIWYG/(W12) logic is unchanged, presentation only |
| Rebuild the Pages site on HARDBRUT classes; remove `gen_site_css` hand CSS | ✅ shipped (#147 + this pass) | `site/style.css` deleted outright (not kept as an `@import` shell); `site/build.mjs` inlines vendored `hardbrut.css` + a thin adapter (doc reading width, code-copy button, print). Markup rebuilt on `.navbar`/`.hero`/`.grid`/`.card`/`.btn`/`.cluster`; a working `.navbar-toggle` + `.open` toggle script makes the nav responsive on mobile. All hand-drawn `<svg>` story-card illustrations (home, Apps, Continuity) removed — cards are plain HARDBRUT `.card`s, text only. Antenna+Seed brand mark and the Baud mascot are not illustrations and stay |
| Android regenerates its palette from the vendored source; drop the copied token table in `design/generate.py` | ✅ shipped | Android gets its own vendored HARDBRUT source, not a CSS reparse: `android/app/src/main/kotlin/org/spore/node/vendor/Hardbrut.kt` is `supernihil/hardbrut`'s official Compose port, pulled by `android/hardbrut-sync.py` from the live `https://supernihil.github.io/hardbrut/Hardbrut.kt` (always latest, same as the web's `ref: 'main'`). `Chrome.kt`'s generated `Palette`/`Metrics` alias its `HardbrutTokens` object directly for the light palette and every border/shadow/spacing metric — not a copied colour. That file has no dark-mode variant, so `design/generate.py` still parses the vendored `hardbrut.css`'s `[data-theme="dark"]` block for just the four dark hexes — the one gap between the two vendored sources. Caught a real drift in the process: the old hand-typed `OnYellow` (`#121210`) didn't match HARDBRUT's actual `--accent-ink` (`#000`). `crate()`/`CrateButton`/`Chip`/`ListRow` now draw their hard shadow via the vendored `hardShadow()` modifier instead of hand-rolled `drawRect` calls; `Chrome.kt` keeps the press-feedback and touch-target logic the drop-in file doesn't have, and its other product-specific primitives (`Chip`, `ListRow`, `ToughbookField`, `CrateSwitch`, `SegmentedLed`, `ConfirmDialog`) — no XML rewrite. Three real upstream compile bugs (`TextTransform` didn't exist in this Compose BOM; `HardbrutButton`/`HardbrutTextField` used `ProvideTextStyle`/`BasicTextField`/`onFocusChanged` without importing them) were patched narrowly in `android/hardbrut-sync.py`'s `COMPILE_FIXES`, reported upstream as [supernihil/hardbrut#4](https://github.com/supernihil/hardbrut/issues/4), and fixed there within the same day — `COMPILE_FIXES` is empty again, the mechanism stays for next time. Upstream also shipped `HardbrutListRow`/`HardbrutSwitch`/`HardbrutChip` in the same pass, answering [#5](https://github.com/supernihil/hardbrut/issues/5)/[#6](https://github.com/supernihil/hardbrut/issues/6) |
| Remove the now-redundant `design/tokens.json` + `gen_site_css` token emission; the drift job becomes "vendored css is in sync with the pinned ref" | ✅ shipped | `gen_site_css`, `gen_standalone_css`, `gen_visualdesign_md`, the WCAG contrast-checking machinery, and the `site`/`standalone` `tokens.json` surface entries are all gone — there's no SPORE-authored contrast claim left to protect. `tokens.json` keeps only the Android-only control-size table (control/chip/row heights, touch floor), which has no HARDBRUT source to regenerate from. The "design tokens in sync" CI job now has two steps: `node web/hardbrut-sync.mjs && python3 android/hardbrut-sync.py` verify both vendored copies match their pinned refs (the one job allowed to touch the network), then `design/generate.py` verifies Android's `Palette` matches them |

**Definition of done:** `site/build.mjs` and the standalone's CSS are the vendored
`hardbrut.css` (plus a thin SPORE-asset layer), not a fork; editing
`supernihil/hardbrut` and rebuilding SPORE changes all three surfaces — the two
web surfaces on the next `hardbrut-sync.mjs` + rebuild, Android on the next
`hardbrut-sync.py` + `design/generate.py`; the standalone still makes zero
external requests; Android's `Chrome.kt` aliases the vendored `Hardbrut.kt`'s
tokens and shadow primitive rather than maintaining its own copy, keeping only
the product-specific primitives (touch targets, press feedback,
`Chip`/`ListRow`/etc.) that file doesn't provide. (Antenna + Seed and Baud
were later retired entirely — see the hard rules: the brand is the wordmark,
nothing stands in for it.)

---

## Milestone 8 — Embedded ESP32 runtime (raw-802.11 relay)

**Goal:** the first real implementation of the `Embedded (ESP32)` runtime
`docs/DESIGN.md` already names ("little memory, no filesystem, one or two
bridges") — a standalone, headless ESP32-S3 that relays real envelopes over
raw 802.11 frames, persists its store to flash, and bridges to a phone or
laptop over USB or BLE when one is nearby. Filed as
[#149](https://github.com/sloev/spore/issues/149).

**Flash it and it works; a tether is just another bridge (locked).** There is
no standalone build and no tethered build, no mode switch, no pairing and no
configuration step. A board that has been flashed and given power is already a
working node. Binding on every task below:

- **Untethered and unconfigured is the normal state.** The board boots and
  carries traffic with nothing attached and nothing set up — the radio comes up
  on a fixed default so that two boards out of the same box find each other with
  no intervention. A board dropped in a field with a battery is the design
  target, not a degraded case.
- **A connection to another device is a *bridge*, not a mode.** USB serial, BLE
  — these register on the hub exactly like the radio does, and the core floods
  between all of them because that is what it already does for every other
  bridge. Nothing in the firmware knows the word "tether". Plugging a cable in
  adds an interface to a node that was already running; unplugging removes it and
  changes nothing else.
- **So gateway behaviour is emergent, not a feature.** A phone on USB OTG serial
  gets the raw-802.11 mesh through the board, and the mesh gets the phone's
  internet, Bluetooth and folder bridges back — because the board floods between
  its two interfaces and the phone floods between its several. Neither end has a
  "gateway" code path; it is two ordinary nodes each doing the one thing the core
  has always done. **If a gateway role ever needs writing, something has gone
  wrong** — the design has stopped being bridges-on-a-hub.
- **Neither role is a build-time choice.** Selecting behaviour with a cargo
  feature would mean two firmwares to test, two ways to be wrong, and "which one
  is on this board?" as a question anyone ever has to ask.
- **Configuration cannot require a tether.** Defaults have to be good enough to
  relay out of the box, anything persistent lives in flash, and nothing waits on
  a companion app — otherwise the tether is load-bearing by the back door.

The ordering follows from this rather than the other way round: E2/E3 (radio,
store) come before E4/E5 (USB, BLE) because a bridge attaches *to* a working
node, so there has to be one to attach to.

**The daemon speaks the same air interface (locked).** Raw 802.11 is not an
embedded-only bridge. A Linux daemon with a monitor-mode-capable card runs the
same frame format (E2d below), so a laptop is a peer of the boards rather than
just something they tether to — which is what makes a mixed deployment of cheap
relays and a real machine one mesh instead of two. The frame layout is written
into [Bridges](BRIDGES.md) before either side is finished, because board-to-board
interop happens by accident when both run our code, and board-to-laptop interop
only happens if the framing is specified.

This is less new architecture than it might look. The bridge-shape taxonomy
already lists LoRa/Meshtastic as "message pipe" examples (`docs/DESIGN.md`
§"Bridges & bindings") — raw 802.11 is another one, not a new shape. The
storage nutrient (`SpillBackend` trait, M2, #87) already shipped *specifically*
to unblock browser/ESP spill — this milestone writes a `littlefs`-backed
implementation of an existing contract, not a new one. The USB bridge reuses
the existing KISS byte-stream framing (`bridge::kiss_stream`), same as the
serial/Meshtastic bridges. The new work is genuinely just the radio driver
(promiscuous RX filter + `esp_wifi_80211_tx` injection) and the BLE fallback.

**Toolchain (locked): esp-idf-sys, not bare-metal `esp-hal`.** ESP-IDF's
std-like environment (newlib) means the core likely compiles close to as-is.
A bare-metal `no_std` port would very likely reopen "Compile-time `max_core`
gating," declined elsewhere in this document with the explicit exception
"revisit only if a real MCU target proves it necessary" — this milestone *is*
that target, but starting from esp-idf-sys avoids forcing the reopening on
day one. Bare-metal `esp-hal` is an explicit non-goal for M8; a future
milestone can attempt it if esp-idf-sys proves too heavy for the target board.

**Regulatory posture (locked): documented, not enforced.** Raw 802.11
frame injection/monitor mode outside normal association, and running
encrypted traffic over any band whose rules restrict it (amateur radio's
no-encryption rule, most notably), are the operator's compliance problem, not
SPORE's to police. The bridge's `BRIDGES.md` entry states plainly what it
does and names the regulatory considerations that follow from that — same as
disclosing a mix mode's limits — but SPORE does not gate, strip, or weaken
encryption to comply with a band's rules on anyone's behalf. Silent
non-compliance would be dishonest; refusing to build the feature over a rule
the operator may not even be subject to is not this project's call to make.

**Tasks** (each a PR). Work is tagged **E1–E6** and ordered on three principles:

- **Biggest risk first.** E1 answers *does the core compile, link, and actually run
  on this board, in this much RAM, under esp-idf-sys?* Nothing else in M8 is worth
  building until that answers yes.
- **Nutrient work stays distinct from bridge work.** E1 and E3 supply the four
  nutrients this runtime owes the *existing* core contract (randomness, time,
  scheduling, storage — [Design](DESIGN.md)); E2, E4 and E5 put the board on the
  *open* bridge list. Conflating the two is how a runtime ends up inventing
  protocol.
- **Solo before paired.** Each phase names what one board alone can prove, so the
  work is not blocked on owning two of everything. Only the rows that say so need a
  second device.

**None of the three bridges is new.** Raw 802.11, BLE GATT and USB/serial-over-KISS
are all already specified in [Bridges](BRIDGES.md) — BLE down to the Nordic UART
UUIDs, USB as byte-for-byte KISS, and 802.11 with the ESP32 path and regulatory note
added alongside `Envelope::probe`. Two of them already have a working *browser* half
(`web/transports/webbluetooth.mjs`, `webserial.mjs`), so E4 and E5 are making the
board the peer for clients that already exist and already speak the framing. That is
why no row below is a design task: the shapes are decided, and what is missing is
the firmware side of each.

Issue [#149](https://github.com/sloev/spore/issues/149)'s own phase order (env setup
→ radio harnessing with a mock envelope → wire to `Node::on_rx` → USB-CDC) maps onto
E1 + E2 + E4. The flash store (E3) and the BLE fallback (E5) go beyond what the issue
scoped.

| Task | Status | Notes |
|---|---|---|
| esp-idf-sys toolchain scaffold: core builds and links for ESP32-S3, with a CI cross-compile job (E1) | ✅ shipped | `esp32/`, its own workspace root like `android/jni`. The core cross-compiles **unmodified** — no ESP `cfg` branches, no feature gates, a plain path dependency. CI builds it in Espressif's Docker image (Xtensa is not an upstream Rust target) and reports the footprint |
| Randomness nutrient: confirm `OsRng` resolves to ESP-IDF's hardware TRNG (E1) | ✅ shipped — **no shim needed** | The `cfg(target_arch = "wasm32")` getrandom block in `Cargo.toml` had no ESP counterpart to write: getrandom 0.2 supports `target_os = "espidf"` natively and routes to `esp_fill_random`. Verified by compiling; that it returns real entropy is a device-run claim |
| Time nutrient: a `now: u32` source, shipping [Spec](SPEC.md) §Time's no-trusted-clock behaviour first (E1) | ✅ **run on hardware** | A cold-booted board with no RTC battery *is* the "no trusted clock" node the spec already covers: relay regardless, age by dwell, drop after 7 local days. NTP-over-Wi-Fi is a stretch goal, not a blocker |
| Scheduling nutrient: a FreeRTOS periodic task calling `Node::tick` (E1) | ✅ **run on hardware** | Absent from the previous version of this table. Without it the runtime silently regresses to maintaining itself only when traffic happens to arrive ([Design](DESIGN.md), nutrient table) — worst on exactly this kind of solo, often-offline node |
| Solo bring-up smoke test: boot, fresh identity, one self-signed envelope logged over UART, a tick observed firing (E1) | ✅ **verified on hardware** | LOLIN S2 Mini (ESP32-S2FNR2 rev v1.0), 2026-08-25. Boots, generates an identity (`addr=8a82bcbd735aed52`), and **a signature it makes verifies on the board** (`sig=ok`) — ed25519 works on this silicon, not merely compiles for it. Tick loop on schedule; **live heap 226,368 bytes free**, the runtime figure section sizes cannot give. Checked by `esp32/diagnose.py`, not by reading a log |
| Promiscuous RX filter: SPORE v1 header match, instant-discard on miss (E2) | ✅ shipped (#171) | `Envelope::probe` — walks the header, returns the wire length or `None`, allocates nothing. Structural only: a hit means "worth decoding", never "authentic". Agreement with `decode` is asserted as a fuzz invariant, since it is a second front door for hostile bytes |
| `esp_wifi_80211_tx` injection wired as `DatagramTransport::send`, RX as `::recv`, driven through `run_datagram` (E2) | 🧪 written, not run | `esp32/src/radio.rs` (ESP-IDF glue) over `bridge::ieee80211` (the codec, portable and CI-tested, shared with E2d). Promiscuous RX filtered to management frames in hardware, then `ieee80211::parse` → bounded queue → `recv`. Builds clean; **nothing has been transmitted or received on air** |
| Solo TX-shape test: an external monitor-mode sniffer confirms the injected frame's shape (E2) | ⬜ todo | Proves the injection path without needing a second SPORE node |
| Device-pair relay: two boards exchange a real envelope over the air (E2) | ⬜ todo | 🧪 until this run happens |
| Linux daemon raw-802.11 bridge: monitor mode + injection over `nl80211`, same frame format as the board (E2d) | ⬜ todo | The other end of the same air interface, so a laptop relays with the boards rather than only talking to them over a tether. Shares the frame layout and `Envelope::probe` filter with the ESP path — one wire format, two implementations. Needs a card whose driver supports monitor + injection (`iw list` → "monitor" and "AP/VLAN"), which is a hardware constraint, not a code one |
| Frame-format note in [Bridges](BRIDGES.md): the exact 802.11 header, vendor tag and payload layout both sides implement (E2d) | ✅ shipped | Vendor-specific Action frame, category 127, on the same mechanism ESP-NOW uses — the one shape an ESP32 is known to inject without association. Fixed BSSID `02:53:50:4F:52:45` (`02` + "SPORE") and a locally-administered OUI, so two boards agree with no configuration. MTU stays 🧪: the 2304 MSDU is the standard's ceiling, not what a driver will inject |
| Flash partition setup (custom `partitions.csv`, mount/format-on-first-boot) (E3) | 🧪 written, not run | **SPIFFS, not `littlefs`** — see the note below. `esp32/src/storage.rs`. The table is applied at *flash* time, not build time: esp-idf-sys's generated CMake project lives under `target/` and cannot see a CSV in the crate root, so `CONFIG_PARTITION_TABLE_*` silently does nothing. 1500K app / 2400K store |
| ~~A flash-backed `SpillBackend`~~ — **not needed** (E3) | ✅ shipped, no new code | ESP-IDF exposes filesystems through VFS, so `std::fs` works once a partition is mounted and the core's existing `FsSpill` runs unmodified. E3 was written as "implement the existing contract, not a new one"; mounting a filesystem satisfies that literally |
| Adopt the last run's spill at boot (E3) | 🧪 written, not run | `Node::set_spill_dir` builds the backend and adopts in one call, re-verifying every id against its bytes — a file SPIFFS damaged is discarded rather than trusted, because an id *is* the hash of its content |
| Power-cycle test: spill past the memory budget, cut power, confirm the adopted set matches (E3) | ⬜ todo | 🧪 until logged in [Hardware verification](HARDWARE.md). A genuinely new row — real flash and real power loss, not merely "not CI-testable" |
| Wired tether: KISS over a byte stream, registered as an ordinary bridge (E4) | 🧪 written, not run | `esp32/src/tether.rs`. No new framing and no new transport — `bridge::kiss_stream` and `stream_link::run_split`, the same loop the desktop serial bridge runs. **UART on the S2, USB-CDC on the S3**, per board: the S2 has one USB peripheral and the console already owns it, so KISS frames would interleave with log text. The S3 has USB-Serial-JTAG *as well*, which is what makes a USB tether possible there |
| Solo loopback: laptop-side KISS echo, board sends and receives its own frames (E4) | ⬜ todo | Proves the framing with no phone in the loop. On the S2 this needs a USB-serial adapter on `tx=17 rx=18` |
| Phone tether: the Android app or web node over USB, real message exchange (E4) | ⬜ todo | 🧪 until run. [Hardware verification](HARDWARE.md) row 4 ("Web Serial → board") is the existing pattern |
| Expose the existing NUS profile from the board: KISS over Nordic UART, `stream` form (E5) | ⬜ todo | No design step needed — [Bridges](BRIDGES.md) already specifies this bridge down to the UUIDs (`6e400001-…`, RX `…0002`, TX `…0003`), KISS framing via `bridge::kiss_stream`, and the ~247-byte ATT MTU. The browser half already speaks it (`web/transports/webbluetooth.mjs`), so the board is the peripheral for a client that exists |
| Solo test: a generic BLE central exercises write/notify against the board (E5) | ⬜ todo | Confirms the characteristic layout without a phone SPORE client |
| Phone tether: Android or Web Bluetooth exchanges a real low-bandwidth message (E5) | ⬜ todo | 🧪 until run. [Hardware verification](HARDWARE.md) row 18 (BLE NUS) is the existing pattern |
| [Bridges](BRIDGES.md): move each of the three from ⚪ planned to 🧪 as its board-side half lands (E6) | ⬜ todo | The entries themselves already exist and are accurate — 802.11 gained the ESP path, the `probe` filter's role and the regulatory note in #172. What is left is a status change per bridge, landing with that bridge's own PR, not new prose |
| Full combined run: two boards, flash store live, an envelope relayed over raw 802.11, one board bridged to a phone over USB, BLE exercised as the fallback (E6) | ⬜ todo | One [Hardware verification](HARDWARE.md) row per path, matching the existing one-row-per-path convention |
| Flash-cycle re-confirmation in the combined rig: power-cycle one board mid-session, confirm it resumes relaying with its spilled store intact (E6) | ⬜ todo | Distinct from E3's isolated test — proves persistence holds while the radio and USB paths are also live |

**On [esp32-open-mac](https://github.com/esp32-open-mac/esp32-open-mac) (considered, not usable yet).**
A reverse-engineered, blob-free 802.11 MAC — written in Rust, and philosophically a
much better fit than a closed binary: [Continuity](CONTINUITY.md) and
[Rebuild](REBUILD.md) are both about not depending on things you cannot inspect,
and the Wi-Fi blob is the one component of an ESP32 node nobody can audit.
**It only runs on the original ESP32** — not the S2 or S3 — because the Wi-Fi
peripheral addresses are hardcoded, so it cannot be used for E2 on either the
documented S3 target or the S2 this is being developed on. Worth revisiting if it
ports, or if a plain ESP32 ever becomes a supported board; until then E2 uses
`esp_wifi_80211_tx` and accepts the blob.

**On SPIFFS vs `littlefs` (E3).** `littlefs` is the better filesystem here — it is
built around power-loss atomicity, exactly the property E3 exists to test — and it
was tried first. The component manager pulls it in cleanly, but `esp-idf-svc` 0.51
then fails to compile: its `io` module references `crate::fs::littlefs`
unconditionally while the module itself stays gated. Accepting a broken build for a
property that cannot be verified without a power-cycle rig was the wrong trade, so
E3 ships on SPIFFS, which ESP-IDF has built in. Revisit when that is fixed upstream.

What the difference costs is smaller than it sounds: spilled envelopes are
content-addressed and re-verified on read, so a file SPIFFS corrupts reads as "not
held" and the mesh re-fetches it — the store is a cache, not a database. The real
exposure is a failed *mount*, which loses everything at once, and that is what
`littlefs` would buy.

**Toolchain checkpoint (E1) — measured, and the decision stands.** The scaffold plus
one signed envelope costs, on a release build:

| | Bytes | Share of an ESP32-S3 |
|---|---|---|
| Flash (app image) | ~542,500 | ~13% of a 4 MB part |
| Internal SRAM (static) | 67,235 | ~13% of 512 KB |

**Update, E2:** linking the Wi-Fi stack in roughly doubled both figures —
**977,279 flash (23% of 4 MB) and 108,148 SRAM (33% of the S2's 320 KB)**. Still
comfortable, and exactly the kind of step-change the per-run CI report exists to
make visible rather than discover later.

Comfortable, and measured before the 802.11 driver, `littlefs` and a BLE stack pile
on top — which was the point of checking here rather than later. **esp-idf-sys stays
the locked toolchain**, bare-metal `esp-hal` stays a non-goal, and compile-time
`max_core` gating stays declined: its stated exception ("revisit only if a real MCU
target proves it necessary") is exactly what these numbers fail to trigger. The CI
job prints both figures on every run, so the headroom is tracked as E2–E5 land rather
than measured once and assumed.

Three caveats on what this does *not* say. These are static sections, not live heap —
runtime headroom is a device-run number the smoke test still owes. The flash figure
excludes the bootloader, partition table, and any `littlefs` partition E3 adds. And
the flash total is approximate on purpose: it moves by ~100 bytes between build
environments because panic messages bake in absolute source paths, so `CARGO_HOME`
being one directory deeper changes it. Compare runs, not the last digits.

**Definition of done:** an ESP32-S3 running this firmware relays real SPORE
envelopes over raw 802.11 to at least one other node, bridges to a phone or
laptop over USB (KISS) with BLE as the low-bandwidth fallback, and its store
survives a power cycle via flash — with `HARDWARE.md` recording a real
device-pair run, not just green CI. 🧪 until then.

---

## Milestone 9 — Threat-model legibility & anti-abuse guardrails

**Goal:** close the gap between what SPORE's protocol already does (stamp
anti-spam, congestion control, mix-mode anonymity, custody re-verification —
all real, all in `SPEC.md`) and what a competent outside reader can find out
without reading the wire format. Triggered by an external architecture
comparison (Meshtastic/Reticulum/SPORE) that raised real questions — Sybil
resistance, replay, metadata leakage, delivery semantics, "who pays the
storage cost" — nearly all of which `SPEC.md` §5/§7/§9/§10 and
`SECURITY_FINDINGS.md` already answer. The reviewer couldn't find those
answers from the public site, which is itself the finding: this milestone is
documentation and UX legibility, not new protocol.

| Task | Status | Notes |
|---|---|---|
| `docs/THREAT_MODEL.md` — six chapters (**observers · participants · identities · resources · network/transport · implementation & evolution**), each threat carrying *adversary capability → attack → asset → mitigation → **residual risk** → explicitly out of scope*, cross-referencing the SPEC §0/§9/§10 mechanism and the `SECURITY_FINDINGS.md` ID that defends it | ⬜ todo | SPEC §0 states the threat model in one paragraph ("every link is hostile"); this expands it without inventing new claims. The governing rule, worth adopting verbatim: **every security mechanism names the attacker it defeats, and every attacker gets an explicit residual-risk statement.** The six-chapter split is broad enough to cover radio-mesh, encrypted-routing and opportunistic-delivery threats without pretending they are the same model |
| **S-032 — delivery-receipt forgery (§8)** | ✅ **shipped, this pass** | Found while auditing the catalogue's "ACK and receipt spoofing" item against the code. Receipts were accepted on payload *shape* alone — unsigned, or signed by any stranger — because the id they reference is public (it rides in every `INV`). `Pending` now records the destination and the receipt must be signed by it. Local state only; no wire/ABI change |
| Audit pass for the catalogue items with no obvious SPEC answer: **wormhole/eclipse against "first copy wins" path learning** (§4), **identity-key revocation while disconnected**, and **crypto agility under a frozen wire** (ver `0x01`, no algorithm negotiation) | ⬜ todo | These are the three where I could not find an existing answer, unlike replication/spam/FS/transport which are all shipped. Each may well end as a documented *residual risk* rather than a code change — a frozen wire is a deliberate trade, not an oversight — but each deserves a written verdict rather than silence |
| Honest-relay retention statement: what an *honest* carrier's storage reveals if seized (seen-set ≥ 30 d, paths 7 d, store-until-expiry, receipts) | ⬜ todo | The catalogue's sharpest store-and-forward-specific point: "honest node" ≠ "collects no metadata". SPORE's numbers are already bounded and documented per-table; what's missing is stating the *aggregate* a seized device yields |
| Locked design guardrail: **no popularity/frequency-weighted replication** (e.g. "send more copies toward frequently-encountered devices") without a documented Sybil analysis first | ⬜ todo | Stamp (§10) is per-envelope proof-of-work, not per-identity — it does not by itself defend a future reputation/social-routing feature against one attacker minting many identities. Nothing today needs this guardrail; it exists so nobody adds social routing later without re-deriving it |
| Public docs/site pass making the already-shipped anti-abuse and privacy mechanisms discoverable: stamp PoW, §5.4 congestion control, custody re-verify-on-read, mix-mode onion+padding+decoys, and the "any underlay with its own routing is just one interface" framing (Meshtastic/Reticulum/Tor/etc. become transports SPORE rides, not rivals) | ⬜ todo | Currently only `SPEC.md` carries this; `README.md`/`site/home.md` don't surface it, which is exactly why an outside technical reviewer concluded these were open questions rather than shipped answers |
| Delivery-status UX language pass (Android/webnode): surface the ACKREQ receipt (§8) and store/expiry state as plain states — e.g. *waiting for contact → travelling → delivered → expired* — rather than raw TTL/hop-count internals | ⬜ todo | Direct extension of the Honesty contract's "no path ⇒ say so" already in `MISSION.md`; check current send/error-feedback UI (M2, #61/#62) before assuming this is greenfield |

**Explicitly not doing here** (already solved, would duplicate shipped work):
replication/copy-count limits (§5.4 congestion control + dedup + store
eviction), spam/"postage" (§10 stamp + quotas), forward secrecy (§7 ratchet +
prekey ring + healing topic-key rotation), transport abstraction (Page 2 —
already the core design). Named "delivery policy" presets (Urgent/Efficient/
Private/Carry/Emergency) were considered and deferred: the underlying knobs
(stamp, hops, FLOOD, mix toggle) already exist, and packaging them as named
UX presets is a candidate for M5 polish, not core protocol — revisit only if
it survives the MISSION.md decision test against "bloats the mental model
past two pages."

**Definition of done:** a reader can go from `README.md` to a real answer for
"what stops someone from flooding my device with junk" and "what can a
carrier who relays my envelope learn" without reading `SPEC.md` cover to
cover; `docs/THREAT_MODEL.md` exists, covers the six chapters, and every claim
in it links to either a SPEC section or a `SECURITY_FINDINGS.md` ID — with a
residual-risk line wherever the honest answer is "partially" or "not at all".

**Why this milestone earns its place** (it is otherwise just documentation):
auditing an external threat catalogue against the code found **S-032**, a
receipt-forgery bug that had been shipping. The catalogue's value was not its
recommendations — most were already implemented — but that it named a class
("authentic ≠ fresh ≠ from the right party") precise enough to test. That is
the argument for doing the remaining audit rows rather than only the prose.

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
| **Compile-time `max_core` gating (C0–C8 cargo features)** | Declined — ratchet session map is inline on `Node`. M8 is the real MCU target that could prove it necessary, but starts on esp-idf-sys specifically to avoid forcing the question; stays declined unless esp-idf-sys proves too heavy for the board. |
| **Identity spanning multiple devices (one person, several keys)** | Not decided, not started. Today identity = one Ed25519 keypair *per device* (SPEC §1/§11 — "ratchet state is per-device, give each device its own key"). A person-level identity with authorised sub-devices is architecturally significant (new envelope semantics, likely wire-affecting) and gated by the same frozen-format process as any 2.0 change. Tracked here so it isn't silently absent; not scheduled. |

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
8. **M8 — Embedded ESP32 runtime** (raw-802.11 relay; first real MCU target) — after the still-open carried-forward items in M2/M4/M5, not ahead of them, unless deliberately reprioritized
9. **M9 — Threat-model legibility & anti-abuse guardrails** (docs/UX only; no protocol gap found) — low urgency, pick up opportunistically

Hardware/community work (the former "Track H" — lived-in prototype, solar cyberdeck,
wear language, community harvest, maintainer culture) is deliberately **not** a
milestone, and is a different thing from M8: Track H is aesthetic/cultural —
objects and vibes, nothing a protocol change depends on — while M8 is a real
runtime implementation the design already committed to. Track H: every row is
`⬜ concept`, nothing in the compass depends on it, and no row earns a 🧪 until
something exists a person could hold or run. It was written up as inspiration in
the now-retired `docs/VISUALDESIGN.md` §6b, not in this plan as a
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
