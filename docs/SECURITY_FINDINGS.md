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
- **`meshtastic` and `audio` codecs** are read but not yet fuzzed as thoroughly as
  the core parsers; both are reachable from a hostile medium.
- **`with_node` reentrancy** is documented but not enforced. An embedder whose
  closure calls back into the hub self-deadlocks; a re-entrant guard or a `&Node`
  variant would make that unrepresentable.
- **Hardware-verified status** for every 🧪 bridge. No marketing claim until the
  `HARDWARE.md` procedure has actually been run.
