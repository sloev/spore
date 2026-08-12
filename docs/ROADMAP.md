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
- **[`VISUALDESIGN.md`](VISUALDESIGN.md) is normative** for colour, contrast, motion,
  components, icon system (Antenna + Seed), and the three control sizes. Never pink on
  olive/kevlar (measured 2.32:1). Never signal failure by colour alone.
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
| Conformance: browser↔native over QUIC/WebTransport (reuses iroh path) | ⬜ open | Was "native WebRTC ice-lite bridge". SPEC page 2 reads as though it ships; doc now says otherwise. Native ICE/DTLS/SCTP declined (largest dep this repo would take — see BRIDGES.md §WebRTC). The native half is reachable via the iroh QUIC path already merged (`src/direct/iroh.rs`); a native WebTransport/QUIC adapter would close the gap without a native WebRTC stack |

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
rules, and the screen structures in [`VISUALDESIGN.md`](VISUALDESIGN.md) §7.

This is a **first-class milestone**, not scattered "nice-to-have" items. The
tokens already exist and are generated (C3/C5-token half shipped #118/#119); the
work below is the code half that changes what is on screen.

| Task | Status | Notes |
|---|---|---|
| Design tokens single-sourced + generated into all surfaces (C3) | ✅ shipped | `design/tokens.json` → `generate.py`; CI drift job |
| Control metrics generated (CONTROL 48 / CHIP 32 / ROW 56) (C5 token half) | ✅ shipped (#118) | Heights/paddings/radii/spacing guarded by drift job |
| Usage matrix — what each control is *for* (C5 matrix) | ✅ shipped (#119) | Generator enforces the count and the touch-floor rule |
| Android `Chip` + `ListRow` primitives; route ad-hoc sizes through them (C5 Kotlin half) | ⬜ todo | The part that changes what is on screen; needs a device |
| Density & type hierarchy pass (C4) — ≤1 instructional sentence, progressive disclosure | ⬜ todo | Depends C5 Kotlin half |
| Bridges & Advanced information architecture (C6) — uniform rows, grouped sections | ⬜ todo | Depends C5, C4 |
| Empty-state & status-line diet (B11) | ⬜ todo | The sweep-up after C4 |
| Replace mushroom icon with Antenna + Seed on Android | ✅ done | `ic_spore.xml` now Antenna + Seed |
| Replace mushroom icon with Antenna + Seed on web node | ⬜ todo | |
| Replace mushroom icon with Antenna + Seed on site (favicon, hero, nav) | ⬜ todo | |
| Persistent identity + status header on web node (WV0) | ⬜ todo | Tokens, identity header, Baud empty states |
| Web node IA — distinct surfaces Mail / Feed / Bridges / Seed (WV1) | ⬜ todo | Renamed from brief's W1 (collides with shipped Encrypted DM ABI) |
| Site design-language execution (Site-2) — usage matrix + density everywhere | ⬜ todo | |
| Site navigation chrome + human/builder paths (Site-3) | ⬜ todo | Today 9 nav items vs matrix's 5; Mission unreachable from nav |

**Acceptance (across the milestone):**

- [ ] Only three interactive control heights exist, in the tokens and in the code.
- [ ] Bridges uses uniform list rows; no heterogeneous full-width button stack.
- [ ] Advanced is sectioned rows, not a tall card stack.
- [ ] No working screen opens with more than one short instructional sentence.
- [ ] Status chrome is compact — `0 peers · 65 stored`.
- [ ] The site has persistent, clear navigation.
- [ ] The web node has a persistent identity + status header.
- [ ] Pink never on kevlar; the contrast table is still green.
- [ ] Reduced motion is fully static; the standalone still makes zero external requests.
- [ ] Baud appears only on empty states and completions.
- [ ] The only brand icon is Antenna + Seed; no mushroom anywhere.

**Definition of done:** a screenshot of every surface passes the VISUALDESIGN §8
checklist, and the mushroom icon is gone from the repo's rendered assets.

---

## Milestone 4 — Webnode as daily driver

**Goal:** the browser is a full daily-driver peer (Mail / Feed / Files / Bridges /
Seed), not a transport demo. The first runtime to consume M1's storage seam and
the communicator-as-façade pattern.

> **Guardrail:** this stays a *reference* client, not "the" SPORE app. No feature
> here requires the standalone HTML specifically; a bridge, a Python script, or
> the daemon CLI must remain equally capable.

| Task | Status | Notes |
|---|---|---|
| Encrypted DM — wasm exports (announce, send_direct, send_direct_sealed, open_dm, env_flags, env_src) | ✅ shipped (#116) | ABI half; sealed on the wire, sender authenticated |
| Encrypted DM — thread list, compose, delivery honesty (no read receipts) | ⬜ todo | UI half; key on `env_src` (authenticated sender), not a claimed field |
| Topics: open group join/create + public shout, clearly labeled public (W2) | ⬜ todo | |
| Sealed group: shared-key/invite-blob room, "anyone with the key can post" banner (W3) | ⬜ todo | No roster — honest UX |
| Feed/microblog: compose to `feed::<addr>`, follow = subscribe (W4) | ⬜ todo | |
| Files: publish → magnet, fetch by magnet, progress UI, local search (W5) | ⬜ todo | |
| Public folder + `spore://` resolver (W6) | ⬜ todo | Sandbox foreign HTML (XSS) |
| Authorized feed polish: invite flow, documented revoke-by-rotation limit (W7) | ⬜ todo | |
| Continuity polish: export seed from new UI, docs updates (W8) | ⬜ todo | |

**UI across runtimes (locked decision):** two UI implementations over three shared
layers — browser/desktop share the web UI (in-process wasm vs localhost HTTP to
daemon); Android stays Compose. Desktop is `daemon + web UI`, optionally wrapped in
Wry (not Tauri). Do not wrap the standalone in a window and call it desktop.

**Definition of done:** two browsers on a LAN can DM, open room, sealed room (with
key), shout, feed-post, publish + fetch a file — all through the standalone HTML,
no daemon involved — and still be a full node (store, bridges, same envelopes).

---

## Milestone 5 — Polish & hardening

**Goal:** the sweep-up after the above. **Only start this milestone once M1–M4 are
done.** Nothing here is load-bearing for a credible node.

| Task | Status | Notes |
|---|---|---|
| Ring health UI + Export with FS warning | 🟡 ring health shipped (#67); export polish open | |
| Group `key_id` divergence badge | ⬜ todo | Warn on mismatch; never claim roster consensus |
| Boot receiver (optional, default off) | ⬜ todo | |
| Sound + particles behind a setting, default off | ⬜ todo | Gated by §0.2/§8 |
| Android bridge list ⊆ BRIDGES.md sync check | ⬜ todo | Honesty check |
| `with_node` reentrancy guard | ⬜ todo | Low; documented, not prevented |
| Beacon duty-cycle measurement | ⬜ todo | HARDWARE.md procedure |
| Two-real-NATs Direct punch verification | ⬜ todo | `HARDWARE.md` row 19; loopback-only today |
| Hardware matrix pass (backup exclusion + migration + 7-day FS) | ⬜ todo | Needs a device; `android/TESTING.md` checklist exists |

---

## Explicitly out of scope / non-goals (locked)

| Item | Decision |
|---|---|
| **iOS** | Not a target, ever. State this on `APPS.md` so the expectation dies early. |
| **Instant delivery with no path** | Impossible under store-and-forward. UI says "no path yet" / fails closed for live media; async fallback (voice note) is fine. |
| **Group membership consensus protocol** | A shared-key sealed topic is shippable now; a Signal-style roster is a deliberate future protocol project, not a UI feature to fake. |
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

Hardware/community work (the former "Track H" — lived-in prototype, solar cyberdeck,
wear language, community harvest, maintainer culture) is deliberately **not** a
milestone: every row is `⬜ concept`, nothing in the compass depends on it, and no
row earns a 🧪 until something exists a person could hold or run. It lives in
[`VISUALDESIGN.md`](VISUALDESIGN.md) §6b as inspiration, not in this plan as a promise.

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
