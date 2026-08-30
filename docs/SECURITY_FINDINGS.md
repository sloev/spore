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

**Nothing here changes the wire format.** `gen_vectors` output stays byte-identical
to `reference/vectors.json` throughout, so every reference decoder, vector and
`spore.h` symbol is untouched. One finding (S-011) does change a frozen *Rust*
signature, which is called out in its entry. Where a fix changes *behaviour* rather
than fixing a crash, it says so explicitly.

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
| [S-011](#s-011) | Public API panic | Low | ✅ | ✅ | ✅ | yes (`send` returns `Result`) |
| [S-012](#s-012) | Reflection / amplification | **High** | ✅ | ✅ | ✅ | yes (per-link WANT budget) |
| [S-013](#s-013) | Resource exhaustion (10 tables) | **High** | ✅ | ✅ | ✅ | yes (eviction under pressure) |
| [S-014](#s-014) | False continuity claim (MSRV) | Low | ✅ | ✅ | ✅ (CI) | no |
| [S-015](#s-015) | Remote OOM (5 unbounded reads) | Medium | ✅ | ✅ | ✅ | no |
| [S-016](#s-016) | Resource exhaustion (filename sets) | Low | ✅ | ✅ | ✅ | no |
| [S-017](#s-017) | Resource exhaustion (ratchet keys) | Medium | ✅ | ✅ | ✅ | no |
| [S-018](#s-018) | Availability (mutex poisoning) | Medium | ✅ | ✅ | ✅ | yes (recover, not die) |
| [S-019](#s-019) | Remote DoS (integer overflow) | Medium | ✅ | ✅ | ✅ | no |
| [S-020](#s-020) | No post-compromise security (topics) | Medium | ✅ | ✅ | ✅ | no |
| [S-021](#s-021) | Release integrity (stale/dead release) | Low | ✅ | ✅ | ✗ manual | release layout |
| [S-022](#s-022) | False security claim → no seal FS | Medium | ✅ | ✅ | ✅ | **yes — `seed()` is no longer a whole backup** |
| [S-023](#s-023) | Spec violation / duty cycle (beacon) | Medium | ✅ | ✅ | ✅ | yes (hourly flood, not 5 s) |
| [S-024](#s-024) | Doc-vs-code mismatch | Low | ✅ | ✗ recorded | n/a | no |
| [S-025](#s-025) | Release integrity (empty "latest") | Low | ✅ | ✅ | ✗ manual | release plumbing |
| [S-026](#s-026) | Release integrity (nightly accumulation) | Low | ✅ | ✅ | ✗ manual | release plumbing |
| [S-027](#s-027) | Persistent DoS (folder sync) | Medium | ✅ | ✅ | ✅ | no |
| [S-028](#s-028) | Memory amplification (materialize) | Low | reasoned | ✅ | ✗ | no |
| [S-029](#s-029) | Release integrity (my own fix, racy) | Low | ✅ | ✅ | ✗ manual | release plumbing |
| [S-030](#s-030) | Release integrity (S-025's trap, in S-025's fix) | Low | ✅ | ✅ | ✗ manual | release plumbing |
| [S-031](#s-031) | Remote CPU exhaustion (audio) | **High** | ✅ measured | ✅ | ✅ | no |
| [S-032](#s-032) | Delivery-receipt forgery (§8) | Medium-High | ✅ | ✅ | ✅ | yes (receipts checked against the destination) |

Two entries above are deliberately not "fixed": S-024 records mismatches whose
resolution is a design choice rather than a repair. And two have no automated test,
both release plumbing — the checklist step in `docs/CONTRIBUTING.md` is the only guard,
which is precisely how S-025 shipped.

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
of every dependency, no registry. `vendor/` stays gitignored, because ~30 MB of
third-party source would contradict the size argument the same document makes.

---

## S-011

**`send` aborted the process on an oversized object.** Low (local input, not
remote). `src/lib.rs` — `Node::send`.

**Root cause.** `assert!(count <= 255, ...)`. The fountain header carries `count`
as one wire byte, so a set addresses at most 255 chunks; past that, `send`
panicked.

**Impact.** Not remotely triggerable — `send` is called by the local application
with its own data — so this is API quality rather than an attack. But a library
that aborts the host process because a payload was big is a poor contract, and the
size often is not the caller's to choose (a file picker, a received blob, a user
paste).

**Patch.** `send` now returns `Result<Vec<Forward>, TooLarge>`, and `TooLarge`
reports the chunks needed, the chunk size in force, and a `Display` that names the
file/manifest layer. `Hub::send` forwards the error rather than swallowing it —
silently sending nothing would be the worst available outcome. The wasm and JNI
entry points return their existing "nothing sent" signal (0).

**Frozen interface.** **Yes** — `tests/api_freeze.rs` pinned `send` at
`-> Vec<Forward>`, and that assertion is updated. The wire format is untouched:
`gen_vectors` output is byte-identical, so this is a Rust API break with no
protocol consequence. Landed under `allow-frozen-change` as a deliberate decision
now that the project has no downstream users; the first attempt at this was an
additive `try_send` alongside a still-panicking `send`, which was a worse API kept
only to respect a constraint that turned out not to bind.

**Test.** `an_oversized_object_is_an_error_not_a_panic`.

---

## S-012

**WANT was a 32x unauthenticated amplifier.** High. `src/lib.rs` — `on_want`,
`on_inv`.

**Root cause.** A WANT payload is a list of 16-byte ids and `on_want` answered each
one with a whole stored envelope. The payload length is a `u16`, so one packet can
list ~4095 ids. Per §6, INV/WANT are per-link, `hops=0`, consumed on receipt —
which also means they return from `on_rx` *before* dedup and before the quota.

**Exploit.** Send one small unsigned WANT listing ids the victim holds; receive
their whole store back. Nothing is signed, so this needs no identity, and nothing
dedups it, so the identical packet can be replayed forever. Three costs, not one:
bandwidth, disk (`store.wire` reads from the spill directory on eviction), and — on
a radio link — airtime that §10 caps at 10% by law. Worth noting the reflection
angle: on a bridge where the source address is not verified (UDP, broadcast), the
flood can be aimed at a third party.

**Reproduced.** Measured before the fix: a 6,418-byte WANT returned 205,600 bytes
across 400 envelopes — **32.0x** — and an identical replay served all 400 again.
The ceiling is the largest envelope divided by 16, around 4000x.

**Patch.** Two bounds, because they address different halves. `MAX_IDS_PER_GOSSIP`
(64) caps what one packet can buy. A per-interface `TokenBucket`
(`DEFAULT_GOSSIP_BUDGET`, 32 KiB/s) caps a *stream* of them. Per-interface rather
than per-source because a WANT carries no identity worth having — the link it
arrived on is the only attributable thing. `on_inv` takes the same id cap, since an
INV listing thousands of ids we lack would have us emit an equally large WANT.

**After.** The same measurement gives 5.0x for a single packet, and a replay serves
**0**. Gossip still works: a modest WANT is answered in full, a different link has
its own budget, and the bucket refills over time.

**Behaviour change.** A link that asks for more than its share is served partially
and must ask again. That is what gossip already tolerates — an id not served now is
offered on the next round — so nothing is lost, only paced.

**Tests.** `a_want_cannot_be_used_as_an_amplifier`, `ordinary_gossip_still_works`.

---

## S-013

**Ten tables a peer can grow, none of them bounded.** High. `src/lib.rs` — `Node`.

**Root cause.** `seen`, `frags`, `peer_prekeys`, `peer_busy`, `peer_names`,
`manifests`, `acked`, `rpc_inbox`, `feed_inbox` had no cap, no timeout and no
eviction of any kind — zero prune operations between them. One systemic omission
rather than ten separate oversights.

**Exploit.** `frags` is the cheapest and worst: one fragment per distinct
`orig_id`, claiming a count it never satisfies, opens a `Fountain` holding real
chunk bytes forever. Unsigned fragments are `Src::None`, which the quota admits
*unconditionally*, so this costs the sender nothing at all. The peer tables need
valid signatures, which is not a bound — an Ed25519 keypair is nearly free, the
same lesson as S-006. `rpc_inbox` and `feed_inbox` are plain `Vec`s the
application is expected to drain; if it does not, a peer decides how much memory
the process uses.

**Reproduced.** 20,000 incomplete fountain sets and 20,000 dedup entries from
unsigned traffic; 3,000 peer records from minted identities; and **all of it still
held 10 million seconds later** — nothing collected anything at any point.

**Patch.** One routine, `enforce_bounds`, called once per ingest. Expiry is
time-based and does not need to be immediate, so it runs at most every
`SWEEP_INTERVAL_SECS` (60): dedup entries past their retain-until go, and fountain
sets idle beyond `PARTIAL_TIMEOUT_SECS` (300) are collected. Hard caps are checked
every ingest, since a flood crosses a cap in far less than a minute, but only cost
anything when a table is actually over: `MAX_SEEN` (65536), `MAX_PARTIAL_OBJECTS`
(256), `MAX_PEERS` (4096), `MAX_MANIFESTS` (1024), `MAX_ACKED` (8192), `MAX_INBOX`
(1024).

Victim choice is deliberate where it matters. Dedup evicts nearest-to-expiry
first, so ids most likely still in flight survive. Partial objects evict oldest,
least likely to still have chunks coming. Inboxes drop from the front. For the
peer and manifest tables any survivor set is equally correct, so those trim
arbitrarily — and not attacker-chosen, since `std`'s hasher is seeded per map.

**Behaviour change.** Under pressure the node forgets rather than growing. Every
eviction degrades a capability instead of breaking one: a forgotten peer
re-announces, a dropped partial object is re-fetched, an evicted dedup entry costs
one duplicate relay. Nothing here can make the node *wrong*, only forgetful.

**Tests.** `incomplete_fountain_sets_cannot_accumulate`,
`every_peer_grown_table_has_a_ceiling`,
`dedup_evicts_the_nearest_to_expiring_first`.

---

## S-014

**The declared MSRV was unbuildable, twice over.** Low, but the same shape as
S-010. `Cargo.toml`, `Cargo.lock`.

**Root cause.** A comment on the dependency pins read "compatible with Rust 1.75
(no edition2024)", and there was no `rust-version` field for anything to check.
Two independent breaks had accumulated behind that comment:

1. `Cargo.lock` was at **version 4**, which Cargo 1.75 cannot *parse at all* — it
   fails before resolution, so the pins never even get consulted.
2. `zeroize` was pinned to `=1.7.0`, but `zeroize` is a facade and the derive macro
   is a **separate crate**. `zeroize_derive` 1.5 moved to edition2024, which needs a
   far newer Cargo. The pin constrained the thing that was safe and missed the thing
   that broke.

**Impact.** Not an attack. It is `CONTINUITY.md`'s promise again — "a repo clone
plus a Rust toolchain rebuilds everything offline" — failing on exactly the machine
that promise is for: an old one, offline, with no way to upgrade Cargo.

**Reproduced.** `cargo +1.75 build` → `lock file version 4 was found, but this
version of Cargo does not understand this lock file`. With the lock at v3 →
`feature edition2024 is required`. Then three test-only uses of APIs newer than
1.75 (`iter_repeat_n` ×2, `is_multiple_of`) and one `Option::is_none_or` in
`bridge::udp`.

**Patch.** Made 1.75 real rather than aspirational: `zeroize_derive` pinned to
`=1.4.2`, lockfile regenerated at version 3, `is_none_or`/`repeat_n`/
`is_multiple_of` replaced with equivalents available in 1.75, and
`rust-version = "1.75"` declared so Cargo itself enforces it. **132 tests pass on
`cargo +1.75`**, and stable is unaffected.

**Regression test.** `.github/workflows/msrv.yml` reads `rust-version` from
`Cargo.toml`, installs exactly that toolchain, runs the suite, and checks the
lockfile is still parseable by it. A separate workflow rather than a job in
`ci.yml`, which the freeze guard's regex covers. Without this the claim simply rots
again — it had already rotted twice.

---

## S-015

**The same unbounded read, in every file-backed bridge nobody had audited.**
Medium. `src/store.rs` (core adoption), `src/bridge/store.rs` (×2),
`src/bridge/foldersync.rs`, `src/bridge/ssb.rs`.

**Root cause.** Five `fs::read` / `read_to_string` calls with no size limit, on
directories that another program writes to *by design* — that is what a synced
folder, an SSB log and a spill directory are. Two of them, including the core
store's adoption path, had a `metadata` size check immediately before the read:
the same stat-then-read gap as S-007, with the same comment explaining why the
stat was enough. It is not; they are separate syscalls and the file can change
between them.

**Why the earlier audit missed it.** #15 fixed "spill adoption in `store.rs`" —
and it did, in `src/store.rs`, with `MAX_ADOPT_BYTES`. But that fix only capped the
*stat*, and there is a second `store.rs` one directory down (`src/bridge/store.rs`,
the folder bridge) that was never looked at. Two files with the same name, one
audited and one not. Worth recording as a lesson about how the gap happened rather
than only what it was.

**Exploit.** Drop one large file into any watched directory. Not remote in the
network sense — it needs write access to the folder — but a synced folder is
routinely shared with a whole cloud account or a whole LAN, which is the premise
of those bridges.

**Patch.** One shared `store::read_capped(path, max)` that bounds the **read**,
used by all five sites plus the spool bridge, whose private near-duplicate was
deleted. A stat may still fast-reject; it is no longer mistaken for the bound.

**Tests.** `read_capped_bounds_the_read_not_a_preceding_stat`, plus the existing
spool test now exercising the shared helper.

---

## S-016

**Imported-filename sets grew forever.** Low. `src/bridge/store.rs`,
`src/bridge/ssb.rs`, `src/bridge/copyparty.rs`.

**Root cause.** Each folder-style bridge keeps `HashSet<String>` of filenames it
has already imported, to avoid re-reading them. One entry per filename ever seen,
never forgotten, in a directory whose contents someone else controls — including a
rotating log, which grows it without any adversary at all.

**Patch.** `store::bound_known` clears the set past `MAX_KNOWN_FILENAMES` (4096).
Clearing wholesale rather than evicting one entry, because the set has no ordering
worth preserving. Forgetting is cheap here in a way worth stating: the node's own
dedup (`seen`) drops a re-imported envelope, so an overflow costs one wasted file
read, not a duplicate delivery.

**Test.** `known_filename_sets_stay_bounded`.

---

## S-017

**The ratchet's skipped-key cache had no total bound.** Medium (needs an
established session). `src/ratchet.rs` — `skip`.

**Root cause.** `MAX_SKIP` (512) bounds a *single* out-of-order gap, and that is
the bound everyone sees. But skipped message keys are stored under `(dh_pub, n)`,
and a DH ratchet step installs a new `dh_pub` **and resets `nr` to zero** — so each
step opens a fresh 512-key window in a different part of the keyspace. Nothing
consumed the old windows except a message that actually arrived to claim them, and
nothing pruned them. Two different quantities, one bound.

**Exploit.** A session partner ratchets repeatedly, each time declaring a wide gap
it never fills. Every step leaves up to 512 unclaimed 32-byte keys resident. It
takes an established session to reach — this is a peer you already agreed to talk
to, not anyone on the medium — which is why it is Medium rather than High. A
partner still should not get to decide how much memory you spend.

**Reproduced.** 40 ratchet steps each leaving a 400-key gap, with the cache
measured after every step.

**Patch.** `MAX_SKIPPED_KEYS` (4 × `MAX_SKIP`) caps the total; `bound_skipped`
trims past it. Victims are arbitrary rather than oldest-first, because the map is
keyed by `(dh_pub, n)` with no recoverable ordering across chains and every entry is
equally a key for a message that may never come — and `std`'s per-map hasher seed
means a peer cannot steer which of its own gaps survive.

Losing a skipped key costs exactly what packet loss costs: an out-of-order message
that no longer opens. The ratchet exists to survive gaps, so the protocol already
tolerates this.

**Tests.** `the_skipped_key_cache_cannot_grow_without_bound`,
`an_absurd_gap_is_still_refused_outright` (the per-gap guard must keep working too).

---

## S-018

**One panic under the hub mutex killed every bridge thread, permanently.** Medium.
`src/bridge/hub.rs`.

**Root cause.** Thirteen `lock().unwrap()` calls across `node`, `out` and
`deliver`. A panic while any of them is held poisons the mutex, and `unwrap` then
panics in *every* thread that touches it afterwards. One fault anywhere under the
lock therefore took the whole node down and kept it down — the same denial of
service this register spent its length removing, arriving through a different door.
The panic need not even be ours: `with_node` runs arbitrary embedder code under
that lock.

**Reproduced.** Poisoned `node` from a helper thread, then confirmed
`Mutex::is_poisoned` and that every subsequent hub call panicked.

**Patch.** One `lock()` helper recovering with `PoisonError::into_inner`. That is
the right trade *for what this protects*: `Node` is a router state machine whose
every table is independently bounded and self-healing — dedup expires, quotas
refill, partial objects time out — so the worst a half-applied `on_rx` leaves is
one duplicate relay or one dropped envelope. Continuing degraded beats dying. The
reasoning is deliberately not generalised in the doc comment: a lock guarding an
invariant other code *depends* on should still poison.

**Behaviour change.** A poisoned lock is now used rather than fatal.

**Test.** `a_poisoned_lock_does_not_kill_every_other_thread`.

---

## S-019

**Meshtastic length varints overflowed the offset, twice.** Medium.
`src/bridge/meshtastic.rs` — `decode`.

**Root cause.** `let end = no + len as usize`, where `len` is a varint read off the
air and so reaches `u64::MAX`. Plain addition overflows. `decode` runs **two**
protobuf loops — the frame, then the decoded sub-message — and both had it, along
with the `o += 4` / `o += 8` fixed-width skips in each.

The sibling parser `from_radio_packet`, 180 lines below, already used
`checked_add` throughout. The same author had already met this hazard and fixed it
in one function; `decode` was simply never revisited. Same shape as S-015, where
the cap existed in `src/store.rs` and not in `src/bridge/store.rs`.

**Exploit.** Anyone with a transmitter in range, one frame, no key. Severity turns
on the build profile, and the honest answer is worth stating precisely rather than
rounding up:

| Profile | Behaviour |
|---|---|
| overflow checks **on** — every `cargo build` / `cargo run` without `--release` | **panic**, node dies |
| overflow checks **off** — this repo's release profile | wraps to a bogus range; here it happened to fail the following bound check and return `None` |

So it is not a remote panic against a release daemon, which is why this is Medium
rather than High. It *is* a remote panic against anything built the default way —
and the daemon demo in the README is `cargo run`. The release behaviour is also
luck rather than design: wrapping arithmetic feeding a slice range is a latent
correctness hazard, not a safe outcome.

**Reproduced.** Found by the new `radio_codecs` fuzz target within 90 seconds of it
existing. The second instance surfaced only after the first was fixed, because
reaching the inner loop requires a well-formed field 4 wrapping a hostile inner
message.

**Patch.** `checked_add` in both loops and on both fixed-width skips, matching
`from_radio_packet`. Verified afterwards with **10.3 million fuzz executions, no
crashes**, and the JS codec parity test still passes.

**Tests.** `a_meshtastic_length_varint_cannot_overflow_the_offset` — the exact
fuzzer input, the general shape across wire types, and a nested sub-message for the
inner loop.

---

## S-020

**The encrypted-topic ratchet had no post-compromise security.** Medium (design).
`src/topic.rs` — `rotate`, and everything built on it.

**Root cause.** `rotate(k) = SHA-256(k ‖ domain)` is a symmetric hash chain. It was
documented as the forward-secrecy mechanism, which it is, and *used* as though it
were the whole key schedule, which it is not. Anyone who obtains one group key
computes every subsequent key with a hash, so rotating faster does not help — the
attacker rotates alongside. Recovery required a human to notice the compromise and
run `rekey_seal` to every member. Security that depends on detecting a silent theft
is not security you can plan around.

**Exploit.** Not remote and not a protocol break: it needs the group key. That key
is the one piece of a SPORE group that gets copied — backed up, synced between a
phone and a laptop, left on a device that was retired, handed to a member who later
leaves. The realistic case is a key that leaked at some point in the past and a
group that has no way to recover without knowing that happened.

**Reproduced.** As a test rather than a run: `a_contribution_locks_out_an_attacker_holding_the_key`
walks the chain ten rotations with the attacker in lockstep and asserts it still
reads the traffic — the property, stated as an assertion, before the fix changes it.

**Patch.** Contributory rotation. `contribute` draws 32 random bytes, seals a copy
to each member's prekey, and `absorb` folds it in with
`mix(k, c) = SHA-256("spore-topic-mix-v1" ‖ k ‖ c)`. The attacker holds the chain
but not the contribution, so the group heals through ordinary use with nobody
detecting anything. Three choices in that sentence are load-bearing:

- **Mixing rather than replacing.** The new key depends on the old one *and* the new
  entropy, so a contribution can only add. An attacker who can sign as a member
  cannot cancel an honest contribution by following it with one of its own.
- **No recipient hints.** Boxes are a uniform 80 bytes and unlabelled, so the
  message does not enumerate the group to an interceptor. The cost is trial
  decryption, hence `MAX_MEMBERS = 256`, so a forged message cannot become a CPU
  sink — the `absorb` count field is attacker-chosen.
- **`key_id`.** Four bytes of `SHA-256(domain ‖ key)` carried in the clear, so a
  receiver holding several candidate keys picks the right one. This does not give
  the group agreement; it makes disagreement readable instead of a silent failure.

**Behaviour change?** No. Additive: `rotate`, `epoch_key`, `seal`, `open`,
`rekey_seal`, `rekey_open` are byte-for-byte unchanged, and nothing in the frozen
wire format is touched — a contribution is application payload inside an ordinary
signed envelope.

**Freeze impact?** `tests/api_freeze.rs` gains pins for the four new functions and
golden hex for the `mix` and `key_id` derivations, so the schedule cannot drift and
leave two releases unable to converge on a key. Under `allow-frozen-change`.

**Tests.** Five in `src/topic.rs`: the lock-out above; that an injected contribution
cannot undo an honest one; that `mix` needs both halves and is not symmetric; that
`absorb` rejects every truncation, an absurd count, a count/body mismatch and every
single-byte corruption without panicking; and that `contribute` caps the member
count. `absorb`, `topic::open`, `peek_epoch` and `rekey_open` added to
`src/robustness.rs` and to the `seal_open` fuzz target — 61,737 executions, no
crashes, and libFuzzer's discovered dictionary contains the `\001\000` contribution
header, which is how we know it reached the parser rather than bouncing off it.

**Still not healed.** A stolen *prekey secret* opens every contribution addressed to
that member, forever. Fixing that needs the prekey to rotate, which is §7's daily
rotation and a separate piece of work. Recorded in "Still open".

---

## S-021

**The Android release looked abandoned, advertised a dead download, and named
itself after its own tag.** Low (release integrity / availability).
`.github/workflows/android.yml`, `docs/APPS.md`.

**Root cause.** Three independent defects that compounded into "there is no recent
build", when in fact every merge had built and published one:

1. **`published_at` never moves.** The workflow reuses the moving `rolling` tag and
   updates the release in place. GitHub's release list shows `published_at`, which
   is fixed at first publish, so three days of successful builds still displayed
   *25 Jul*. `updated_at` was current; nothing a visitor sees uses it.
2. **Assets accumulated.** Each run uploaded `spore-<version>.apk`, so the release
   held four APKs from four days plus a stray `spore-v0.0+…apk`, with nothing
   indicating which was current — and no stable filename, so no page could offer a
   direct download link at all.
3. **The version string was self-referential.** `git describe --tags --abbrev=0`
   found the `rolling` tag *this workflow creates*, so after the first run the
   "current version" became the literal string `rolling`, producing releases titled
   `SPORE rolling rolling+2026.07.27`.

Separately, `docs/APPS.md` advertised a "stable release" linking to
`/releases/latest`. No `v*` tag has ever been cut and the only release is a
pre-release, so that link resolved to nothing.

**Exploit.** No attacker required; this is availability and trust. A visitor
concludes the project is dormant, or downloads a three-day-old APK believing it is
current. The documented `sha256sum -c` verification also could not be followed for
the newest build, because only one day's checksum file survived.

**Reproduced.** `GET /repos/sloev/spore/releases/tags/rolling`: `published_at`
`2026-07-25T14:11:20Z`, `updated_at` `2026-07-27T07:30:38Z`, five assets dated
across three days, name `SPORE rolling rolling+2026.07.27`. Zero tags in the repo.

**Patch.** `--match 'v*'` on `git describe`, so the workflow cannot mistake its own
tag for a version. The rolling release is deleted and recreated each build, so its
date is the build's date and it holds exactly one APK. Both releases publish a
constant `spore-android.apk` (plus `.sha256`) alongside the versioned archive copy,
which is what makes a permanent download URL possible. `docs/APPS.md` now leads with
that direct link and states plainly that there is no stable release yet, instead of
linking to one that does not exist.

**Behaviour change?** Release layout only. No code, no wire format.

**Freeze impact?** None — `android.yml` is not in the freeze set.

**Tests.** None automated; this is release plumbing, verified against the API after
the next merge builds. Worth noting as the weakest verification in this register.

---

## S-022

**The one-shot seal had no forward secrecy, and four places said it did.** Medium
(false security claim). Fixed: the prekey ring below. `docs/SPEC.md` §7, `src/seal.rs` module docs and its §7
banner, versus `src/lib.rs` `Node::from_seed`.

**Root cause.** §7 read: *"seal to the recipient's **newest** prekey … Rotate
prekeys daily; **delete private prekeys after 7 d** → seized devices cannot read
mail older than a week."* `src/seal.rs` repeated the conclusion twice. None of the
mechanism exists:

```rust
// Node::from_seed — src/lib.rs
let mut buf = seed.to_vec();
buf.extend_from_slice(b"spore/prekey/v1");
let pb: [u8; 32] = Sha256::digest(&buf).into();
let prekey_sec = crypto_box::SecretKey::from(pb);
```

One prekey, derived from the identity seed, for the life of the identity. `announce`
carries that same `prekey_pub` forever. There is no set of prekeys, no age, no
rotation function, no expiry — `grep -rn 'rotate_prekey\|prekey_age\|old_prekey'`
over `src/`, `web/`, `android/` and `bindings/` returns nothing.

And this is not merely unimplemented — as designed it is **unimplementable in the
form described**. The prekey is a pure function of the seed that `Node::seed`
persists so an identity can be restored. Deleting the private half achieves nothing,
because anyone holding the seed re-derives it. The stated conclusion is therefore
false in the strongest sense: seize a device, take the seed, and read *every* message
ever sealed to that node, week-old mail included.

This is the same class as S-014, where the MSRV floor was asserted in a comment and
false in two ways at once. A property claimed in prose and contradicted by the code
is worse than an absent property, because it is the one people plan around.

**Exploit.** Device seizure or a backup leak — the threat §7 explicitly claimed to
mitigate. Not remote, and not a break of the sealing itself, which is sound. What
fails is the time-bound: there is no window after which sealed mail becomes
unreadable.

**Reproduced.** By reading, not running: the derivation above, the single
`prekey_pub` field, `announce` at `src/lib.rs:760`, and the empty grep. Prompted by
being asked to justify the phrase "which §7 describes and nothing enforces" — which
was itself too generous, and is corrected here.

**Patch, in two steps.** First the claims were removed rather than softened, because
a false property is worse than an absent one. Then the property was built.

**The prekey ring.** `Node` holds up to `MAX_PREKEY_RING` (16) entries of
`{public, secret, born}`, oldest first, and advertises the newest.
`rotate_prekey` mints a **random** X25519 pair every `PREKEY_PERIOD_SECS` (24 h);
`sweep_prekeys` deletes any secret older than `PREKEY_LIFETIME_SECS` (7 d), never
the newest, so a node is always sealable. `Node::open` tries every live entry
newest-first, so a sender working from a stale ANNOUNCE still reaches us until that
secret expires. Rotation is driven from the router's periodic sweep
(`enforce_bounds`) rather than left to each embedder, because a forward-secrecy
property that every platform must remember to switch on is one most platforms will
not have.

Three things are load-bearing:

- **Random, not derived.** This is the entire fix. A seed-derived secret is
  permanent, so deleting it deletes nothing. `Node::seed()` still restores identity
  and the ability to mint *new* prekeys; it cannot resurrect a swept secret.
- **The nonce mixes the recipient's public key**, so a secret can only be tried
  against its own public half — hence entries store both, and
  `restore_prekey_ring` recomputes the public half rather than trusting the blob.
  A hostile blob must not make a node advertise a key it cannot open.
- **Persistence replaces, never merges.** The stored ring is the authority on which
  secrets exist, so a bootstrap key that has already aged out of it does not come
  back to life just because `from_seed` can still derive it.

**Behaviour change?** Yes, and it has a price worth stating: **`Node::seed()` is no
longer a complete backup.** A node restored from the seed alone keeps its address,
gets one bootstrap prekey, and cannot read mail sealed to anything it had rotated
to. `Node::prekey_ring()` / `restore_prekey_ring()` persist it; the browser node
(localStorage) and the Android app (SharedPreferences) both now do. Mail sealed to
an expired prekey is unreadable by everyone including the recipient — the feature,
not a defect. And a *backup* of the ring defeats the seven-day window exactly as it
would for any forward-secret keystore; `CONTINUITY.md` says so, because continuity
of identity and forward secrecy of content genuinely pull against each other here.

**Freeze impact?** None. The wire is untouched — ANNOUNCE carries one prekey and
always did; it is simply a different one each day. `prekey_pub` keeps its type and
meaning. Everything added is additive.

**Tests.** Six in `src/lib.rs`, and the one that matters is
`a_rotation_keeps_old_mail_readable_until_the_secret_expires`: seal, rotate, confirm
the old mail still opens, sweep past the lifetime, confirm it never opens again.
`a_seed_restore_does_not_resurrect_a_swept_prekey` is the old design's failure stated
as an assertion — restore from seed + ring and the mail reads; sweep, persist that,
restore again, and it stays dark; seed alone yields one bootstrap key and no access.
Plus: an ANNOUNCE moves a peer's seal target to the newest prekey; the ring is
bounded and sweeping is idempotent and never empties it; rotation happens from
ordinary traffic rather than being asked for; and `restore_prekey_ring` rejects every
truncation, an absurd count, a public/secret mismatch and every single-byte
corruption without panicking. `web/test.mjs` covers the same round-trip and rejection
across the wasm boundary, where a silent failure would cost a page reload's inbox.

---

## S-023

**The daemon beaconed a mesh-wide flood every 5 seconds instead of a link-local
HELLO every 5 minutes.** High (availability / regulatory).
`src/main.rs` beacon loop, `src/lib.rs` `build_announce`.

**Root cause.** Two mistakes stacked, and each hid the other.

1. **Wrong unit.** `Trickle::new(now(), 5, 80)`, where `now()` is
   `SystemTime::now()… .as_secs()` — wall-clock **seconds**. The spec says the
   HELLO interval doubles **5 → 80 minutes**. Someone wrote the spec's numbers
   into a timer whose base is seconds, so the interval was 5 → 80 s: **60× too
   fast**.
2. **Wrong frame.** §4 has said from the start that there are two forms —
   *"Link HELLO = hops 0; flooded = hops 16."* Only the flooded one was ever
   built. `build_announce` unconditionally set `hops = 16` and `fl::FLOOD`, and
   `Hub::beacon` was the only beacon call, so the frequent beacon was the
   mesh-wide one.

Net effect: roughly **45 mesh-wide ANNOUNCE floods per hour per node** at the
steady-state 80 s interval — and ~720/h during the 5 s phase, which `Trickle::reset`
returns to on any novelty, so a busy mesh sits near the fast end. The documented
ceiling in the same spec line is **≤ 1/h**. Every flood is relayed by every node
that hears it, and each one was also `store_put`, so the store churned too.

**Exploit.** No attacker needed; the node does it to itself and to everyone in
range. It matters most on exactly the media SPORE is *for*:

| Medium | Consequence |
|---|---|
| LoRa (EU868) | A ~130-byte signed ANNOUNCE flooded every 5–80 s will exceed the **1 % legal duty cycle**. This is a compliance problem, not a tuning one. |
| AX.25 / packet | Continuous beacon traffic on a shared channel; antisocial at best. |
| Meshtastic | Each SPORE flood becomes N mesh hops in the underlay (§: "one SPORE hop, N invisible mesh hops"). |
| LAN | Harmless. Which is why nobody noticed. |

The §5.4a token bucket does **not** save you: it caps *relayed bulk* traffic, and
announces are deliberately exempt so a slow link stays a full mesh member.

**Reproduced.** By reading, then pinned as a test. `now()` at `src/bridge/hub.rs:18`
is seconds; `Trickle::fired` sets `fire_at = now + cur`; the beacon loop polled every
500 ms. The 60× and the 45/h both follow arithmetically.

**Patch.** `Node::build_hello` builds §4's link-local form — same signed payload,
`hops = 0`, so §5 rule 5 stops it at the first hop. `Hub::hello` sends it. The
daemon now runs two cadences: HELLO on Trickle between `HELLO_MIN_SECS` (300) and
`HELLO_MAX_SECS` (4800), and the flooded ANNOUNCE no more often than
`ANNOUNCE_FLOOD_MIN_SECS` (3600). The constants are named and carry their unit,
because the bug was a bare number in the wrong unit. A HELLO is no longer stored: it
is link-local and superseded by the next one.

**Behaviour change?** Yes — local policy, not wire. Both frames are ordinary v1
ANNOUNCEs; a hops-0 ANNOUNCE is what §4 always specified and what §5 already handles.
Peers need no change. Neighbour discovery on a LAN is slower to *first* contact (up
to 5 minutes rather than 5 seconds) — that is the spec's chosen trade, and INV/WANT
on first contact still backfills immediately.

**Freeze impact?** None. `build_announce` keeps its signature; `build_hello` and the
three constants are additive.

**Tests.** `hello_is_link_local_and_the_beacon_cadence_is_in_seconds` — the constants
are the spec's minutes expressed in seconds; a HELLO decodes with `hops == 0`, still
verifies, teaches a neighbour our prekey, and produces **no** forwards from that
neighbour; the flooded form has `hops == 16` and *is* relayed.

---

## S-024

**Two smaller divergences found in the same sweep**, recorded rather than fixed,
because both are documentation-versus-code mismatches with no exploit and fixing
them properly is a design choice rather than a repair. Low.

**(a) The ratchet's skipped-key cache has no age bound.** `docs/SPEC.md` §7 said
*"cache skipped mks ≤ 7 d"*. `src/ratchet.rs` contains no notion of time at all —
no timestamps, no `now` parameter. The cache is bounded by **count**
(`MAX_SKIPPED_KEYS`, from S-017), which is the right defence against the DoS but a
different property: a skipped message key can be held indefinitely, so the
forward-secrecy window for out-of-order messages does not decay after a week as the
spec claimed. Nothing is zeroised on drop either — `zeroize` is a direct dependency
but, as its own `Cargo.toml` comment says, only to pin a transitive version; no code
uses it. §7 now states what the implementation does. Giving it the 7-day property
means threading time into `Ratchet`, which touches its whole API.

**Fixed (S-024a), PR0.** `decrypt` and the internal `skip` now take `now`, each
skipped key is banked with its `inserted_at`, and `purge_skipped` drops (and
zeroizes) any older than `SKIP_TTL_SECS` — defined as `crate::PREKEY_LIFETIME_SECS`
so the session layer and the seal layer promise the *same* seven days rather than
drifting. `Ratchet` and `SkippedKey` gained `Drop` impls that zeroize the root key,
both chain keys, the DH secret, and every cached message key. Two tests pin the
behaviour (`skipped_keys_expire_after_ttl`, `skipped_keys_live_inside_ttl`); the
existing count-bound and absurd-gap tests still hold. The ratchet is not in the
frozen API surface, so this touched no frozen file. **Field-verification** of the
window end-to-end on a device remains open and is tracked for PR6 — the unit tests
prove the deadline logic, not that a real node's clock and delivery behave.

**(b) `mark_seen` ignores the 30-day dedup floor.** The spec's defaults line
promises *"seen-set ≥ 30 d"*. `ingest` honours it —
`e.expiry.max(now + SEEN_MIN_SECS)`. Its twin `mark_seen`, used by all fifteen
*origination* paths, computes `e.expiry.max(0u32.wrapping_add(SEEN_MIN_SECS))` — no
`now`, because the method does not take one. Since `SEEN_MIN_SECS` is 2 592 000 and
any real expiry is a Unix timestamp far larger, the `max` is a **no-op** and the
`// >= expiry` comment describes nothing. Not exploitable: the id is retained until
the envelope expires, and `ingest` independently drops anything with
`e.expiry < now`, so a forgotten id cannot be re-accepted. It is the same
fixed-in-one-twin-not-the-other shape as S-015 and S-019, and it is a misleading
expression sitting in the dedup path, which is worth knowing about.

---

## S-025

**A capital letter published a release that served nothing.** Low (release
integrity). `.github/workflows/android.yml`.

**Root cause.** The workflow triggered on `tags: ['v*']`. GitHub Actions glob
matching is **case-sensitive**, so `V0.1.0` and `V0.2.0` did not match it. Both tags
were created, GitHub created a release for each, and no build ever ran.

`V0.2.0` is not a pre-release, so it became **"latest" while holding zero assets**.
That makes `releases/latest/download/spore-android.apk` — the permanent stable link
`docs/APPS.md` advertises, added in S-021 — return 404 from a page that looks like a
finished release. Strictly worse than the state S-021 fixed, where the link 404'd
because no non-prerelease existed at all: a missing release reads as "not out yet",
an empty one reads as "broken download".

A second mismatch rode along. The tag said `0.2.0` while `Cargo.toml` said `0.1.0`,
so rolling builds would have been named `0.1.<stamp>` while the newest release was
`0.2.0`, drifting apart with nothing checking.

**Exploit.** None; availability and trust again. But note the shape, because it is
the third of its kind in this register: the mechanism was verified once, by hand,
against a case that happened to work (`rolling` builds, which are branch-triggered),
and the untested neighbouring path was assumed to work. S-015, S-019 and S-023 were
all "fixed in one twin, missed in the other". This is the same error with `v` and `V`
as the twins.

**Reproduced.** `git ls-remote --tags` shows `V0.1.0` and `V0.2.0`; the release API
reports `"assets": []` for `V0.2.0` with `prerelease: false`; and
`curl -o /dev/null -w '%{http_code}'` on the stable link returns **404** while the
rolling link returns 206.

**Patch.** `tags: ['v*', 'V*']`. The tagged path now also derives the tag's
`major.minor` and **fails the build** if it disagrees with `Cargo.toml`, so the two
cannot drift silently. `Cargo.toml` bumped to `0.2.0` to match the tag that exists.
The tagged release step gained `append_body: true`, so re-running a release build
adds its caveats under GitHub's generated notes instead of overwriting them.

`docs/CONTRIBUTING.md` gains the step this needed: bump `Cargo.toml` before tagging, and
**confirm the release has assets** with `curl -fsI` before announcing it. A tag whose
build never ran still produces a release page.

**Behaviour change?** Release plumbing only.

**Freeze impact?** None.

**Tests.** None automated — this is workflow-trigger behaviour, only observable by
tagging. The `curl -fsI` step in the release checklist is the manual check that would
have caught it, and its absence is why this shipped.

---

## S-026

**The accumulation bug I fixed for `rolling` I reintroduced for nightlies.** Low
(release integrity). `.github/workflows/android.yml`. **Self-inflicted, in the same
change that fixed S-021.**

**Root cause.** S-021's accumulation fix was to delete and recreate the `rolling`
release each build. The dated `nightly-<date>` release added at the same time was
left to update in place, and it carries the *versioned* filename — which now embeds a
minute-level stamp and the commit sha. So a second merge on the same day uploads a
new name instead of overwriting one, and the day's release grows.

Both halves of S-021 came straight back, one level up:

- **Assets accumulate.** After two merges on 2026-07-27 the nightly held four:
  `spore-v0.0+2026.07.27.apk` next to `spore-0.1.202607270951+7b2a185.apk`, plus both
  checksums, with nothing indicating which was current.
- **The date lies.** `published_at` was `08:44:28` while `updated_at` was `09:53:07`
  and the contents were the 09:53 build — the same in-place-update trap S-021 was
  about.

**Exploit.** None; availability and trust. What it cost was a claim: the release body
said "kept as one of the last five nightlies so a bad build can be rolled back from",
and rolling back needs to know which file is which.

**Reproduced.** The release API for `nightly-2026.07.27`: four assets, two timestamps
an hour apart, `published_at` an hour behind `updated_at`.

**Patch.** Delete today's nightly before recreating it, exactly as `rolling` is
handled. One day, one build — the day's *last* master — with a `published_at` that is
the build's own time. The five-day retention window is unaffected.

**Why it is worth a register entry.** Because the failure was not the bug, it was the
reasoning: the fix was verified on the artefact it was written for and assumed on its
neighbour. That is S-015, S-019, S-023 and S-025 as well — four earlier entries with
the same shape, and it still happened while writing the entry that described it.
Verifying the mechanism is not the same as verifying every artefact the mechanism
produces.

**Behaviour change?** Release layout only.

**Freeze impact?** None.

**Tests.** None automated. Same weakness as S-021 and S-025: release plumbing is only
observable by releasing, and the `curl -fsI` step in `docs/CONTRIBUTING.md` is the only
guard. It would not have caught this one, because the link it checks kept working —
what broke was which of four files the link's neighbours were.

---

## S-027

**One hostile filename stopped a whole folder from syncing, permanently.** Medium.
`src/bridge/foldersync.rs` — `materialize`.

**Root cause.** A file manifest's name is `String::from_utf8_lossy` of bytes chosen
by whoever published it (`src/file.rs:183`). **NUL is valid UTF-8**, so it survives
decoding untouched. The traversal guard is sound —

```rust
let safe = Path::new(&name).file_name().map(|s| s.to_owned());
```

— `file_name()` strips every directory component, so `"../../etc/passwd"` becomes
`"passwd"` *inside* `out_dir` and `"."`/`".."` are dropped. That part was right. The
bug was the next line:

```rust
fs::write(out_dir.join(fname), bytes)?;   // <-- `?`
```

`file_name()` happily returns `"a\0b"`, the OS refuses it, and `?` propagates the
error out of the loop.

**Exploit.** Publish one file manifest whose name contains a NUL onto a topic the
victim follows. Manifests are ordinary flooded envelopes — no key, no session, no
forgery. From then on every call to `materialize` returns `Err` and **not one file
is written**, including files that were already complete and perfectly writable.
It persists until the poisoned manifest expires out of the store, and can be
renewed indefinitely for the cost of one small envelope. The folder-sync bridge is
silently dead while looking merely idle.

**Reproduced.** First against `std` directly, to establish the primitive: of
`"../../etc/passwd"`, `".."`, `"/etc/passwd"`, `""`, `"."`, `"a\0b"`, `"x/../../y"`,
only `"a\0b"` survives `file_name()` and fails to write. Then end to end: a `Node`
with two NUL-named manifests and one honest `good.txt`, and `materialize` returning
`Err` with an empty output directory.

**Patch.** Skip what will not write instead of aborting the run. The same applies to
a permission error or a full disk on one path — none of those are reasons to stop
writing the other files.

**Behaviour change?** `materialize` now returns `Ok(n)` where it previously returned
`Err` for an unwritable name, and `n` counts what was actually written. A caller
that treated `Err` as "nothing happened" still sees a correct count.

**Freeze impact?** None.

**Tests.** `a_hostile_filename_cannot_stop_the_other_files_being_written` — two
poisoned names either side of an honest one, asserting the honest file lands and the
directory holds exactly one entry. `a_name_cannot_escape_the_output_directory` pins
the traversal property that *was* correct, so a future refactor of the guard cannot
quietly lose it.

---

## S-028

**`materialize` pulled the entire store into RAM.** Low.
`src/bridge/foldersync.rs` — `materialize`; `src/lib.rs` — `complete_files`.

**Root cause.** `complete_files()` returns `Vec<(String, Vec<u8>)>` — it assembles
*every* complete file into memory before a single one is written. The store is
deliberately disk-backed for exactly the opposite reason: the Android node is
configured for `STORE_BUDGET_BYTES = 256 MB` on disk with `MEM_BUDGET_BYTES = 8 MB`
resident, and this path defeats that split in one call.

**Exploit.** Weak, which is why this is Low: `materialize` is called by the operator
or their sync loop, not by the receive path, so a peer cannot trigger the allocation
directly. What a peer controls is the *size* — filling a follower's store with
complete files is ordinary use of the file layer.

**Reproduced.** By reading, not measuring — the allocation is plain in the type. Said
plainly because this register's rule is that findings are reproduced, and this one is
reasoned. It is listed because the fix is small and the disk-backed store's whole
purpose is undermined without it, not because it was demonstrated.

**Patch.** `Node::complete_file_names()` returns `(name, magnet)` pairs, and
`materialize` streams each file straight to its `File` handle with the existing
`write_file_to`, which is already generic over `io::Write`. One file is in flight at
a time. `complete_files()` is kept — it is public API and useful for small stores —
with its doc now pointing at the streaming path for anything going to disk.

**Behaviour change?** None observable; same files, same contents.

**Freeze impact?** None. `complete_files` is not in `api_freeze.rs` or the C ABI, and
is unchanged regardless.

**Tests.** Covered incidentally by the two S-027 tests, which now exercise the
streaming path. No test asserts the memory profile — a peak-RSS assertion would be
flaky, and pretending otherwise would be the kind of claim this register exists to
prevent.

---

## S-029

**My fix for S-026 destroyed the release it was meant to repair.** Low (release
integrity). `.github/workflows/android.yml`. **Self-inflicted, second time.**

**Root cause.** S-026's fix was `gh release delete <tag> --yes --cleanup-tag`
followed immediately by recreating the release and tag. On its first live run
(`27bea16`) every step reported success — the job is green, steps 18, 19 and 20 all
`success` — and the end state was `nightly-2026.07.27` **existing as a git tag with
no release attached to it**.

Deleting a tag and recreating it a second later asks GitHub to reconcile two
operations on the same ref, and tag deletion is not synchronous. I could not
attribute the exact interleaving from the job logs, and say so rather than invent
one: what is established is the end state, that it was not there before this change,
and that the pattern is racy by construction.

**Exploit.** None. The damage is that the rollback window S-026 existed to provide
was empty, and the job that emptied it reported success — the worst combination for
something nobody watches.

**Reproduced.** Observed, not constructed: `git ls-remote --tags` lists
`nightly-2026.07.27`, and `GET /releases/tags/nightly-2026.07.27` returns 404.

**Patch.** Stop deleting releases. Clear their **assets** instead —
`gh release delete-asset` over what `gh release view --json assets` lists — then
upload. The tag and the release identity are never touched, so there is no ref to
race on, and the accumulation S-026 was about is still prevented because the old
assets go before the new ones arrive. Applied to both `rolling` and the nightly.

The cost, which is the honest trade: an in-place update never moves `published_at`,
so the sidebar date lags again — the thing S-021 was originally about. The release
*name* carries the full version and the body carries the commit, so the build is
still identifiable. A stale sidebar date is a smaller lie than a release that
disappears, and unlike the delete/recreate approach it cannot lose the artefact.

**Behaviour change?** Release plumbing only.

**Freeze impact?** None.

**Tests.** None automated, and this is now the fourth release-plumbing finding with
no test (S-021, S-025, S-026, S-029). That is the pattern worth naming: this
subsystem is only observable by running it against GitHub, every fix here has been
verified by inspecting the artefacts afterwards, and twice that inspection found the
fix had broken something else. Nothing in CI will catch the fifth one either.

---

## S-030

**I reproduced S-025's bug inside the fix for S-025.** Low (release integrity).
`.github/workflows/android.yml` — the tag-cutting step. **Self-inflicted, third
time.**

**Root cause.** The step that cuts a tagged release when `Cargo.toml` names a new
version checked whether that release already existed:

```sh
tag="v${ver}"
if gh release view "$tag" >/dev/null 2>&1; then exit 0; fi
```

Git tags are case-sensitive, and so is `gh release view`. `Cargo.toml` said `0.3.0`
and a hand-cut **`V0.3.0`** already existed, so the lookup missed it and the step
published a *second* release for the same version.

S-025 was: a `v*` glob does not match a `V` tag. This is the identical mismatch, in
the remedy for it, written while that register entry was open. Three of the last
four findings have been my own fixes breaking something (S-026 → S-029 → S-030).

**Exploit.** None. The damage is a releases page carrying two entries for 0.3.0 —
`V0.3.0` empty, `v0.3.0` with the artefacts — which is confusing rather than
dangerous, and would recur on every future release cut against a hand-made capital
tag.

**Reproduced.** Predicted from reading the diff before the run finished, then
confirmed: `GET /releases` returns both `v0.3.0` (id 360447083, `github-actions`,
with assets) and `V0.3.0` (id 360429808, `sloev`, `"assets": []`).

**The upside, recorded because it is instructive.** `v0.3.0` is a non-prerelease
newer than `V0.3.0`, so it became "latest" — and
`releases/latest/download/spore-android.apk` returned **206** for the first time in
this project's history. The link `docs/APPS.md` has promised since S-021 now works.
A correct outcome produced by a broken mechanism is not a working mechanism, and is
the single most misleading state a release pipeline can be in: had nobody checked,
"the download works" would have been taken as evidence the code was right.

**Patch.** Check both `v${ver}` and `V${ver}` before creating. Two lines.

**Behaviour change?** Release plumbing only. The duplicate `V0.3.0` is left in place
— deleting a published release is the maintainer's call, not CI's.

**Freeze impact?** None.

**Tests.** None, and this is the fifth release-plumbing finding without one (S-021,
S-025, S-026, S-029, S-030). The pattern is now unambiguous: **every fix in this
subsystem has been verified only by inspecting the artefacts afterwards, and three
of those inspections found the fix had broken something else.** The subsystem needs
a way to be exercised without publishing, or it will keep producing findings at this
rate. That is the recommendation this entry exists to make.

---

## S-031

**Any noise in the room saturates a core, permanently.** High.
`src/bridge/audio.rs` — `Demod::push`.

**Root cause.** `push` restarted its scan at offset `0` across the entire retained
buffer on every call, so the work per call was proportional to the *buffer*, not to
the new samples. `consumed` only advances when a frame successfully decodes, so a
stream that never syncs never advances it and the buffer sits at its cap:
`(6 + 2*(2+4096+4)) * 1024` = **8,407,040 samples = 175 seconds**.

The S-013-era cap bounds *memory*. It was mistaken for bounding *work*.

**Exploit.** Play anything that does not decode — noise, music, speech, a silent mic
with dither — near a node running the audio bridge. No key, no session, no protocol
participation, no need to be a peer at all. Measured on this machine, feeding 100 ms
chunks:

| audio buffered | cost of one 100 ms push |
|---|---|
| 1.0 s | 13.2 ms |
| 3.0 s | 44.0 ms |
| 6.0 s | 93.9 ms ← real-time budget |

Linear, crossing the budget at ~6.4 s and settling near **2.7 s of CPU per 100 ms of
audio** at the cap — roughly 27× real time. The demod thread never catches up, and on
Android this runs in a foreground service fed by `AudioRecord`.

**Reproduced.** Measured, not reasoned: a harness feeding 100 ms noise chunks through
the real `Demod::push` and timing each call, producing the table above.

**Patch.** A `scanned` cursor. An offset that had a full sync window and did not
match cannot begin matching once *more* samples arrive after it, so it is never
retested. `push` resumes where it stopped, and the cursor is shifted down when the
buffer is drained.

The subtlety that makes this safe: when sync *does* match but the frame does not
decode, the body is usually still arriving, so the cursor **holds** at that offset
rather than advancing past it — otherwise every frame whose sync lands near the end
of a chunk would be dropped, which is most of them. It gives up only once the buffer
is longer than a maximal frame, which bounds the stall.

**After:** flat at **1.5 ms** per push regardless of buffer length — 63× faster at
6 s, and no longer growing.

**Behaviour change?** None observable. Same frames decoded; the pre-existing
round-trip, chunked-streaming and back-to-back-frame tests all still pass unchanged.

**Freeze impact?** None. `Demod` gains a private field; no public signature moves.

**Tests.** `a_frame_split_across_many_pushes_still_decodes` across four chunk sizes
(64, 257, 1024, 4096) with lead-in silence — the case the cursor could plausibly
break. `two_frames_in_a_stream_both_decode`, so the cursor survives a consume and a
drain. `scanning_does_not_redo_work_as_the_buffer_fills` asserts the cost stays flat
as noise fills the buffer, which is the property itself rather than a proxy for it.

---

## S-032

**Anyone can forge "Delivered ✓".** Medium-High. `src/node/ingest.rs` — the §8
receipt branch of `ingest`.

**Root cause.** The branch that consumes a delivery receipt checked only the
*shape* of what arrived — `typ == DATA`, dest is one of mine, `ACKREQ` clear,
payload starts `[0x06]` and is ≥ 17 bytes — and then recorded the referenced id
as delivered. It never consulted `verified_src`, which the same function had
already computed twelve lines earlier, and it never required the `SIGNED` flag at
all. The `Pending` entry did not even retain the address the message was sent to,
so there was nothing to compare a receipt against.

The mistaken assumption is that knowing the original envelope's id implies having
received it. It does not: **`ID` is not a secret**. It travels in the clear inside
every `INV` (§6), which is unsigned, exchanged on every meeting, and lists exactly
the ids a peer is carrying. Any node that relayed the envelope, or merely heard an
`INV` naming it, can name it back.

**Exploit.** Alice sends Bob a message with `ACKREQ`. Mallory — any relay on the
path, or any peer that saw an `INV` — sends Alice one `DATA` envelope addressed to
Alice with payload `[0x06][orig_id]`. Alice records the message as delivered.
Bob never received anything. Two variants both work: a receipt Mallory *signs with
her own key*, and a receipt with **no signature at all** (an unsigned envelope
carries no `src` and "rides last everywhere", but the branch never looked). No key,
no session, no position on the path beyond hearing one `INV` — one packet.

The damage is a false security claim rather than disclosure, which is what keeps
this below the spoofing entries: nothing decrypts and nothing is impersonated at
the content layer. But "delivered" is precisely a claim [Mission](MISSION.md)'s
honesty contract forbids getting wrong, and a store-and-forward system whose whole
premise is *eventual* delivery is one where the receipt is the only evidence a user
ever gets. A forged one is indistinguishable from the real thing, and it
suppresses the §5.4d resend that would otherwise have kept trying.

**Reproduced.** A test driving the real `Node::on_rx`: Mallory meets Alice, Alice
sends Bob an `ACKREQ` message that Bob never receives, Mallory returns a forged
receipt. `a.acked(&id)` was `true` for both the stranger-signed and the wholly
unsigned variant.

**Patch.** `Pending` gains a `dest` field recording who the message was addressed
to, and the receipt branch requires `verified_src` — `Some` only for a signature
that checks out — to equal that address. An unsigned receipt has no
`verified_src` and is rejected; a stranger's signature verifies but does not match
the destination and is rejected.

**Freeze impact?** None. `Pending` is a private struct, the check is local state,
and no envelope byte changes — a receipt on the wire is exactly what it was.
`tests/api_freeze.rs` passes untouched.

**Behaviour change.** Yes, and deliberately: a receipt that would previously have
been accepted from anyone now counts only from the destination. A node that was
relying on a third party to close out its `pending` entry will now keep resending
per §5.4d until the real recipient answers — which is the correct behaviour.

**Test.** `a_receipt_only_counts_from_the_address_it_was_sent_to` — asserts both
forgeries are refused *and* that the genuine recipient's receipt is still accepted,
so the fix cannot be satisfied by simply breaking receipts.

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
| `tor` SOCKS5 reply allocates from a wire byte | Bounded: the length is a `u8`, so `vec![0u8; skip + 2]` is at most 257 bytes. |
| `udp` reads `/proc/net/route` unbounded | A kernel file, not attacker-controlled. |
| `meshtastic` / `ssb` base64 `with_capacity` | Sized from our own buffer's length, not from a wire-declared length. |
| Hub lock **ordering** (`node` vs `out`) | No deadlock. Every method needing both scopes the `node` guard in a block and releases it before `dispatch` takes `out`, and `dispatch` never takes `node`. The ordering is deliberate; `with_node`'s doc now records that it is load-bearing. |
| `mix::pad_to_class` on oversized payloads | Bounded and panic-free: rounds up to a whole multiple of the top size class. |
| Ratchet per-gap skip limit | `MAX_SKIP` works as intended and still refuses an absurd jump; the gap it missed was the *total* across chains (S-017). |

## Still open

Carried deliberately, not overlooked.

- **Ratchet, mix and lock ordering** are unreviewed: deeper state machines, but
  less "anyone on the medium can crash you" than the items above.
- ~~**The ratchet's skipped-key cache is count-bounded, not age-bounded** (S-024a).~~
  **Fixed in PR0:** the cache is now age-bounded to seven days
  (`SKIP_TTL_SECS = PREKEY_LIFETIME_SECS`) and both `Ratchet` and its skipped keys
  zeroize on drop. Field-verification of the window on hardware is still open (PR6).
- **`mark_seen` vs `ingest`** disagree about the 30-day dedup floor (S-024b). Not
  exploitable, but one of them is doing nothing and says otherwise.
- **Beacon cadence is fixed but unmeasured on radio.** S-023 makes the daemon obey
  the spec's numbers; whether 5→80 min HELLO plus hourly flood actually fits a LoRa
  duty cycle in practice is a question for the first hardware run, not for a
  calculation.
- **S-020's healing now survives a stolen prekey secret for at most seven days**,
  since contributions are sealed to prekeys and those expire (S-022). Within the
  window a stolen prekey secret is still a decryption oracle for every contribution
  addressed to that member. Shortening the window is a knob, not a fix.
- **No platform has verified the seven-day window end to end.** The unit tests prove
  the ring deletes and that a restore cannot resurrect; nobody has yet run a phone
  for eight days, seized it, and confirmed week-old mail is unreadable. Until
  someone has, this is "implemented and tested", not "field-verified".
- **Encrypted groups have no roster.** Membership, and therefore who a contribution
  is sealed to, is entirely the application's problem. In a partition two halves can
  diverge onto different keys; `topic::key_id` makes that legible but does not
  resolve it. Solving it properly is distributed agreement, not cryptography, and it
  is the largest honest gap between a SPORE group and a messenger's.
- **`with_node` reentrancy** is documented but not enforced. An embedder whose
  closure calls back into the hub self-deadlocks; a re-entrant guard or a `&Node`
  variant would make that unrepresentable.
- **Hardware-verified status** for every 🧪 bridge. No marketing claim until the
  `HARDWARE.md` procedure has actually been run.
