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
| 4 | Desktop binaries on a version tag, checksums, and the Wry webview over a native node | [#291](https://github.com/sloev/spore/issues/291) |
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
here. **Anything promoted into a release section must already have a detailed
issue**; the roadmap carries only the link.

### Candidates for 0.9 — richer bridges and radio

- Meshtastic / RNode / LoRa: the first real sub-GHz path. The part choice is
  settled (E22-900M30S, SX1262); listen-before-talk is worth more than the chip.
- **Listen-before-talk** (ETSI EN 300 220). The largest practical airtime win
  available on the same hardware: LBT+AFA replaces the 10% duty cycle, which is a
  10× multiplier for the cost of code rather than money.
- Audio modem: measure the symbol error rate before adding coding.
- A **companion profile** — a node that carries its own traffic and relays
  nothing, for a phone on battery or a metered link. Local policy, never a
  protocol role.
- SNR-weighted contention window on shared media: the weakest signal rebroadcasts
  first, because it is the one that reaches nodes nobody else can.
- More bridges off the 🧪 list, each with a dated run.
- **Per-neighbour INV memory.** Two nodes that meet repeatedly re-offer each other
  the same ids and are re-declined every time. Bounded by `MAX_IDS_PER_GOSSIP`, so
  it is waste rather than a vulnerability. The hard part is bounding the
  remembering — a table keyed by neighbour is a table a stranger can grow — and
  expiring it, since a peer that evicted its copy must be offered it again.

### Candidates for 1.0 — daily-driver depth

- Chat attachments: ExoPlayer preview, edit-after-send, public-file single bubble.
- Web node parity with the shared communicator, on the single ABI.
- Private-group `key_id` divergence badge. Honest about disagreement; never
  claims roster consensus.
- Export polish, with the filesystem warning.
- The hardware matrix actually exercised: the seven-day window, backup exclusion
  and migration, beacon cadence on real radio, two-real-NATs Direct punch.
- Remaining security items:
  - **Identity compromise: decide, do not drift.** A stolen seed signs as that
    address forever. Either scope it out in the spec with the reasoning, or design
    the successor statement. Group keys have `rotate`/`rekey_seal`/`contribute`;
    identity has nothing, and the asymmetry should be deliberate.
  - **Encrypted groups have no roster.** Membership — and so who a contribution is
    sealed to — is the application's problem, and in a partition two halves
    diverge onto different keys. Solving it properly is distributed agreement
    rather than cryptography, and it is the largest honest gap between a SPORE
    group and a messenger's.
  - **A stolen prekey secret is a decryption oracle for its window.** Healing
    survives it for at most seven days, because contributions are sealed to
    prekeys and those expire. Shortening the window is a knob, not a fix.
  - **Move retired designs out of the normative spec.** §3 still spends most of
    its length on the end-to-end fountain, which nothing emits and nothing parses.
    A reimplementer reading Part I should meet only rules that bind them.
  - **An independent review.** [The security matrix](SECURITY_MATRIX.md) has an
    empty column for all twelve components, and that is the most important cell
    in it.
- `with_node` reentrancy: a closure that calls back into its own hub panics naming
  the bug rather than deadlocking silently, but the call shape still cannot
  succeed. Making it work needs a reentrant path or a documented queue, not a
  better error.
- Benchmark suite: throughput and memory, reproducible, tracked per platform.

### Undecided, or later

- Native WebTransport / QUIC. The spike validated it; iroh's QUIC is not HTTP/3
  WebTransport, so this is its own piece of work.
- Multi-device identity, and identity rotation.
- Boot receiver; sound and particles behind a setting, default off.
- `spore-sim` at real scale: 1k/10k nodes, mobility, adversaries.
- Documentation: [`SPEC.md`](SPEC.md) opening with the problem rather than the
  vocabulary, mobile-readable tables, the `DEV_GUIDE`/`DEVELOPER` rename, a
  README→site bridge, homepage depth, an Apps comparison, web-node microcopy.
- Anything requiring a wire-format change.

---

## Post-release workflow

1. Tag the release. CI attaches the artefacts.
2. Delete the shipped section, or collapse it to a pointer at the tag.
3. Plan: which backlog items become the next release's deliverables?
4. For every task in the new section: link its issue, or open one first.
5. Write the new section and begin.

A release is not done when CI is green. It is done when the deliverables exist,
the acceptance demos are recorded, and **a stranger can download the artefacts
and reproduce them**.
