# SPORE — release plan

**Project:** `sloev/spore` · **Current version:** 0.7.0

This is the single forward-looking plan. Releases are defined by **concrete
artefacts** a person can download and run, plus the **communicator features** that
work across those artefacts. Tasks live under the release they serve.

**Process:** when a release ships, its section is deleted (or collapsed to a
one-line pointer at a git tag). The next release is then planned from the
remaining backlog. PRs are the merge units inside a release; the release itself
is the unit of planning.

**Only what is ahead lives here.** A task is deleted when it ships — what it did
and why is in the commit that did it.

| Question | Where it is answered |
|---|---|
| What is planned? | this file |
| What shipped, when, and why? | `git log` — the commit message carries the reasoning |
| What stops it breaking again? | the test that came with the fix |
| What actually works today? | [What works today](STATUS.md), counted from the tree |

**Read order for agents:** [Mission](MISSION.md) → this file → [Spec](SPEC.md) as
needed + `git log`.

---

## Hard rules (do not violate)

- **Frozen:** the wire format — `reference/vectors.json`, its generator
  `examples/gen_vectors.rs`, and the API pin `tests/api_freeze.rs`. No change
  without the `allow-frozen-change` label. The C ABI (`bindings/spore.h`) is
  **freeze-on-remove**: symbols may be added freely, none removed or renamed.
- **Honesty over polish:** 🧪 markers, "still open", served-vs-fetching language,
  and **no fake UI** — never a control whose backend is missing.
- **HARDBRUT upstream** is normative for colour, contrast, motion and components.
- **Zero external network requests** in `web/spore-standalone.html`.
- Motion fully static under reduced motion. Sound and particle bursts stay **off**
  until the user enables them.
- One concern per PR. Distinguish **Verified** (code/tests) from **Reasoned** from
  **Needs device run**.
- **No icon, no mascot.** The brand is the wordmark.
- **Every task references an issue.** If one exists, link it. If not, open it
  first — the roadmap carries the link, never a second copy of the design notes
  that can drift from it.

---

## Next release: 0.8.0 — Holdable nodes

