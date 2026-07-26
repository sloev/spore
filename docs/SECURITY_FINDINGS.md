# Security findings register

Every security issue found in this repository, with enough detail to re-verify it
independently: where it lived, what an attacker could do, how it was reproduced,
what changed, and which test now fails if the fix is reverted.

Two rules for this file:

- **A finding is only listed once it has been reproduced.** Reasoning about a code
  path is how findings are *discovered*; a proof of concept is how they are
  *confirmed*. Several plausible-sounding candidates were investigated and turned
  out to be already handled — those are in [Investigated and not a
  finding](#investigated-and-not-a-finding), because knowing what was checked and
  cleared is as useful as knowing what broke.
- **Severity describes reachability, not cleverness.** "Any peer on the medium,
  no key, one packet" outranks anything needing a position or a secret.

Nothing here changes the frozen 1.0 wire format. Where a fix changes *behaviour*
(as opposed to fixing a crash), it says so explicitly.

## Summary

| ID | Category | Severity | Reproduced | Fixed | Regression test | Behaviour change |
|---|---|---|---|---|---|---|
| [S-001](#s-001) | Remote DoS (panic) | **High** | ✅ | ✅ | ✅ | no |
| [S-002](#s-002) | Spoofing / trust boundary | **High** | ✅ | ✅ | ✅ | yes (relays verify before binding) |
| [S-003](#s-003) | Congestion control bypass | **High** | ✅ | ✅ | ✅ | yes (stamp threshold) |
| [S-004](#s-004) | Third-party DoS (quota) | Medium-High | ✅ | ✅ | ✅ | no |
| [S-005](#s-005) | Undefined behaviour (FFI) | Medium-High | ✅ | ✅ | ✅ | yes (null → error, not abort) |
| [S-006](#s-006) | Resource exhaustion | Medium | ✅ | ✅ | ✅ | no |
| [S-007](#s-007) | Remote OOM (TOCTOU) | Medium | ✅ | ✅ | ✅ | no |
| [S-008](#s-008) | Frame corruption / injection | Medium | ✅ | ✅ | ✅ | no |
| [S-009](#s-009) | Silent packet loss | Low | ✅ | ✅ | ✅ | no |
| [S-010](#s-010) | False continuity claim | Low | ✅ | ✅ | n/a | no |

Earlier, in #15: five unbounded-read bugs (`kiss_stream`, `bag` ×2, `copyparty`,
`i2p`, spill adoption in `store`). Same class as S-007, already merged, each with
a bound and a test.

---

## S-001

**Zero-count fragment is a remote panic.** High. `src/lib.rs` — `Fountain::add`,
`selection`.

**Root cause.** `count` is `e.payload[17]`, one byte taken straight off the wire.
A count of zero passed the `count != self.count` guard — zero equals zero — and
reached `selection`, whose empty-selection fallback is `b.set(idx as usize % count)`.
Division by zero is a panic.

**Exploit.** Send one public `FRAGMENT` envelope with `payload[17] == 0`. A public
flood is deliverable to anyone, so no key, no session and no forgery are required;
the panic takes the process, and it can be repeated. Any peer can stop any node
that can hear it.

**Reproduced.** Found by `src/robustness.rs` on its first run, before the guard
existed: `attempt to calculate the remainder with a divisor of zero`, reached
through `Node::on_rx`.

**Patch.** Rejected at two levels. `Fountain::add` refuses `count == 0` as
malformed. `selection` independently refuses zero and anything above the 256 bits
its 32-byte digest can index — unreachable today, since the wire field is a `u8`
and the sender asserts `count <= 255`, but both of those are facts about *callers*
and this is a pure helper that should not depend on them.

**Test.** `a_zero_count_fragment_does_not_kill_the_node` — the specific packet,
then every `count` in `0..=255` across five indices.

---

## S-002

**Address learning trusted a flag instead of a signature.** High.
`src/bridge/neighbors.rs` — `Neighbors::snoop`; `src/lib.rs` — `ingest` path
learning.

**Root cause.** Both places that bind a SPORE address to an underlay address
checked `flags & SIGNED` and never called `verify()`. That flag is one bit chosen
by whoever wrote the frame. `Src::Short` is worse than unverified — it carries an
8-byte address and no key, so it cannot be verified at all.

**Exploit.** Nothing secret is needed, because a public key is public. Copy the
victim's into `src`, set the `SIGNED` bit, attach 64 zero bytes. `addr_of` derives
the victim's address from that key, so the table binds *victim address → attacker's
underlay address*, and every directed send for the victim is unicast to the
attacker. Sealed payloads stay unreadable, so this discloses nothing; it redirects
delivery. The path table variant binds the victim to whichever interface the
forgery arrived on.

**Reproduced.** Proof of concept bound a victim's address to attacker-chosen
underlay addresses 666 and 777, with `verify()` returning `false` on the very same
frames. The verifier existed and was never called on this path.

**Patch.** `Src::Full` must pass `verify()`, which proves a signature by the key
the address is derived from. `Src::Short` teaches nothing. Failing to learn is
safe: the bridge falls back to broadcast, which always reaches the peer.

**Behaviour change.** SPEC says "relays never verify — endpoints do". Verification
is confined to the paths that *bind local trust state* — neighbour table, path
table, quota attribution — computed once per envelope and only after the dedup and
expiry checks, not on every relayed envelope. This is a real cost on constrained
hardware (an Ed25519 verify per newly-seen signed envelope) and was accepted
deliberately; see `docs/DESIGN.md`.

**Tests.** `a_forged_signature_cannot_bind_a_victims_address`,
`a_short_source_is_a_claim_not_evidence`,
`path_learning_also_refuses_a_forged_signature`.

---

## S-003

**A free stamp bypassed both quotas and backpressure.** High.
`src/congestion.rs` — `Quotas::admit`, `admit`.

**Root cause.** Both admission paths exempted `stamp > 0`. A stamp is the count of
leading zero bits of the envelope id, and the id is a hash — so class 1 is not
work. Half of all envelopes have it by chance, and grinding one costs about two
tries.

**Exploit.** §10(d) per-source quotas and §10(c) backpressure could be ignored for
approximately nothing, deliberately by grinding or accidentally by luck. The
mechanism intended to stop one node flooding the mesh bounded nothing.

**Reproduced.** Found while writing the S-004 proof of concept, which refused to
reproduce because the victim's envelope happened to carry class 2. Instrumented:
12 of 20 arbitrary junk envelopes were exempt from the quota by luck alone.

**Patch.** `STAMP_QUOTA_BYPASS_BITS = 16` (~65k tries) replaces `> 0` in both
paths. Below that class mail still flows, charged to its source's budget, and
stamp still orders eviction and TX priority (§10.3).

**Behaviour change.** Yes, and it is the reason this finding needed sign-off. A
node running this throttles low-class stamped traffic that older nodes pass —
a stricter *local policy*, not a wire change. SPEC supports it: "priority is
bought, not claimed" (§2), "priority is proof of work" (§10). An existing test
asserted that class 3 rides through backpressure; that assertion encoded the
defect and is now inverted. Deployer note in `docs/BRIDGES.md`.

**Test.** `a_free_stamp_does_not_buy_a_quota_exemption`, plus the inverted
`congestion_primitives`.

---

## S-004

**Quota attribution accepted an unauthenticated source.** Medium-High.
`src/lib.rs` — `ingest`.

**Root cause.** The per-source token bucket was keyed on whatever address `src`
named — an unverified `Src::Full`, or a `Src::Short` with no key at all.

**Exploit.** Spray unstamped junk naming a victim as the source. The victim's
bucket drains, and their own mail then fails admission: not stored, not relayed.
A denial of service against a third party, paid for in junk. Degraded rather than
absolute while S-003 stood, since roughly half of the victim's traffic bypassed
the quota by luck.

**Reproduced.** Yes — see S-003 for why the first attempt was inconclusive.

**Patch.** Only a verified signature spends a named budget. An unverified `Src::Full`
or any `Src::Short` shares one `UNATTRIBUTED` bucket: still bounded, but impossible
to aim at a chosen victim. The verify is the same one S-002 added, computed once
and reused, so this costs no extra crypto.

**Test.** `a_forged_source_cannot_spend_a_victims_quota`.

---

## S-005

**Panics could unwind across the C ABI.** Medium-High. `src/ffi.rs`.

**Root cause.** Fifteen `extern "C"` functions, no `catch_unwind`, and no
`panic = "abort"` to make unwinding impossible instead. Separately, `arr32`/`arr8`
read fixed-size keys with `copy_from_slice`, and the `slice` helper returns an
empty slice for a null pointer — so the length mismatch panicked.

**Exploit.** Not an attack so much as ordinary misuse with an undefined outcome. A
wrapper passing `None`/`nil`/`null` for a key — Python, Go and JS can all do this
by accident — reached the panic, which then unwound into foreign frames that have
no unwind tables. That is undefined behaviour, not a catchable exception, so the
result is a corrupted host process.

**Reproduced.** Every affected entry point called with null pointers.

**Patch.** Both halves. The helpers no longer panic: a short or null pointer yields
zeroes, a well-defined wrong key that fails the way a wrong key fails. And `guard`
wraps every entry point with `catch_unwind`, returning the failure value each
function already documents. The backstop matters because the crypto and decode
paths below are not audited for panic-freedom.

**Frozen interface.** Untouched — `bindings/spore.h` and every signature are
unchanged. This is behaviour only: a null key now returns an error instead of
aborting.

**Tests.** `null_pointers_return_failure_instead_of_unwinding`,
`a_panic_inside_the_boundary_becomes_a_failure_value`.

---

## S-006

**Neighbour table had no bound on address count.** Medium.
`src/bridge/neighbors.rs`.

**Root cause.** `keep: 3` bounds the bindings held *per address*. Nothing bounded
the number of addresses, and `expire` was never called from the driver loop.

**Exploit.** S-002's fix requires a valid signature to learn, which raises the
price of an entry from nothing to almost nothing: minting an identity is one
Ed25519 keypair. A peer can present as many genuinely signed addresses as it likes
and grow the map for the full two-hour TTL. **Verified is not the same as scarce**,
and it is worth recording that the S-002 fix left this open rather than closing it.

**Reproduced.** 500 identities inserted against a table with no cap.

**Patch.** `MAX_NEIGHBOURS = 4096`, matching the cap `congestion::Quotas` already
placed on tracked sources. Only a new address pays for the check: stale bindings
are reclaimed first, and the least-recently-heard address is evicted only if the
table is still full. Eviction is safe here in a specific way — an address that
fails to resolve falls back to broadcast, so the table degrades to slower, never
to unbounded and never to wrong.

**Tests.** `the_table_cannot_be_grown_without_bound`,
`stale_bindings_are_reclaimed_before_anything_live_is_evicted`.

---

## S-007

**Spool read was bounded by a stat, not by the read.** Medium.
`src/bridge/spool.rs` — `ingest_dir`.

**Root cause.** `fs::metadata` checked the size, then `fs::read` read the file.
Two syscalls with a gap between them.

**Exploit.** A spool is written by someone else by definition — that is what a
spool *is* — so the file can be replaced or extended in that gap, and the read
then buffers whatever is there at the later moment.

**Patch.** `read_bounded` caps the read itself via `Read::take`. An over-cap file
is left for the next sweep, whose stat sees the real size and clears it, so it
cannot jam the queue either.

**Test.** `the_read_itself_is_capped_not_just_the_stat`.

---

## S-008

**Reticulum UDP fed its framer from any source.** Medium.
`src/bridge/reticulum.rs` — `run_udp`.

**Root cause.** `recv_from` ignored the source address, and this is the one
datagram bridge that carries KISS framing state *across* datagrams.

**Exploit.** Where `udp::run_group` can ignore a stranger's datagram as one bad
envelope, here a stranger's bytes interleave into a frame the companion is midway
through sending — corrupting a good frame rather than merely adding a bad one. Any
host that knew the bind port could do it.

**Patch.** Datagrams whose source is not the configured peer are dropped, keeping
the framer single-sourced.

**Test.** `foreign_bytes_would_corrupt_a_half_assembled_frame` demonstrates the
damage the source check prevents.

---

## S-009

**ICMP assumed a 20-byte IP header.** Low. `src/bridge/icmp.rs` — `run`.

**Root cause.** A raw `IPPROTO_ICMP` socket hands back the IP header, which is 20
bytes only when it carries no options. The runner skipped a fixed 20.

**Impact.** Not memory-unsafe — it fails closed. Every packet carrying
record-route or timestamp options decoded from four bytes off, failed the
checksum, and was dropped silently. Those options are exactly what a
"diagnostics only" network adds, which is the kind of network this bridge exists
for.

**Patch.** `ipv4_payload_offset` reads the IHL field and range-checks it against
the datagram, since IHL is chosen by whoever sent the packet.

**Tests.** `the_ip_header_length_is_read_not_assumed`,
`a_malformed_ip_header_cannot_steer_the_read`.

---

## S-010

**`CONTINUITY.md` promised an offline build the repo could not deliver.** Low, but
load-bearing. `docs/CONTINUITY.md`.

**Root cause.** Three places claimed a clone plus a toolchain rebuilds everything
offline — "the build vendors its dependencies and needs no registry". There is no
`vendor/` and no `.cargo/config.toml`. `Cargo.lock` is committed, which pins
versions, but a lockfile *names* crates, it does not contain them.

**Impact.** Not an attack; a false promise in the one document whose entire
purpose is that promise, which would only ever fail on the machine least able to
fix it — offline, with no registry to reach.

**Reproduced.** With a fresh `CARGO_HOME`, `cargo build --offline` fails at
resolution on the first dependency: `error: no matching package named blake2 found`.

**Patch.** Made the claim true rather than deleting it.
`scripts/make-offline-bundle.sh` vendors the dependencies, writes the config that
actually activates `vendor/`, and verifies itself with `cargo build --offline`.
Confirmed with an empty `CARGO_HOME` and a fresh target directory: a cold compile
of every dependency, no registry. `vendor/` stays gitignored, because ~10 MB of
third-party source would contradict the size argument the same document makes.

---

## Investigated and not a finding

Recorded because a cleared hypothesis is worth as much as a confirmed one, and
because two external reviews asserted several of these as defects.

| Claim | Finding |
|---|---|
| "Signatures not verified before trusting content" | Already verified wherever content is trusted: `absorb_announce`, `absorb_manifest`, session handling all call `verify()` first. The gap was address *learning* only (S-002). |
| "No per-source quotas / rate limits" | `congestion::Quotas` exists with per-source token buckets, a 4096-source cap, and LRU eviction whose comment already anticipated forged source sprays. |
| "Neighbour entries unbounded / no TTL" | TTL and a 3-binding-per-address cap already existed. The *address count* was the gap (S-006). |
| "No consolidated bridge threat model" | `docs/BRIDGES.md` has a bridge-security section with the rule, a table of every bound, and per-bridge notes. |
| "`Cargo.lock` should be committed / deps pinned" | Already committed; `ed25519-dalek`, `zeroize`, `base64ct` already exact-pinned with a stated reason. |
| "wasm randomness unvalidated" | CI already asserts the wasm has exactly one import and that it is `spore_fill_random` — stricter than the proposed check. |
| "No reproducible-build verification" | CI regenerates `reference/vectors.json` and fails on any diff, and runs `check_docs_sync.py`. Not bit-for-bit binary reproducibility, but determinism is verified. |
| Fountain `count > 256` indexing past the digest | Not reachable: the wire field is a `u8` and the sender asserts `count <= 255`. Guarded anyway in S-001, because those are facts about callers. |

## Still open

Carried deliberately, not overlooked.

- **`send` panics** on objects needing more than 255 fountain chunks — a public
  API that aborts on input size. Backlog item; may need API evolution.
- **`Cargo.lock` is version 4** while the dependency pins claim Rust 1.75
  compatibility. The MSRV story and the lockfile disagree, which matters most for
  the offline path S-010 just fixed.
- **`seen`, `frags`, ack and rpc/feed tables** need the S-006 treatment: bounds
  and timeouts, not just TTLs.
- **INV/WANT** admission is unbounded — a classic amplification surface.
- **Ratchet, mix and lock ordering** are unreviewed: deeper state machines, but
  less "anyone on the medium can crash you" than the items above.
- **Bridges never audited**: `ssb`, `foldersync`, `csma`, `meshtastic`, `audio`,
  `serial`, `tcp`, `hub`, `ax25`, `tor`, `udp`.
