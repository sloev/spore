# Glossary

Words this project uses in a particular way, defined once. Each entry says where
the thing actually lives, so a definition can be checked rather than believed.

If a term here and the code disagree, the code is right and this page is a bug —
[a claim in the docs that the code does not honour](SECURITY.md) is a reportable
defect here, not a documentation nit.

---

### Envelope

The unit that crosses the wire: **a signed postcard**. To, from, when it was
written, a payload, and a signature over all of it.

Sixteen bytes of fixed header (version, type, flags, hops, `created_at`,
destination), then the sender — a 32-byte public key, or an 8-byte address if
[SRC8](#src8) is set, or nothing at all — then a two-byte payload length, then
the payload, then the signature.

Everything else in the protocol is either an envelope or a way of moving one.
Pinned byte-for-byte in `reference/vectors.json`; the layout is [SPEC §2](SPEC.md).

### Address

The first 8 bytes of `SHA-256(public key)`. Short enough to say out loud and to
fit a LoRa frame, long enough that colliding with a specific person is expensive.

**A topic has an address too**, from `SHA-256(topic name)`, in the same 8-byte
space. That is deliberate — a relay treats "for this person" and "for everyone
following this" identically, which is why there is no separate multicast
machinery. It also means *a name cannot be recovered from an address*, which is
why the app layer has to write topic names down ([Topic](#topic)).

### Topic

A public feed, named by a string, addressed by the hash of that string. Anyone
who knows the name can follow it, publish to it, and read it. A **keyed** topic
adds a shared secret so the mesh carries the traffic without being able to read
it — the address still leaks *that* the topic exists.

### Custody

Carrying an envelope that is **not for you and that you cannot read**, because
the node it is for is not here yet.

This is the behaviour the protocol is named for and the one that makes it
store-and-forward rather than routing. A node under custody holds the bytes,
offers them to everyone it meets, and has no idea where the recipient is or
whether they will ever appear. [A worked trace](KERNEL_FLOWS.md).

### Courier

A node that chooses to carry other people's mail for a long time — a high
`max_relay_age`. Not a role, a capability or anything on the wire: **it is one
local number.** A phone might carry an hour of other people's traffic; a solar
box on a hill might carry a month.

### Flood

The default way an envelope moves: every node that hears it relays it to every
interface **except the one it arrived on**, decrementing `hops`, until `hops`
reaches zero or everyone has already seen it.

`hops` counts **down**, and it bounds *push*, not reach: an envelope can travel
further than its hop count by being pulled ([INV / WANT](#inv--want)).

### Damped flood

A flood with the duplicates suppressed. On a shared medium a node waits a
jittered moment before transmitting, and **if it overhears the same envelope id
while waiting, it cancels its own copy** — someone else got there first, and a
second transmission would cost airtime to tell everyone something they now know.

`bridge::Csma`, and the reason a flood on one radio does not scale with the
square of the neighbours.

### Dedup

An envelope is recognised by its **id**: `SHA-256` of the whole envelope with
`hops` zeroed, truncated to 16 bytes. Zeroing `hops` is what makes two copies
that travelled different distances the *same* message — without it every hop
count would be a new id and a flood would never terminate.

### INV / WANT

How two nodes that meet find out what the other has. One sends an **INV** (a
list of ids it holds), the other replies with a **WANT** (the subset it lacks),
and the first sends those.

Both are unsigned, `hops = 0`, consumed on receipt, and never stored or relayed.
This is the *pull* half of the protocol, and it is bounded by demand rather than
by a hop count.

### Bridge

Anything that moves bytes between two nodes: a UDP socket, a serial cable, a
LoRa radio, an audio modem, a Bluetooth link, a file on a USB stick.

**Every bridge is assumed hostile.** It may drop, delay, reorder, duplicate or
read everything it carries; envelopes are signed and sealed end to end so that
none of that matters. [The full list](BRIDGES.md).

### Fragment / piece

How one envelope crosses one narrow hop. A bridge whose link cannot carry a
frame splits it, the far end of that same link puts it back together, and the
node never sees a piece.

A fragment lives for **one hop** and is never relayed, never named, never
stored. Its header is seven bytes and its first byte is `0xF6`, which is not a
protocol version, so a receiver can tell a piece from a whole envelope on sight.

### Chunk

A *file-layer* object: a fixed-size slice of a file, named by the hash of its
own content.

**A chunk is not a fragment**, and confusing the two is how chunk size once got
derived from an MTU. A chunk is the same size everywhere in the protocol and is
addressed by content; a fragment is however much of *some* envelope fits one
hop's frame and is addressed by nothing.

### Content id

`SHA-256(payload)` truncated to 16 bytes — the name of some **bytes**.

Distinct from an envelope id, which covers the whole envelope including when it
was written and who it was for. That is right for a message and wrong for
content: it would make the same chunk published a second later a different
object, so two people sharing the same file would share nothing.

### Magnet

The content id of a file's **root manifest** — the single 16-byte name you give
someone so they can fetch a whole file, however large.

### Manifest

The small object a file is published as: its name, its length, and the content
ids of its chunks. Above a certain size it becomes a *tree* of manifests, and
only the root is signed — everything beneath it is held together by the hash
chain, so a file of any size costs one signature.

### Fountain code

An erasure code: from `n` pieces it can mint **unlimited** distinct repair
symbols, and a receiver that collects enough of *any* of them reconstructs the
original. Useful on a link that loses frames and cannot ask for a retransmission.

**Scope has narrowed.** It was once the end-to-end fragmentation scheme; that
was removed. It now lives under [link fragmentation](#fragment--piece), where a
sender adds repair symbols sized to what the link is actually losing.

### Prekey

A short-lived X25519 public key a node publishes in its ANNOUNCE so that someone
who has never met it can still send it something sealed.

The seed restores *who you are*; the prekey ring restores *what you can still
open*. A node that keeps its seed but loses its ring keeps its address and
silently loses inbound mail sealed to any prekey it had rotated away from.

### Ratchet

The forward-secret session between two nodes that have exchanged messages: each
message advances a key chain, so a key recovered today does not open yesterday's
traffic. A Double Ratchet, in `src/ratchet.rs`.

### Mix

Optional batching that holds outbound envelopes and releases them **together, in
a shuffled order**, so an observer cannot pair "A sent" with "B received" by
timing or by position alone. It is a traffic-analysis speed bump, not Tor.

### Armor

Text encoding for an envelope — `~S1.…~` — so one can be pasted into a chat, an
email, a forum post, or read aloud. A SPORE message can cross a network that has
nothing to do with SPORE.

### SRC8

A flag meaning the sender is named by its 8-byte address rather than its full
32-byte public key, saving 24 bytes on a narrow link. **The receiver cannot
verify the signature of an SRC8 frame it has no key for**, so such a sender is
unauthenticated and the app layer refuses to file it under a conversation.

### Resource invariant

The rule the whole design is arranged around:

> No remote node can cause another to transmit, store, or process an unbounded
> amount, without continuing evidence of demand, or an explicit bounded local
> allowance.

Most of the constants in this project are one of those allowances. When a
proposal is refused, this is usually why.

### Verify before binding trust state

The rule that decides when a signature is checked ([SPEC §5](SPEC.md)):

> **Verify before binding trust state; do not verify to forward.**

A relay moves bytes it cannot read toward a destination it is not. It does not
check signatures to forward — it *does* check before learning a path, binding a
neighbour, or telling an app who sent something. An implementation that
"hardened" by dropping what it cannot verify would drop every envelope it merely
lacks the key for, which is most of them.

---

## Terms this project has stopped using

**Watermark.** [SPEC §6](SPEC.md) described INV as filtered by a "per-neighbour
watermark". Nothing implemented one, and writing this page is how that was found
— `build_inv` filters by topic and custody and nothing else. The claim has been
corrected in the spec and the idea is now a roadmap row, since re-offering a
neighbour ids it has already declined is real waste.

**Expiry.** An envelope used to carry a deadline every relay was expected to
honour, which let a stranger reserve a week of somebody else's storage. It now
carries `created_at` — when it was written — and each node decides how much age
it is willing to carry. See [Courier](#courier).