**Status:** in progress · umbrella [#303](https://github.com/sloev/spore/issues/303)

**Goal:** make the three runtimes a person can actually hold — a desktop, an
Android phone, a $5 board — genuinely work, on the same Rust core.

### Deliverables

| Artefact | What ships |
|---|---|
| **Shared Rust kernel + communicator** | One portable core and one application layer (conversations, contacts, unread, delivery state), used by CLI, Android and web. No more divergent host layers. |
| **Desktop (Win / Linux / macOS)** | Auto-built CLI binary plus a webview app over a **native** node (Wry, not Tauri). Identity and store survive restart. Prebuilts attached to the release. |
| **Android APK** | Already built on every PR. Now a full node: identity, store, communicator features, usable bridges. |
| **ESP32-S2 Mini firmware** | Auto-built `.bin`. Flash it and it is a headless relay. Identity and store survive a power cycle. |
| **Bridges that are no longer 🧪** | UDP (desktop ⇄ Android), a USB/KISS or BLE tether (ESP32 ⇄ host), and whatever the four demos need. |
| **Communicator features across CLI, Android and web** | Identity/seed sheet, 1:1 DMs, open groups, private groups, basic file send/receive, store-and-forward across restarts, an honest peer list and diagnostics. |

### Acceptance demos

Recorded in [`docs/HARDWARE.md`](HARDWARE.md) or
[`android/TESTING.md`](../android/TESTING.md), with dates.

- **Demo A** — two desktop nodes: a message crosses, and a second survives the
  receiver being offline and coming back.
- **Demo B** — Android ⇄ desktop over UDP. Both send. Android is killed and
  restarted; identity remains; messages still work.
- **Demo C** — three hops: Android → ESP32 → desktop, with the direct path
  disabled. The message arrives.
- **Demo D** — store and forward: Android sends while the desktop is offline, the
  ESP32 holds the envelope, the desktop comes online and receives it.

Until a dated run exists, every radio path stays 🧪. **CI-green is not working.**

### Tasks, in order

Biggest risk first: prove the thing that would make the rest pointless, then pair
devices.

| # | Task | Issue |
|---|---|---|
| 1 | Finish the application layer: Android chat and feed onto the shared ABI, and the CLI with it | [#306](https://github.com/sloev/spore/issues/306) |
| 2 | Desktop ⇄ Android over UDP, both directions, surviving restarts on both sides | [#307](https://github.com/sloev/spore/issues/307) |
| 3 | ESP32: identity re-run, power-cycle store, solo TX-shape, two-board air, tether | [#149](https://github.com/sloev/spore/issues/149) |
| 4 | Desktop binaries on a version tag, checksums, and the Wry webview over a native node | [#334](https://github.com/sloev/spore/issues/334) |
| 5 | Demos A–D green and recorded | [#303](https://github.com/sloev/spore/issues/303) |
| 6 | Release checklist: artefacts, evidence, honesty | [#308](https://github.com/sloev/spore/issues/308) |

**Do not start** the desktop GUI, ESP32 OTA, or signing and notarisation before
1–3 have dates. Those are how a working node gets distributed, not how it becomes
one.

### Definition of done

You can truthfully say:

> SPORE runs as the same Rust protocol core on a desktop, an Android phone and an
> ESP32 board.

and the four demos work end to end.

---

## Backlog

Deliberately unscheduled. After 0.8.0 ships, the next release is chosen from
here. Every item is an issue — the reasoning lives there, where the work is done,
and this list is only the shape of what is ahead.

### Candidates for 0.9 — richer bridges and radio

| | Issue |
|---|---|
| Sub-GHz LoRa bridge — the first path where a node can be kilometres from anything | [#310](https://github.com/sloev/spore/issues/310) |
| Listen-before-talk (ETSI EN 300 220) — a 10× airtime multiplier on hardware already bought | [#311](https://github.com/sloev/spore/issues/311) |
| Audio modem: measure the symbol error rate *before* choosing a code | [#312](https://github.com/sloev/spore/issues/312) |
| A companion profile — carries its own traffic, relays nothing. Local policy, never a role | [#313](https://github.com/sloev/spore/issues/313) |
| SNR-weighted contention window — the weakest signal rebroadcasts first | [#314](https://github.com/sloev/spore/issues/314) |
| Per-neighbour INV memory — stop re-offering what a neighbour already declined | [#315](https://github.com/sloev/spore/issues/315) |
| Take bridges off the 🧪 list, one dated hardware run at a time | [#316](https://github.com/sloev/spore/issues/316) |

### Candidates for 1.0 — daily-driver depth

| | Issue |
|---|---|
| Chat attachments: preview, edit-after-send, one bubble per public file | [#324](https://github.com/sloev/spore/issues/324) |
| Web node parity on the shared communicator | [#325](https://github.com/sloev/spore/issues/325) |
| Private-group `key_id` divergence badge — reports a split, never claims consensus | [#326](https://github.com/sloev/spore/issues/326) |
| Export polish, with the filesystem warning | [#327](https://github.com/sloev/spore/issues/327) |
| The hardware matrix rows deferred to device QA | [#328](https://github.com/sloev/spore/issues/328) |
| `with_node` reentrancy is unsupported, not merely guarded | [#322](https://github.com/sloev/spore/issues/322) |
| Benchmark suite: throughput and memory, per platform | [#323](https://github.com/sloev/spore/issues/323) |

**Security items still open.** These are decisions as much as tasks, and each
issue carries the reasoning rather than a checklist:

| | Issue |
|---|---|
| Identity compromise: decide, do not drift | [#317](https://github.com/sloev/spore/issues/317) |
| Encrypted groups have no roster — the largest honest gap versus a messenger | [#318](https://github.com/sloev/spore/issues/318) |
| A stolen prekey secret is a decryption oracle for its window | [#319](https://github.com/sloev/spore/issues/319) |
| Move the retired end-to-end fountain out of the normative spec | [#320](https://github.com/sloev/spore/issues/320) |
| **An independent security review** — the empty column in [the matrix](SECURITY_MATRIX.md) | [#321](https://github.com/sloev/spore/issues/321) |

### Undecided, or later

| | Issue |
|---|---|
| Native WebTransport / QUIC — iroh's QUIC is not HTTP/3 WebTransport | [#329](https://github.com/sloev/spore/issues/329) |
| Multi-device identity, and identity rotation | [#330](https://github.com/sloev/spore/issues/330) |
| Boot receiver; sound and particles behind a setting, default off | [#331](https://github.com/sloev/spore/issues/331) |
| `spore-sim` at real scale: 1k/10k nodes, mobility, adversaries | [#332](https://github.com/sloev/spore/issues/332) |
| Documentation backlog from the #290 audit | [#333](https://github.com/sloev/spore/issues/333) |

**Anything requiring a wire-format change** is not on this list and does not get
one without a `ver` bump, which is a hard fork. That is the point of the freeze,
not an obstacle to be worked around.

## Post-release workflow

1. Tag the release. CI attaches the artefacts.
2. Delete the shipped section, or collapse it to a pointer at the tag.
3. Plan: which backlog items become the next release's deliverables?
4. For every task in the new section: link its issue, or open one first.
5. Write the new section and begin.

A release is not done when CI is green. It is done when the deliverables exist,
the acceptance demos are recorded, and **a stranger can download the artefacts
and reproduce them**.
