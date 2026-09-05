# SPORE roadmap — milestones

**Project:** `sloev/spore` · **Version:** 0.7.0 (`Cargo.toml`).

This is the single forward-looking plan, organised as **milestones** rather than
a flat PR map. Each milestone is a coherent body of work with a clear definition
of done; PRs are the merge units inside a milestone, not the plan itself.

**Only what is ahead lives here.** A task is deleted from this file when it
ships, and a milestone is deleted when its last task does — which is why the
numbering has gaps. There is no changelog and no shipped column to maintain,
because both are records of the past and both can drift from it:

| Question | Where it is answered |
|---|---|
| What is planned? | this file |
| What shipped, when, and why? | `git log` — the commit message carries the reasoning |
| What stops it breaking again? | the test that came with the fix |

A regression is caught by a test, not by a line in a document. If a fix ships
without one, that is the thing to fix — not an entry to write.

**Read order for agents:** [Mission](MISSION.md) → this file →
[Spec](SPEC.md) as needed, plus `git log` for what shipped. See
[Dev guide](DEV_GUIDE.md) for the full repo map.

---

## Hard rules (do not violate)

- **Frozen:** the wire format — `reference/vectors.json`, its generator
  `examples/gen_vectors.rs`, and the API pin `tests/api_freeze.rs`. No change
  without the `allow-frozen-change` label. The C ABI (`bindings/spore.h`) is
  **freeze-on-remove**: symbols may be added freely, none may be removed or
  renamed. Nothing else is frozen — adding a test is not a breaking change. See
  [Contributing](CONTRIBUTING.md#the-frozen-v1-contract).
- **Honesty over polish:** 🧪 markers, "Still open", served-vs-fetching language, and
  **no fake UI** — never a control whose backend is missing.
- **HARDBRUT upstream (`supernihil/hardbrut`) is normative** for colour, contrast,
  motion and components — SPORE no longer maintains its own design-language document.
  The flat `web/vendor/hardbrut/hardbrut.css` is vendored at build time and trusted
  as-is for the Pages site and Android; the web app (the standalone) instead
  consumes **HARDBRUT/3** at `web/vendor/hardbrut3/` (M10-D) — both are upstream,
  neither is a SPORE-authored fork. Never signal failure by colour alone.
- **Zero external network requests** in `web/spore-standalone.html` (CI greps for it).
- Motion fully static under reduced motion / `ANIMATOR_DURATION_SCALE == 0`. Sound and
  particle bursts stay **off** until the user enables them.
- One concern per PR (CI-green alone). Security and design-language work are orthogonal
  when independent.
- Distinguish **Verified** (code/tests) vs **Reasoned** vs **Needs device run**. Do
  not claim hardware verification that was not run, or invent protocol features.
- **No icon, no mascot.** SPORE's brand is the wordmark — plain "SPORE" text — on
  every surface. Nothing stands in for it, not even a monogram; Android's
  `ic_spore.xml` is a plain HARDBRUT accent swatch because the platform
  requires an icon file to exist, not because it represents anything.

---

## Milestone 1 — Security & correctness

**Goal:** close the last forward-secrecy and store-bounds gaps; make every spec
claim the code actually honours.

| Task | Status | Notes |
|---|---|---|
| Field-verify the offline window end-to-end on a device | ⬜ deferred to hardware QA | Unit tests prove deadline/clamping; needs a real clock/delivery run (M4) |
| Backup exclusion + migration tested on hardware | ⬜ deferred to hardware QA | No device in CI; tracked in `android/TESTING.md` |
| Benchmark suite: throughput/memory, reproducible, tracked per platform | ⬜ todo | Folded into **M11's simulator** rather than built separately: a throughput number from a microbenchmark says less than delivery probability and bytes-per-delivered-byte under loss, and two harnesses would drift. Keep the per-platform regression-threshold idea — that is the part `spore-sim` owes CI |
| `unsafe`-code snapshot tracked and diffed in CI | ⬜ todo | No inventory of `unsafe` blocks exists today. Prns keeps `audits/unsafe-snapshot.json`, diffed so new unsafe code is a visible, reviewable event rather than something that can land unnoticed |

**Definition of done:** every SPEC claim about forward secrecy and store bounds is
backed by a test; the `## Unreleased` SECURITY_FINDINGS Still-open list has no P0
items that are not either fixed or honestly marked deferred-to-hardware.

---

## Milestone 2 — Core functionality & reliability

**Goal:** the phone node and daemon are credible daily drivers on the transports
that are verified, with honest limits on the ones that aren't.

| Task | Status | Notes |
|---|---|---|
| Chat attachments: ExoPlayer preview, edit-after-send, public-file single bubble | 🟡 partial | The staging → bubble → FileProvider path exists; these are what remain — the same list as the carried-forward gaps below |
| Conformance: browser↔native over QUIC/WebTransport | ⬜ open — **spike validated** | Spike `spikes/001-webtransport-native` confirms feasible: a feature-gated `wtransport`+`quinn` native server plus the browser shim, both mapping onto `DatagramPort` like `IrohPort`. **Constraint:** iroh's QUIC is not HTTP/3 WebTransport, so the native side is a *new* listener, not a reuse of iroh's endpoint — only `DatagramPort` and Direct signalling are reused. `quinn` is a second QUIC stack; feature-gate it like `bridge-iroh` |

**Carried-forward functional gaps (still real, not regressions):**

- [ ] ExoPlayer audio/video inline preview + playback
- [ ] Edit / remove an attachment after send
- [ ] Merge the bubble for public/unsealed files (needs `nativeEnvId` at `route()` time)
- [ ] Edit a bridge in place (today Remove + re-add; needs a native mutate helper)

**Definition of done:** a minimum-credible phone node — M1 + attachments usable
end-to-end + bridges stoppable/removable + one device-matrix pass (backup
exclusion + migration). Direct connects on a LAN and degrades honestly on a WAN.

---

## Milestone 4 — Webnode as daily driver

**Goal:** the browser is a full daily-driver peer (Chats / Feed / Files /
Bridges / Seed), not a transport demo. The first runtime to consume M1's storage
seam and the communicator-as-façade pattern.

**Surfaces (locked IA):** **Chats** (unified: 1:1 DMs + open groups + private
groups) · **Feed** (personal microblog + subscribed feeds) · **Files** ·
**Bridges** · **Seed**. The old Mail / Topics / Sealed-Topics panels are merged
into Chats; the old shared `spore/feed` topic is replaced by per-address feeds.

> The IA is intact but the **names moved** when M10-D rebuilt the web node on
> HARDBRUT/3: Feed shipped as **Blogs**, and Bridges and Seed are both inside
> **Settings**, alongside **Contacts** which this list never named. The surfaces
> below map to what exists; the labels do not. Reconciling the two is M10-B's
> job, since that is where the six domain stores get named once for every shim.

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
| W9–W11 Android parity — Chats list adds private groups; Feed adds per-address subscribe; formatting in chats | ⬜ todo | Android Chats already mixes DMs + PUBLIC; add private-group rows + per-address feed |
| Public folder + `spore://` resolver (W6) | ⬜ todo | Sandbox foreign HTML (XSS) |
| Continuity polish: export seed from new UI, docs updates (W8) | ⬜ todo | |
| Zero-config bridge discovery: web node auto-probes a well-known local port/hostname + fetches a small catalog, instead of the user typing a bridge address | ⬜ todo | [Prns](https://github.com/KenAKAFrosty/Prns)'s browser SDK does this ("Auto Wi-Fi"): probes `ws://localhost:<port>` and `ws://<name>.local:<port>` plus a `GET /.well-known/…` catalog, with per-candidate exponential backoff and a seeded deterministic pick so concurrent tabs don't pile onto one gateway. SPORE's `bag` API already listens on a fixed port (7373) to localhost and the LAN — this is a client-side convention over what exists, not a new server surface |

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

**Goal:** the sweep-up after the above. **The milestone as a whole waits on M1–M4**,
though small opportunistic fixes may land early when they're cheap and isolated.
Nothing here is load-bearing for a credible node.

| Task | Status | Notes |
|---|---|---|
| Export polish, with the filesystem warning | 🟡 partial | Ring health exists; the export side is what is left |
| Private group `key_id` divergence badge | ⬜ todo | Warn on mismatch in a sealed group chat; never claim roster consensus |
| Boot receiver (optional, default off) | ⬜ todo | |
| Sound + particles behind a setting, default off | ⬜ todo | Gated by the hard rule above: motion static under reduced motion, sound and particle bursts off until the user enables them |
| SNR-weighted contention window on shared media | ⬜ todo | Today `Csma::schedule` waits a **random** 1–5× airtime before a flood and cancels if the id is overheard (§5.5); there is no signal-quality input anywhere in `src/`. [Meshtastic](https://meshtastic.org/docs/overview/mesh-algo/) instead gives the node that heard a frame *weakest* the **shortest** delay, so the most distant node rebroadcasts first and each hop covers the most ground, with nearer nodes then cancelling. That is a strict improvement to a mechanism SPORE already has: `schedule()` already takes the delay as a parameter, so only the delay computation and one signal-quality argument change — no wire change, pure local policy. The input is already in hand where it matters first: `esp32/src/radio.rs` reads `pkt.rx_ctrl` for `sig_len()`, and RSSI is a sibling field in that same struct (M8/E2) |
| A node that carries its own traffic but relays nothing (a "companion" profile) | ⬜ todo | `set_bulk_budget`/`register_limited` cap *other people's file chunks* only; messages, announces, receipts and manifests always pass, so there is no way to say "do not relay for others" — the case for a phone on battery, or a node on a metered link. [MeshCore](https://github.com/meshcore-dev/MeshCore) ships this as a fixed `Companion` role that never repeats, which it argues also keeps bad paths out of the routing table. For SPORE it must be **local policy, not a protocol role** — a relay budget of zero, chosen by the operator and invisible on the wire — because M8's locked rule is that gateway behaviour is emergent and "if a gateway role ever needs writing, something has gone wrong." Needs an honest UI: a node that relays nothing is a worse citizen, and should say so rather than look identical to one that does |
| Beacon duty-cycle measurement | ⬜ todo | HARDWARE.md procedure |
| Two-real-NATs Direct punch verification | ⬜ todo | `HARDWARE.md` row 19; loopback-only today |
| Hardware matrix pass (backup exclusion + migration + 7-day FS) | ⬜ todo | Needs a device; `android/TESTING.md` checklist exists |

---

## Milestone 8 — Embedded ESP32 runtime (raw-802.11 relay)

**Goal:** the first real implementation of the `Embedded (ESP32)` runtime
`docs/SPEC.md` already names ("little memory, no filesystem, one or two
bridges") — a standalone, headless ESP32-S3 that relays real envelopes over raw
802.11, persists its store to flash, and bridges to a phone or laptop over USB
or BLE when one is nearby. Filed as
[#149](https://github.com/sloev/spore/issues/149).

**Flash it and it works; a tether is just another bridge (locked).** No
standalone build and no tethered build, no mode switch, no pairing, no
configuration step: a flashed board with power is already a working node.
Binding on every task below:

- **Untethered and unconfigured is the normal state.** The radio comes up on a
  fixed default, so two boards out of the same box find each other with no
  intervention. A board dropped in a field with a battery is the design target,
  not a degraded case.
- **A connection to another device is a *bridge*, not a mode.** USB serial and
  BLE register on the hub exactly like the radio, and the core floods between
  all of them as it already does for every other bridge. Nothing in the firmware
  knows the word "tether" — plugging a cable in adds an interface to a node that
  was already running.
- **Gateway behaviour is therefore emergent, not a feature.** A phone on USB
  gets the mesh, and the mesh gets the phone's internet, Bluetooth and folder
  bridges, because each end floods between its own interfaces. **If a gateway
  role ever needs writing, something has gone wrong.**
- **Neither role is a build-time choice** — a cargo feature would mean two
  firmwares to test and "which one is on this board?" as a question anyone has
  to ask. And **configuration cannot require a tether**: defaults must relay out
  of the box and anything persistent lives in flash, or the tether is
  load-bearing by the back door.

Hence the ordering — E2/E3 (radio, store) precede E4/E5 (USB, BLE), because a
bridge attaches *to* a working node.

**The daemon speaks the same air interface (locked).** A Linux daemon with a
monitor-mode card runs the same frame format (E2d), so a laptop is a peer of the
boards rather than something they tether to, which makes a mixed deployment one
mesh instead of two. The layout is specified in [Bridges](BRIDGES.md) before
either side is finished: board-to-board interop happens by accident when both
ends run our code, board-to-laptop only if the framing is written down.

**Less new architecture than it looks.** Raw 802.11 is another "message pipe",
a shape the bridge taxonomy already lists. The storage nutrient (`SpillBackend`,
#87) shipped specifically to unblock ESP spill, so E3 implements an existing
contract rather than a new one, and USB reuses `bridge::kiss_stream`. The
genuinely new work is the radio driver (promiscuous RX filter +
`esp_wifi_80211_tx`) and the BLE fallback.

**Toolchain (locked): esp-idf-sys, not bare-metal `esp-hal`.** ESP-IDF's
std-like environment (newlib) means the core compiles close to as-is. A
bare-metal `no_std` port would reopen "compile-time `max_core` gating", declined
below with the exception "revisit only if a real MCU target proves it
necessary" — this milestone *is* that target, and starting from esp-idf-sys
avoids forcing the question on day one. `esp-hal` is an explicit non-goal here;
a later milestone can attempt it if esp-idf-sys proves too heavy.

**Regulatory posture (locked): documented, not enforced.** Frame injection
outside normal association, and encrypted traffic on bands whose rules restrict
it (amateur radio's no-encryption rule most notably), are the operator's
compliance problem. The `BRIDGES.md` entry states plainly what the bridge does
and names the considerations that follow — the same posture as disclosing a mix
mode's limits — but SPORE does not gate, strip or weaken encryption on anyone's
behalf. Silent non-compliance would be dishonest; refusing to build the feature
over a rule the operator may not even be subject to is not this project's call.

**Tasks** (each a PR), tagged **E1–E8** and ordered on three principles.
**Biggest risk first:** E1 answers *does the core compile, link and actually run
on this board, in this much RAM, under esp-idf-sys?* — nothing else is worth
building until it does. **Nutrient work stays distinct from bridge work:** E1
and E3 supply the four nutrients this runtime owes the *existing* core contract
(randomness, time, scheduling, storage — [Spec](SPEC.md)), while E2, E4 and
E5 put the board on the *open* bridge list; conflating the two is how a runtime
ends up inventing protocol. **Solo before paired:** each phase names what one
board alone can prove, so only the rows that say so need a second device.

**None of the three bridges is new.** Raw 802.11, BLE GATT and
USB/serial-over-KISS are all already specified in [Bridges](BRIDGES.md) — BLE
down to the Nordic UART UUIDs. Two already have a working *browser* half
(`web/transports/webbluetooth.mjs`, `webserial.mjs`), so E4 and E5 make the
board the peer for clients that already speak the framing. No row below is a
design task; what is missing is the firmware side of each.

| Task | Status | Notes |
|---|---|---|
| `esp_wifi_80211_tx` injection wired as `DatagramTransport::send`, RX as `::recv`, driven through `run_datagram` (E2) | 🧪 written, not run | `esp32/src/radio.rs` (ESP-IDF glue) over `bridge::ieee80211` (the codec, portable and CI-tested, shared with E2d). Promiscuous RX filtered to management frames in hardware, then `ieee80211::parse` → bounded queue → `recv`. Builds clean; **nothing has been transmitted or received on air** |
| Solo TX-shape test: an external monitor-mode sniffer confirms the injected frame's shape (E2) | ⬜ todo | Proves the injection path without needing a second SPORE node |
| Device-pair relay: two boards exchange a real envelope over the air (E2) | ⬜ todo | 🧪 until this run happens |
| Linux daemon raw-802.11 bridge: monitor mode + injection over `nl80211`, same frame format as the board (E2d) | ⬜ todo | The other end of the same air interface, so a laptop relays with the boards rather than only talking to them over a tether. Shares the frame layout and `Envelope::probe` filter with the ESP path — one wire format, two implementations. Needs a card whose driver supports monitor + injection (`iw list` → "monitor" and "AP/VLAN"), which is a hardware constraint, not a code one |
| Flash partition setup (custom `partitions.csv`, mount/format-on-first-boot) (E3) | 🧪 written, not run | **SPIFFS, not `littlefs`** — see the note below. `esp32/src/storage.rs`. The table is applied at *flash* time, not build time: esp-idf-sys's generated CMake project lives under `target/` and cannot see a CSV in the crate root, so `CONFIG_PARTITION_TABLE_*` silently does nothing. 1500K app / 2400K store |
| Adopt the last run's spill at boot (E3) | 🧪 written, not run | `Node::set_spill_dir` builds the backend and adopts in one call, re-verifying every id against its bytes — a file SPIFFS damaged is discarded rather than trusted, because an id *is* the hash of its content |
| Power-cycle test: spill past the memory budget, cut power, confirm the adopted set matches (E3) | ⬜ todo | 🧪 until logged in [Hardware verification](HARDWARE.md). A genuinely new row — real flash and real power loss, not merely "not CI-testable" |
| Wired tether: KISS over a byte stream, registered as an ordinary bridge (E4) | 🧪 written, not run | `esp32/src/tether.rs`. No new framing and no new transport — `bridge::kiss_stream` and `stream_link::run_split`, the same loop the desktop serial bridge runs. **UART on the S2, USB-CDC on the S3**, per board: the S2 has one USB peripheral and the console already owns it, so KISS frames would interleave with log text. The S3 has USB-Serial-JTAG *as well*, which is what makes a USB tether possible there |
| Solo loopback: laptop-side KISS echo, board sends and receives its own frames (E4) | ⬜ todo | Proves the framing with no phone in the loop. On the S2 this needs a USB-serial adapter on `tx=17 rx=18` |
| Phone tether: the Android app or web node over USB, real message exchange (E4) | ⬜ todo | 🧪 until run. [Hardware verification](HARDWARE.md) row 4 ("Web Serial → board") is the existing pattern |
| Expose the existing NUS profile from the board: KISS over Nordic UART, `stream` form (E5) | ⬜ todo | No design step needed — [Bridges](BRIDGES.md) already specifies this bridge down to the UUIDs (`6e400001-…`, RX `…0002`, TX `…0003`), KISS framing via `bridge::kiss_stream`, and the ~247-byte ATT MTU. The browser half already speaks it (`web/transports/webbluetooth.mjs`), so the board is the peripheral for a client that exists |
| Solo test: a generic BLE central exercises write/notify against the board (E5) | ⬜ todo | Confirms the characteristic layout without a phone SPORE client |
| Phone tether: Android or Web Bluetooth exchanges a real low-bandwidth message (E5) | ⬜ todo | 🧪 until run. [Hardware verification](HARDWARE.md) row 18 (BLE NUS) is the existing pattern |
| Feasibility check: can the board run its raw-802.11 monitor/inject radio and a Wi-Fi soft-AP at the same time? (E8) | ⬜ todo | Blocking question before designing the task below. [Prns](https://github.com/KenAKAFrosty/Prns)'s ESP32 firmware ("Hopspot") pairs a soft-AP with its own zero-config discovery convention so a phone browser tethers over Wi-Fi with no cable — a third path alongside USB (E4) and BLE (E5). ESP32 Wi-Fi radios are single-radio, time-multiplexed between modes; whether AP mode can share airtime with continuous monitor-mode capture on this chip (vs. only sequential STA/AP coexistence, which several ESP-NOW libraries rely on) is a real hardware constraint, not a design choice — answer before scoping a task |
| [Bridges](BRIDGES.md): move each of the three from ⚪ planned to 🧪 as its board-side half lands (E6) | ⬜ todo | The entries themselves already exist and are accurate — 802.11 gained the ESP path, the `probe` filter's role and the regulatory note in #172. What is left is a status change per bridge, landing with that bridge's own PR, not new prose |
| Full combined run: two boards, flash store live, an envelope relayed over raw 802.11, one board bridged to a phone over USB, BLE exercised as the fallback (E6) | ⬜ todo | One [Hardware verification](HARDWARE.md) row per path, matching the existing one-row-per-path convention |
| Flash-cycle re-confirmation in the combined rig: power-cycle one board mid-session, confirm it resumes relaying with its spilled store intact (E6) | ⬜ todo | Distinct from E3's isolated test — proves persistence holds while the radio and USB paths are also live |
| Firmware update path for a fielded board — no factory re-flash required (E7) | ⬜ todo | Not scoped by #149; no signing/staged-rollout/rollback story exists yet. [Prns](https://github.com/KenAKAFrosty/Prns)'s ESP32 equivalent ("Hopspot") ships a full candidate → sign → promote → rollback CI pipeline for exactly this — worth studying before designing SPORE's own, given M8 boards are meant to run unattended in the field |

**On [esp32-open-mac](https://github.com/esp32-open-mac/esp32-open-mac)
(considered, not usable yet).** A reverse-engineered, blob-free 802.11 MAC in
Rust — philosophically a much better fit, since [Continuity](CONTINUITY.md) and
[Rebuild](REBUILD.md) are both about not depending on what you cannot inspect,
and the Wi-Fi blob is the one component of an ESP32 node nobody can audit.
**But it only runs on the original ESP32**, not the S2 or S3, because the Wi-Fi
peripheral addresses are hardcoded. Worth revisiting if it ports, or if a plain
ESP32 becomes a supported board; until then E2 uses `esp_wifi_80211_tx` and
accepts the blob.

**On SPIFFS vs `littlefs` (E3).** `littlefs` is the better fit — built around
power-loss atomicity, exactly the property E3 exists to test — and was tried
first, but `esp-idf-svc` 0.51 fails to compile: its `io` module references
`crate::fs::littlefs` unconditionally while the module itself stays gated.
Accepting a broken build for a property that cannot be verified without a
power-cycle rig was the wrong trade, so E3 ships on SPIFFS. Revisit when that is
fixed upstream. The cost is smaller than it sounds: spilled envelopes are
content-addressed and re-verified on read, so a file SPIFFS corrupts reads as
"not held" and the mesh re-fetches it. The real exposure is a failed *mount*,
which loses everything at once — and that is what `littlefs` would buy.

**Footprint checkpoint (E1/E2) — measured, and the toolchain decision stands.**

| Build | Flash (app image) | Internal SRAM (static) |
|---|---|---|
| Scaffold + one signed envelope (E1) | ~542,500 B · ~13% of a 4 MB part | 67,235 B · ~13% of 512 KB |
| Wi-Fi stack linked in (E2) | 977,279 B · 23% of 4 MB | 108,148 B · 33% of the S2's 320 KB |

Linking Wi-Fi in roughly doubled both — exactly the step-change the per-run CI
report exists to make visible rather than discover later. Still comfortable, and
measured before a BLE stack piles on top, so compile-time `max_core` gating
stays declined: these numbers do not trigger its "only if a real MCU target
proves it necessary" exception. The CI job prints both figures on every run, so
headroom is tracked as E3–E5 land rather than measured once and assumed.

Two caveats. These are static sections, not live heap — for heap see the E1
smoke test's measured 226,368 bytes free. And the flash total is approximate on
purpose: it moves by ~100 bytes between build environments, because panic
messages bake in absolute source paths. Compare runs, not last digits.

**Definition of done:** an ESP32-S3 running this firmware relays real SPORE
envelopes over raw 802.11 to at least one other node, bridges to a phone or
laptop over USB (KISS) with BLE as the low-bandwidth fallback, and its store
survives a power cycle via flash — with `HARDWARE.md` recording a real
device-pair run, not just green CI. 🧪 until then.

---

## Milestone 10 — One application layer, three UI shims

**Goal:** stop reimplementing the communicator (conversations, contacts, unread,
delivery state) once per platform. Promote it into the Rust core so CLI, Android
and web differ only in their UI shim, and rebuild the web node on HARDBRUT/3 as
the first consumer of that contract.

**The finding that motivates this** (verified against the tree): the kernel owns
**no application state at all** — no contact book, no conversation store, no
message history anywhere in `src/`; `petname` exists only as an ANNOUNCE field and
a CLI config key. Above it sit **three host layers that have already diverged**:

| Host layer | Lines | Exports | Holds state? |
|---|---|---|---|
| `src/wasm.rs` (web) | 645 | 34 | no |
| `src/ffi.rs` (C ABI, frozen) | 491 | 20 | no |
| `android/jni/src/lib.rs` | 1637 | 64 | **yes** — `Runtime{inbox, ifaces, demod, direct, stream_stop}` |

Each exports what the others don't (`send_direct`/`acked` in wasm only,
`armor_wrap`/`keypair` in the C ABI only), and the communicator is then written a
*third* and *fourth* time on top — ~6130 lines of Kotlin and ~1300 of JS embedded
in `build-standalone.mjs`'s template literal. `android/jni/src/lib.rs` is the proof
this is **consolidation, not invention**: 1637 lines of Rust already holding a
stateful runtime over the kernel, simply Android-only and in the wrong directory.

**Feasibility (verified).** The crate is already shaped for it: 64
`cfg(not(target_arch = "wasm32"))` gates, wasm-bindgen-free plain WebAssembly, and
`getrandom`'s `custom` backend already routed to a JS import — the exact pattern a
wasm storage port needs, with `set_spill_backend` as the in-tree precedent for a
pluggable one.

**Portability is a hard requirement and is already met — keep it that way.** Every
target below is built on **every PR**, from the same crate.

| Target | CI job | How |
|---|---|---|
| Linux / macOS / Windows | `ci.yml` | `os: [ubuntu-latest, macos-latest, windows-latest]` |
| Browser (wasm) | `ci.yml`, `android.yml` | `--target wasm32-unknown-unknown` |
| Android | `android.yml` | `cargo-ndk`, `aarch64-linux-android`, `armv7-linux-androideabi`, `x86_64-linux-android` |
| ESP32-S3 | `esp32.yml` | `xtensa-esp32s3-espidf` in Espressif's container — "prove the unmodified core still compiles" |

The crate is already split into a **portable core** and a **native-only layer**,
and that split is what makes this work. `bridge/{hub, driver, serial, ax25,
copyparty, foldersync, i2p, spool, store, stream_link}` and `direct/` are
`#[cfg(not(target_arch = "wasm32"))]`; everything below them is portable.

**The communicator belongs in the portable half**, which means it obeys four
rules. The first three are enforced automatically — the wasm and ESP32 jobs stop
compiling if they are broken — and the fourth is not, which is why it is written
down:

1. **No filesystem.** Reach storage only through the M10-A port. Web is the only
   target with no filesystem at all; ESP32 has one (`esp32/src/storage.rs` mounts
   SPIFFS through ESP-IDF's VFS, so `FsSpill` runs there unmodified), as do
   desktop, CLI and Android. **The port therefore needs exactly one new
   implementation, not one per platform.**
2. **No threads.** `wasm32-unknown-unknown` has none. The layer is driven by the
   host — "here is a tick, do your work" — exactly as the core already is.
3. **No new dependencies** unless they build on Espressif's Xtensa Rust fork.
   The repo already has scar tissue here: the optional `iroh` feature moved the
   MSRV floor from 1.75 to 1.85 because Cargo resolves optional dependencies too.
4. **No reading the clock.** `SystemTime::now()` and `Instant::now()` *compile*
   on `wasm32-unknown-unknown` and then panic at runtime, so no compile guard
   catches this one. The core's existing convention is the answer: time arrives
   as a `now: u32` parameter (`Mix::add`, `Pending::expire`, every `spore_node_*`
   entry point). The communicator does the same. Clock reads stay in the
   native-only layer, where they already live.

**ESP32 gets the kernel, not the communicator.** It is a headless relay (M8) with
320–512 KB of SRAM, no screen and no conversations; a contact book on it would be
dead weight. The communicator is a Cargo feature, default on, off for ESP32 —
excluded by design, not by limitation. Portability of the *kernel* is the win;
the app layer goes where there is a user.

**Desktop is a webview *and* a daemon — not the web node in a window.** If the
desktop build is Tauri (or equivalent), it reuses the web UI, but it is not
limited to what a browser can do: it also has every native bridge. So the host,
not the UI, decides which transports exist. `SporeClient` therefore takes a
transport registry rather than importing one — a browser gets
`BROWSER_TRANSPORTS`, a desktop host passes that plus the eleven native bridges
proxied to Rust.

That leaves one open decision, and it is the more consequential half:

| | Where the node runs | Consequence |
|---|---|---|
| **A** | wasm in the webview; native bridges proxied in over IPC | Two envelope paths to keep in step; the node still has no threads, no sockets, no filesystem |
| **B** | native Rust node; the webview is pure UI over IPC | The daemon *is* the app. Real sockets, threads and filesystem, all eleven bridges natively, one node |

**B is the better shape**, and it is the strongest argument yet for M10-B/C: if
the communicator is in Rust behind one app-level ABI, then desktop is that ABI
over IPC and the browser is the same ABI over wasm. The screens cannot tell the
difference, because they only ever talk to `SporeClient`. That is precisely what
contract-first was meant to buy, and it means **desktop should follow M10-C
rather than precede it** — building it before then would mean writing the IPC
layer against an interface that is about to be replaced.

**Sequencing (locked): contract first, stores after.** The `SporeClient`
command+event interface is defined *first* and is the UI's only contract. The web
UI is built against it immediately, backed initially by thin JS stores; those
stores then migrate into Rust one at a time **behind an unchanged interface**. The
design ships without waiting on M10-A/B, and the Rust consolidation is not
foreclosed. The deliberate cost is ~300 lines of throwaway JS store code.

**Kernel gaps blocking the HARDBRUT/3 design:**

| # | Gap | Blocks |
|---|---|---|
| G1 | No conversation / message history store | Chat, Blogs |
| G2 | No contact/petname book (petname is an ANNOUNCE field only) | Contacts |
| G3 | No unread/read state | chat-list badges, `.chat-unread` divider |
| G4 | No drafts | composer `data-draft="true"` |
| G5 | **No wasm storage port** — `FsSpill` is `cfg(not(wasm32))`, `SpillBackend` has no wasm impl | G1–G4; nothing can persist |
| G6 | Three divergent ABIs | one app-level ABI needed |
| G7 | Files are flat (`publish`/`fetch`/`list`); design wants per-contact folders + suggestions | Files |

G5 is the true blocker — it gates G1–G4, and it is the smallest piece.

**Tasks** (each a PR):

| Task | Status | Notes |
|---|---|---|
| M10-B `communicator` module in `src/`: Identity, Thread, Topic, Bridge, Transfer, Contact stores | ⬜ not started | The six stores from the Phase-2 architecture definition, in Rust |
| M10-C Collapse `wasm.rs` / `ffi.rs` / `android-jni` onto one app-level command+event ABI | ⬜ not started | **`bindings/spore.h` is frozen** — needs `allow-frozen-change`, or an additive app-level ABI beside it |
| M10-E Re-point Android Kotlin + CLI at the shared layer; delete duplicated logic | ⬜ not started | Retires ~6130 lines of Kotlin app logic and the JS blob |
| M10-F Desktop (Tauri or equivalent): the web UI over a **native** node, with the eleven native bridges | ⬜ not started | Must follow M10-C, not precede it — desktop is that app-level ABI over IPC while the browser is the same ABI over wasm. `SporeClient` already takes a host-provided transport registry so this needs no UI change |
| M10-G SDK packaging/release plan for the app-level ABI, once M10-C lands | ⬜ not started | [Prns](https://github.com/KenAKAFrosty/Prns) already ships "one core, many language bindings" at wider scope — Rust/TS/Python/.NET/Go/Swift/JVM/Julia/C via `prns-host/bindings/*`, with a staged qualify→promote pipeline per SDK. A concrete reference for how each UI shim's release process could work; M10 doesn't currently address packaging at all, only that the ABI exists |

**Scope: the web node is not the landing site.** Two different products live in
this repo and M10 touches exactly one of them.

| | Source | Output | Audience |
|---|---|---|---|
| **Web node** | `web/` | `web/spore-standalone.html`, one self-contained file, zero external requests | someone *running a node* |
| **Landing site** | `site/` | GitHub Pages | someone *reading about* SPORE |

Separate builds, separate artefacts; the only coupling is that `site/build.mjs`
imports the vendoring helper and renders `web/README.md`. **M10 rebuilds the web
node only** — moving the site onto HARDBRUT/3 would be its own milestone, since a
docs site and a mesh client share almost no components, and conflating them is how
the design language forked the first time.

**Design system.** M10-D consumes **HARDBRUT/3**, a three-layer rebuild
(primitive tokens → semantic roles → patterns) of `supernihil/hardbrut`, vendored
at `web/vendor/hardbrut3/` — 36 CSS files (63.5 KB) plus `inter-900-latin.woff2`
(23,900 bytes, verified byte-identical to upstream). It supersedes the flat
`web/vendor/hardbrut/hardbrut.css` **for the web app only** — the Pages site and
Android still consume the flat copy, and the hard rule now records that split.
Two open questions inherited from the design system: it substitutes **Unicode
glyphs in the mono face** for a real icon set, and renders the brand as **type**
— the latter already agreeing with the "no icon, no mascot" rule.

**Definition of done:** one Rust implementation of conversations, contacts,
unread and delivery state, consumed by web, Android and CLI through a single
app-level ABI; `NodeController.kt` and the JS app blob reduced to UI shims; the
standalone still makes zero external requests (Inter 900 inlined base64, not
fetched); no screen renders a control whose backend is missing.

---

## Milestone 11 — Fragmentation where it belongs, and a way to measure it

**Goal:** move fragmentation from end-to-end to per-hop, keeping it *below* the
signature where it is; leave files as the one thing that works differently; and
stop guessing about the rest by building the harness that can measure it.

### Two ways to be too big, and they are not the same problem

The wire already draws the line. `plen` is a `u16` (`src/envelope.rs:191`), so an
envelope is structurally capped at 65 535 payload bytes — 65 649 on the wire with
a full source key and signature. Nothing larger can *be* an envelope. That is the
boundary, and it needs no new rule:

| | Too big for **this link** | Too big for **an envelope** |
|---|---|---|
| Threshold | the link's frame, 32 B to 64 KB | 65 535 B, structural |
| Mechanism | **fragmentation, below the signature** | **file: manifest + chunks** |
| Scope | one hop; fragments never relayed | end to end |
| Fragments/chunks are | unsigned; the reassembled whole is verified | content-addressed, each verified alone |
| Recovery | reassemble at the far end of the link | WANT the ids you are missing |

**Files are deliberately the special case.** A manifest naming content-addressed
chunks, pulled with bounded WANTs, with a few chunks pushed alongside the
manifest as an optimisation. Everything else — messages, announces, receipts,
posts — is just an envelope, and if it does not fit a hop, that hop splits it.
Generic objects do **not** get manifests: an earlier draft of this milestone
proposed that and it is dropped. It would have put a round trip in front of a
slightly-over-MTU message and given end-to-end machinery to a problem that is
local to one link.

### The layer that is missing

Fragmentation today is below the signature — correct — but **end to end**, which
is not. `Node::send` fragments once, at origination, at the **sender's own**
`self.mtu` (`src/node/send.rs:137`); the fragments then flood the mesh as
ordinary envelopes and a relay "just forwards the fragments"
(`src/node/ingest.rs:417`), reassembling only at the destination.

**The defect.** A fragment cannot be fragmented again — its header is
`orig_id ‖ idx ‖ count`, one level, no nesting. A Wi-Fi node that splits a 4 KB
message into 1364-byte fragments has emitted frames that **cannot cross a LoRa
hop anywhere on the path**, and no node on that path can repair it.

The five `n.mtu = n.mtu.min(m)` lines (`bridge/driver.rs:64`, and the same line
in `i2p`, `icmp`, `reticulum`) are not a bug — they are the workaround. A node
holding a narrow bridge drags its *entire* node MTU down so it pre-fragments
small enough for its own worst link. That helps only a node that owns the narrow
link. Nothing helps a node three hops upstream whose fragments must cross
**someone else's** LoRa link. The clamp is `min` and never restored, so detaching
the bridge does not raise it back. **Deleting all five is the acceptance test.**

### What per-hop buys, beyond correctness

- **Unsigned fragments stop travelling.** Today they flood the whole mesh and are
  verified only at the far end. Link-scoped, a fragment lives for one hop and the
  receiver reassembles and verifies immediately.
- **The reassembly buffer becomes per-neighbour** rather than keyed by remote
  origin. What one attacker can make you hold is bounded by their being someone
  you can actually hear — which is what made S-013 awkward to bound.
- **The fragment header leaves the protocol.** Per-hop fragments are never
  relayed, so their framing is a bridge concern like KISS already is, not §3
  wire. That drops `FRAG_OVERHEAD` from 36 bytes (16 envelope header + 2 plen +
  16 `orig_id` + idx + count) to about 6 (set id, index, count — the link already
  frames and addresses). At a 54-byte Zigbee frame that is 18 usable bytes per
  fragment today against 48: **2.7×**.
- **No MTU floor to pick.** "The smallest link we support" stops being a protocol
  constant; LoRa, Zigbee at ~54 B and JANUS at ~32 B differ in cost, not
  capability.

### A gap to close while doing it

`count` is one wire byte, so a set is at most 255 fragments — and the largest
envelope is 65 649 B. One set therefore carries only 51 255 B at a 237-byte LoRa
frame, 4 590 B at Zigbee's ~54, 3 825 B at LoRaWAN SF12. **Envelopes between
those figures and 64 KB are legal but uncarriable.** Invisible today because the
sender clamps itself; per-hop it becomes a hole a relay walks into. Widening the
count is nearly free once the header is link framing rather than frozen wire.

### Open, and for the simulator to answer

L1 must handle loss: one lost fragment costs the whole envelope for that hop.
Either hop-local NACK/retry, which needs a return path, or forward error
correction — which is what the fountain is good at, and rateless open-loop is far
more defensible *per hop*, where the buffer is per-neighbour and short-lived,
than end to end. **The fountain may well survive, relocated.** Measure it.

**Broadcast-only media — the accounting unit M11-D must keep.** On a transport
with no underlay address (`U = ()`: raw LoRa P2P, audio) a receiver cannot tell
two senders apart, so reassembly cannot be keyed by neighbour and the
per-neighbour argument does not reach it. **The interface is the finest unit of
accounting such a medium can offer, and it is enough:** anyone able to flood a
shared radio with fragment sets can also simply jam it, which is cheaper and
denies everything, so a reassembly budget adds no exposure the medium did not
already have.

What that budget must *not* do is let the damage escape the radio — one pool
evicted globally lets the loudest link empty it and take every other link's
reassembly with it. Charge each set to the interface it arrived on. This already
holds for today's end-to-end fragments (`enforce_partial_budget`,
`Node::partial_sets_on`) and M11-D must preserve it when reassembly moves to the
link.

**Tasks** (each a PR):

| Task | Status | Notes |
|---|---|---|
| M11-A Name the resource invariant, and test it | ⬜ | "No remote node can cause another to transmit, store, or process an unbounded amount without continuing evidence of demand, or an explicit bounded local allowance." Every mechanism already exists — bounded WANTs, `MAX_IDS_PER_GOSSIP`, per-interface token buckets, store budget, table caps, `MAX_ADOPT_BYTES`, expiry — but the invariant is named nowhere, so they read as unrelated defences. Write it into Part II and add a test per resource-consuming path. First: it is what the rest is argued against |
| M11-B `spore-sim`: deterministic simulator over the real implementation | ⬜ | Seeded, declarative scenarios, machine-readable metrics. First 100 nodes with loss and partitions; then 1k/10k, mobility, malicious nodes, asymmetric links, tiny stores. Metrics: delivery probability, median/p95/p99 delivery time, **bytes transmitted per delivered byte**, duplicate ratio, flood amplification, storage pressure, energy as TX/RX/CPU/wakeups. Must include **mixed-MTU topologies** — a Wi-Fi island bridged to LoRa is the case broken today and nothing would have caught it. Must exercise the real crate, or it validates the simulator instead of SPORE |
| M11-C Measure: fragment loss recovery, and the file push threshold | ⬜ | Needs B. Settles whether hop-local retry or a relocated fountain recovers a lost fragment better across loss rates, and settles how many chunks to push with a manifest. Decide on numbers |
| M11-D Link fragmentation at the bridge | ⬜ | The milestone. Split below the node and below the signature, reassemble at the far end, per-link MTU, per-neighbour bound (set id where the medium has no addresses). Header becomes link framing, not wire; widen `count` past 255 while it is being written. Then delete every `n.mtu.min(...)` and the end-to-end fragment path in `Node::send` |
| M11-E Files: push a few chunks with the manifest | ⬜ | The pull half already exists (`fetch_n` → WANT → `on_want`). Add pushing the first N chunks alongside the manifest so a small file needs no round trip. **Local policy, not a wire constant** — sender and receiver never need to agree, since the receiver ignores what it holds and WANTs the rest either way. Counted in *chunks*, not bytes, so it scales with the MTU; charged to the same per-interface budget; `0` and `1` both legal. N from M11-C |
| M11-F Sealed manifests do not fit a small link | ⬜ | `publish_file_sealed` needs **MTU ≥ 256** for even one id, ≥ 264 with an 8-char name — measured. Raw LoRa P2P tops out at ~255, so it misses by a byte; Meshtastic's 237 by 19. It fails cleanly (returns `None`) but a narrow-link node can never publish a sealed file. The ~82-byte sealed header sits *inside* the signed root, on top of the root's own 114 bytes of key and signature; it belongs in its own object the root names. Per-hop fragmentation does not fix this — the root must fit as a unit for a stranger to verify it |

**Considered and rejected: `created_at` + `lifetime` instead of absolute expiry.**
The stated motivation is that a relay might extend a message's lifetime. It
cannot: `expiry` is inside the signed bytes and the ID is their hash, so changing
it invalidates the signature and the id together. Stores already clamp the
horizon to 30 days and a clockless node ages by dwell. The change buys clarity of
expression and no property the protocol lacks. Revisit only if something else
needs `created_at`.

**Considered and rejected: manifests for generic objects.** Only files get a
manifest. Anything else that does not fit a hop is fragmented by that hop.

**Definition of done:** a bridge splits what its link cannot carry and the far
end reassembles; no `n.mtu.min(...)` remains anywhere; a Wi-Fi node and a LoRa
node three hops apart exchange an envelope larger than either link's frame, with
a `spore-sim` scenario proving it; files push a measured number of chunks and
pull the rest; the resource invariant is stated and has a test per path.

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
3. **M4 — Webnode as daily driver** (first runtime on the storage seam)
4. **M5 — Polish & hardening** (only after the above)
5. **M8 — Embedded ESP32 runtime** (raw-802.11 relay; first real MCU target) — after the still-open carried-forward items in M2/M4/M5, not ahead of them, unless deliberately reprioritized
6. **M10 — One application layer, three UI shims** — the three ABIs have already
   diverged, and every capability gap hit while building the web node had the same
   shape: the capability present in the portable core, only the export missing
   (`spore_node_peers`, `spore_node_files`, `spore_node_unsubscribe`). M10-C is
   the answer to that pattern
7. **M11 — Fragmentation where it belongs** — M11-A first, since it is what the
   rest is argued against; M11-D fixes a live cross-MTU delivery failure

The gaps in the numbering are milestones that finished. They are not summarised
here — see `git log`.

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
