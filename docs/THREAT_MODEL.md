# Threat model

SPEC §0 states the whole thing in one sentence: **every link is hostile —
logged, spoofed, jammed, MITM'd. Links are trusted with nothing; authenticity
and secrecy live only in the envelope.** This document is that sentence
expanded into something checkable: six chapters of adversary, what they can
try, what stops them, and — where nothing fully does — a stated residual risk
rather than silence.

**The rule every entry follows:** name the attacker a mechanism defeats, and
give every attacker an explicit residual-risk line. A mitigation with no
named attacker is a slogan. An attacker with no residual-risk line is a claim
of perfect security, which this project's Honesty contract
([`MISSION.md`](MISSION.md)) does not permit.

Every claim below links to a [`SPEC.md`](SPEC.md) section or a
[`SECURITY_FINDINGS.md`](SECURITY_FINDINGS.md) ID. Where the honest answer is
"partially" or "not at all," that is what it says — this is a document about
where the edges are, not a marketing page.

**Why this exists now.** An external threat-model catalogue, written against
SPORE's public site rather than its spec, raised questions this project had
already answered in code — and one it had not
([S-032](SECURITY_FINDINGS.md#s-032), found by checking the catalogue's
"receipt spoofing" item against `src/node/ingest.rs`). That is the argument
for this document existing: the answers were real but undiscoverable, and in
one case checking the question against the code found a live bug.

---

## 1. Observers — passive, local, global

**Asset:** message content, sender/recipient identity, timing, location,
social graph.

| Level | Capability | What SPORE does |
|---|---|---|
| Passive link observer | Receive and record traffic on one medium | Content: sealed (§7) or ratcheted; unreadable without the key |
| Local traffic analyst | Correlate timing/size/frequency on one medium | Partially addressed — see residual risk |
| Global passive adversary | Observe a large fraction of the network at once | Addressed only while mix decoys flow (§9) |

**Mitigation.** Content confidentiality is the baseline: `seal`/ratchet (§7)
for 1:1, XChaCha20-Poly1305 pre-shared-key sealing with healing rotation (§7.1)
for private groups. Open-group and microblog traffic is plaintext **on
purpose** — MISSION.md states it outright: "open group posts are public on
purpose," which is a design choice, not an oversight, because a stranger's
device has to be able to carry it forward without holding a key.

For metadata, §9's mix mode is the actual answer: onion-wrapped envelopes
through 2–3 mixes, Poisson-delayed batching (1–30 s, batch ≥ 3), and payload
padding to three size classes (256/1024/4096 B) so depth doesn't show.

**Residual risk — stated in the spec itself, not new here.** §9's own honest
limit: *"beats local observers and any subset of mixes; a global passive
observer is only beaten while decoy traffic flows."* Mix mode is an explicit,
non-default opt-in (`MISSION.md`'s locked decision table: "Tor / global
anonymity by default" is a stated non-goal) — a node that never turns it on
gets none of this, and that is visible to the user as a mode, not a silent
gap.

**Explicitly out of scope.** Physical-layer transmission is not made
invisible by anything above it. A radio observer with a directional antenna
can locate a transmitting device regardless of what the payload says — the
same limit that applies to Meshtastic or any other RF system riding the same
physical layer. SPORE does not claim otherwise on any radio-bound interface.

---

## 2. Participants — malicious carriers and relays

**Asset:** availability of the message, integrity of what's delivered, this
node's own resources.

**The model, stated once, is the "untrusted postman":** a carrier is asked to
move an object it cannot read and should not be trusted merely because it is
carrying it. `MISSION.md`'s "Confidential ≠ anonymous" and SPEC §0's "every
link is hostile" both point at the same design: nothing here assumes a relay
is honest.

| Attack | Mitigation | Reference |
|---|---|---|
| Forge a delivery receipt to fake "delivered" | Receipt must carry a verified signature from the actual destination | [S-032](SECURITY_FINDINGS.md#s-032) |
| Reflect/amplify via WANT | Per-interface token bucket on gossip service | [S-012](SECURITY_FINDINGS.md#s-012) |
| Bypass congestion control with a low-effort flood | Stamp (proof-of-work) threshold gate | [S-003](SECURITY_FINDINGS.md#s-003) |
| Forge a path-table entry to redirect unicast | Relays verify a signature before binding a path to it | [S-002](SECURITY_FINDINGS.md#s-002) |
| Flood ten different growable tables | Bounded, with deliberate eviction order (expired → lowest stamp → largest → oldest) | [S-013](SECURITY_FINDINGS.md#s-013) |
| Exhaust an MCU node's heap with desktop-sized ceilings | Every table ceiling derivable from a runtime's actual memory budget | audit [#189](https://github.com/sloev/spore/issues/189) rec. 2, `Limits::for_budget` |
| Tamper with spilled/cached bytes | An entry read back is re-verified against its content-derived ID; a mismatch reads as "not held" | SPEC §5, "Custody is untrusted storage" |

**Residual risk — this is not solvable by cryptography, and the spec says
so.** A relay can still drop, delay, or reorder anything that passes through
it — signing and sealing protect what the message *says*, not whether a
hostile forwarder chooses to forward it. This is the same lesson that applies
to Reticulum or any store-and-forward design: **you can encrypt a packet, but
you cannot cryptographically force a hostile relay to forward it.** What
bounds the damage is redundancy — multiple carriers, flood-fallback when a
unicast path dies (§5.5–5.6), custody re-push on the next fresher-path
sighting (§6) — not a guarantee that any single relay behaves.

A relay that behaves correctly *most* of the time and selectively drops one
target's traffic (a greyhole, in the terminology an external audit used) is
harder to catch than one that always fails, and nothing here specifically
detects that pattern. It is bounded by the same redundancy mechanisms, not
eliminated by them.

---

## 3. Identities — compromise, Sybil, and what "revoked" can mean

**Asset:** the ability to speak *as* an address; group membership; the
correctness of "who sent this."

**Address = SHA-256(pubkey) (§1).** This sidesteps the classic TOFU
first-contact problem other systems have: there is no separate "channel key"
someone can hand out that grants speaking rights for an identity that isn't
theirs. What an [`invite.rs`](../src/invite.rs) *can* forge is the **hint**
attached to an address — a claimed name, claimed bridges — never the address
itself; a forged invite gets you a wrong contact, never a forged identity.
That file's own doc comment says this and means it as a warning, not a
guarantee: "confirm the bridges before joining them."

**Sybil resistance — a locked design guardrail, not a shipped feature.**
Stamp (§10) is **per-envelope** proof-of-work. It raises the cost of any one
envelope; it says nothing about *identity count*, because it isn't bound to
one. An attacker who mints ten thousand addresses pays the same per-envelope
stamp cost as one honest sender making ten thousand honest posts — the
mechanism cannot tell them apart, because it was never asked to.

> **Guardrail:** no feature that weighs replication, priority, or trust by
> *how many distinct peers* vouch for or forward something — "send more
> copies toward frequently-encountered devices," reputation scores,
> popularity-weighted routing — ships without a documented Sybil analysis
> first. Nothing in the codebase today needs this guardrail; stamp and
> per-source quotas are both identity-agnostic by design. It exists so a
> future feature doesn't have to re-derive the problem from scratch, and so
> "many nodes agree" is never treated as "many independent people agree"
> without first stating what makes them independent.

**Key compromise — a real, unaddressed gap.** Topic (group) keys have a full
answer: `rotate` for forward secrecy, `rekey_seal` for eviction,
`contribute`/`absorb` for healing after a copied key (§7.1,
[Design](DESIGN.md#invites-and-what-revoke-can-and-cannot-mean-w7)). A
node's **identity** signing key has none of this. If the seed that derives
it is stolen, the thief can sign as that address indefinitely — there is no
revocation certificate, no protocol-level "this address's key is no longer
trusted" statement, and no CRL-equivalent, because none exists in the wire.
The only real answer today is social: stop trusting the old address, out of
band, the way you would if someone told you their phone was stolen.

Building a real answer (a signed statement peers could carry and honor) is a
new wire feature under the frozen-format process, not an oversight fixable
in a doc pass — recorded here so it is a stated gap, not a silent one.

**Prekeys are a separate, already-solved problem.** A stolen *prekey* secret
is bounded by the 7-day offline window and daily rotation (§7.2) — narrower
blast radius than an identity key by design, and already documented in
[Design](DESIGN.md)'s "Prekey ring (§7)" section, "what is still not
solved," point 2.

**Multi-device identity — tracked, not decided.** One keypair per device
today (§1, §11: "ratchet state is per-device — give each device its own
key"). A person-level identity spanning several authorized devices is an
architecturally significant, likely wire-affecting change, tracked as an
explicitly undecided question in [`ROADMAP.md`](ROADMAP.md)'s non-goals
table rather than promised or silently absent.

---

## 4. Resources — storage, CPU, battery, bandwidth, spam

This is the best-covered chapter, because it has been the subject of a
dedicated audit ([#189](https://github.com/sloev/spore/issues/189)) on top
of the ordinary findings register.

| Attack | Mitigation | Reference |
|---|---|---|
| Storage exhaustion (unbounded growth of any table) | Every growable table capped, with eviction order | [S-013](SECURITY_FINDINGS.md#s-013), [S-006](SECURITY_FINDINGS.md#s-006), [S-016](SECURITY_FINDINGS.md#s-016), [S-017](SECURITY_FINDINGS.md#s-017) |
| Storage exhaustion (fragment reassembly, count bounded but not bytes) | Byte budget across all incomplete fountain sets, not just a count cap | audit #189 F-3 |
| Storage exhaustion (unsigned traffic exempt from source quota) | Quota applies regardless of signature presence | audit #189 F-2 |
| CPU exhaustion (audio demod rescans its whole buffer every call) | Scan cursor makes cost proportional to new samples, not buffer size | [S-031](SECURITY_FINDINGS.md#s-031) |
| CPU exhaustion (integer overflow reachable from the wire) | Checked arithmetic on the affected path | [S-019](SECURITY_FINDINGS.md#s-019) |
| Battery exhaustion (aggressive beaconing) | Trickle timer (5→80 min, doubling on silence) instead of a fixed short interval | [S-023](SECURITY_FINDINGS.md#s-023) |
| Bandwidth exhaustion (reflection/amplification) | Per-interface WANT service budget | [S-012](SECURITY_FINDINGS.md#s-012) |
| Bandwidth exhaustion (relayed traffic crowding a link) | Token bucket at 10% of interface capacity | SPEC §5.4a |
| Every table sized for a desktop, shipped on a 226 KB-heap MCU | `Limits::for_budget` scales every ceiling from one number, with floors so a table never trims to zero | audit #189 rec. 2 |
| Third party's traffic filling a node's quota | Per-source, per-topic quotas | SPEC §10, [S-004](SECURITY_FINDINGS.md#s-004) |
| Spam (unlimited low-cost envelope minting) | Stamp: proof-of-work priority, unforgeable on every medium; unsigned mail rides last | SPEC §10 |

**Residual risk — stamp raises cost, it does not cap it.** Proof-of-work
priority means a well-resourced attacker can still mint high-stamp junk; it
simply costs them real compute and battery to do it, same as it would cost
an honest sender to earn priority. This is the "postage" model doc-review
commentary independently arrived at, and it is the right *shape* of partial
answer — cost, not prohibition — but it is not a cap, and nothing here claims
it is one.

**Explicitly out of scope.** Jamming and other physical-layer denial of
service. Listen-before-talk and duty-cycle limits (SPEC page 2, per-medium
tables in [Bridges](BRIDGES.md)) are politeness, not defense — nothing at
this layer addresses someone filling the air with noise, and nothing here
claims to.

---

## 5. Network / transport — routing, partitions, wormholes, gateways

**Finding, from auditing this catalogue's "wormhole/eclipse" item against
the code:** path learning (§4) is **local and non-transitive.**

`Paths::learn` records up to 3 `(iface, neighbor, age)` candidates per
address, newest first, from the first copy of any signed envelope a node
personally receives. The wire's ANNOUNCE payload has a field for advertising
*other* nodes' path freshness (`np:1][(addr:8, age_min:2) ea]`, SPEC §4) —
but the reference implementation always sends `np = 0`
(`build_announce_at_hops`: *"we advertise no distant paths in this reference
build"*), and `absorb_announce` does not parse that field on receipt at all.
So a node's belief about how to reach an address is built **only** from
signed traffic it has personally, directly seen — never adopted secondhand
from a peer's claim about a third party.

That bounds a wormhole/eclipse attacker's reach precisely: it can dominate
the "tried first" slot in *its own direct neighbors'* path table for
addresses whose traffic it relays fast enough to keep winning — by
continually being the fastest deliverer, it keeps re-inserting itself at the
front (`learn` always inserts at index 0). It **cannot** poison path beliefs
at any node it has not directly delivered signed traffic to; there is no
mechanism for that belief to spread.

The scope is narrower still: path-table lookups only gate **directed
unicast with `FLOOD` unset** (§5 rule 5). Topic, public, and `FLOOD` traffic
always damps-floods on every interface regardless of the path table, so an
eclipse cannot suppress or redirect anything but the one-hop unicast slice —
and even that slice flood-falls-back under custody if the winning path dies
(§5.5–5.6), so the attacker has to keep winning, not just win once.

**Residual risk, within that narrow scope.** A relay that wins "front of
list" for a target address sees more of that address's directed unicast
traffic pass through it than it otherwise would — a metadata/traffic-volume
concern (chapter 1), and it can drop that slice selectively (chapter 2's
already-stated relay-availability limit). Both are real, both are bounded to
one hop and to unicast, and neither compromises content — everything routed
through a hostile relay is still exactly as confidential and authentic as
§7's mechanisms make it regardless of which relay carries it.

**Gateways and bridged underlays.** SPEC page 2 treats Meshtastic,
Reticulum, Tor, WireGuard, and plain IP identically: "underlays with their
own routing = ONE interface." A compromised or monitored underlay is then
just a compromised or monitored *link*, already inside SPEC §0's baseline
assumption — not a special case requiring its own model.

**Partitions.** Not an attacker (see chapter 6's "accidental" category in
spirit) — a natural operating condition this whole architecture is built
around, not a failure mode to defend against.

---

## 6. Implementation & evolution — parsing, crypto, supply chain, upgrades

**Parser robustness.** Six fuzz targets in `fuzz/fuzz_targets/`
(`armor_and_framing`, `envelope_decode`, `fragment_reassembly`, `node_on_rx`,
`radio_codecs`, `seal_open`) exercise every wire-facing decoder against
malformed, truncated, and adversarial input. `robustness.rs` runs targeted
arbitrary-bytes tests against the live receive path — the mechanism that
found [S-001](SECURITY_FINDINGS.md#s-001) (a zero-count fragment causing a
division-by-zero panic) before it shipped. No `unwrap`/`expect`/indexing
panic is reachable from untrusted input on `ingest`/`fountain`/`session`/
`file`/`armor` (audited, per #189's "what is already solid").

**Cryptographic primitives.** Ed25519 (`ed25519-dalek`), X25519
(`x25519-dalek`, `crypto_box`), ChaCha20-Poly1305 / XChaCha20-Poly1305
(`chacha20poly1305`), BLAKE2b/SHA-256 (`blake2`/`sha2`) — standard,
widely-audited crates, not hand-rolled primitives. `deny.toml` and the
`cargo-deny`/`cargo-audit` CI jobs (`supply-chain.yml`) gate every dependency
change against known vulnerabilities and license terms.

**Supply chain.** A compromised build pipeline or release artifact is not
the only way to get a working node: [`REBUILD.md`](REBUILD.md), the
dependency-free reference T0 decoders (`reference/`), and the frozen test
vectors (`reference/vectors.json`) mean anyone can independently reimplement
and cross-check against the wire, not trust any single maintainer's binary.
Public-domain licensing (`LICENSE`) removes the legal kill switch a
corporate license could hold. Five release-integrity findings on record
([S-021](SECURITY_FINDINGS.md#s-021),
[S-025](SECURITY_FINDINGS.md#s-025)/[026](SECURITY_FINDINGS.md#s-026)/
[029](SECURITY_FINDINGS.md#s-029)/[030](SECURITY_FINDINGS.md#s-030)) are
evidence this specific class — the release pipeline itself as an attack
surface — has been checked, not assumed clean.

**Crypto agility under a frozen wire — a deliberate trade, stated plainly.**
`Envelope::decode` checks `buf[0] == VER` for **exact equality**; an
envelope carrying any other version byte is rejected outright, not carried
along or tolerated. There is no version-negotiation path. A hypothetical v2
wire format is a hard fork by construction: an old node silently drops every
v2 envelope it sees, rather than routing it unread the way an IP router
forwards a packet whose contents it can't parse.

This is consistent with — not a gap in — the project's stated priorities.
`MISSION.md`'s "small-spec + continuity" pillar and the frozen-wire hard
rule (`ROADMAP.md`: wire format is frozen, changed only via the
`allow-frozen-change` process, a deliberate major-version act) both choose
holdability over in-place upgradability. The cost is real: mixed v1/v2
meshes require every node to move, not a rolling negotiation. Agility
*within* v1 exists at finer grain — the one reserved flag bit (§2 flag b7,
"MUST be 0" today) and the payload-level app-tag convention the file layer
already demonstrates (`[0x01]`/`[0x07]`/`[0x08]`/`[0x09]` tags distinguishing
manifest/chunk/nested-manifest/sealed within one envelope type) both allow
new conventions without moving `VER`. A new AEAD suite or a revocation
certificate (chapter 3) is more likely to arrive that way than as a wire
version bump.

**Accidental / environmental — not an attacker, stated for completeness.**
Radio interference, device failure, battery death, packet corruption,
network partitions, no nodes nearby. This is the ordinary operating
condition store-and-forward delivery is built to survive, not a threat
requiring a countermeasure beyond what already exists (redundant carriers,
expiry rather than permanence, custody re-push on next contact).

---

## What a seized *honest* relay's storage reveals

Every mitigation above assumes a hostile participant. This section asks a
different question, and it is the sharpest one this document has to answer
honestly: **a fully compliant, non-malicious node's own storage — if
physically captured — is not empty of information about other people,**
even though it holds no plaintext it wasn't itself the endpoint for.

| Table | Retention | What it reveals |
|---|---|---|
| `store` (envelope bytes) | Until expiry — app default 7 d, wire-clamped ≤ 30 d (§2, §11) | Payload — ciphertext for sealed/ratcheted traffic, plaintext for public/open-group posts (by design) |
| `seen` (dedup ids) | ≥ 30 d received (§11, `SEEN_MIN_SECS`/`MAX_EXPIRY_HORIZON_SECS`) | Opaque content-hash IDs this node has relayed — not content, not sender/recipient by themselves |
| `paths` (addr → iface/neighbor/age) | Fresh < 3 h, purged at 7 d (§4) | **The sharpest one.** Which addresses this node has directly exchanged signed traffic with, and over which link — a real social-graph fragment |
| `peer_prekeys`/`peer_busy`/`peer_names` | Bounded by peer *count* (`MAX_PEERS`), not time — no explicit expiry | Claimed display names and current prekeys of every peer met, for as long as the peer table doesn't evict them |
| `sessions` (ratchet state) | In-memory only, not persisted across a restart | Which addresses have an active ratchet session (an ongoing 1:1) — and, if seized *live*, current chain state that could open messages not yet advanced past, though never messages already past a ratchet turn (§7's forward-secrecy property) |
| `pending`/`acked` (own sent-message ids) | `MAX_ACKED` count-bounded | This node's own send/receipt activity, by opaque id |

The honest conclusion: content stays protected exactly as far as §7's
mechanisms protect it, regardless of who seizes the device. **Metadata does
not** — `paths` in particular is a real, if partial, social graph, built
from ordinary operation with no malice required. This is the same
"honest-but-curious" concern chapter 2 raises about relays in general,
concretized with real retention numbers rather than left as a gesture. There
is no mitigation to cite here beyond what already exists (bounded retention,
not unlimited) — this is a residual risk of running a node at all, stated
plainly rather than left for a user to discover by reading source.

---

## Summary: what's real, what's partial, what's not attempted

| Property | Status |
|---|---|
| Content confidentiality (sealed/ratcheted traffic) | Real — §7 |
| Content confidentiality (open groups, microblog) | Not attempted, on purpose — public by design |
| Sender authenticity | Real — Ed25519 signatures, verified before any trust decision (§2, [S-002](SECURITY_FINDINGS.md#s-002)) |
| Replay resistance | Real — content-addressed IDs + dedup (§5), envelope expiry |
| Resource-exhaustion resistance | Real, and runtime-scalable — chapter 4, `Limits::for_budget` |
| Malicious-relay availability attack | Bounded by redundancy, not eliminated — universal to store-and-forward, chapter 2 |
| Sender/recipient metadata under passive local observation | Not attempted by default; real under opt-in mix mode | chapter 1 |
| Metadata under a global passive adversary | Partial — only while mix decoys flow | chapter 1, SPEC §9 |
| Sybil resistance | Not needed today (nothing counts identities); a guardrail exists for when it is | chapter 3 |
| Identity-key revocation | **Not solved** — social/out-of-band only | chapter 3 |
| Wormhole/eclipse | Bounded — local, non-transitive, unicast-only | chapter 5 |
| Honest-relay metadata retention | Real and bounded, but non-zero | above |
| Parser/implementation robustness | Real — fuzzed, tested, audited | chapter 6 |
| Crypto agility | Deliberately narrow — payload/flag-level only, not wire-version | chapter 6 |
