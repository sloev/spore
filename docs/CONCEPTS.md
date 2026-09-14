# Concepts

The landing page speaks in metaphor and the spec opens in wire format. This page
is the bridge: each metaphor, then the thing it is actually made of, then where
to read the rules.

You do not need this page to *use* SPORE. You need it to stop being surprised by
it — and the surprises are usually places where the protocol refuses to pretend.

---

## "A spore, and every device is soil"

A spore is a small package carrying everything needed to regrow the whole
organism from one copy. That is the design constraint, not a slogan: **any single
device that survives must be able to rebuild the network around it.**

Which is why the reference implementation is one Rust crate with no services in
it, why the browser node is a single HTML file with zero external requests, why
there are decoders in C, Python and shell that depend on nothing, and why
[Rebuild](REBUILD.md) walks the whole protocol from a pen and paper upward.

There is no bootstrap server to be missing, because there is no bootstrap server.

## "A signed postcard"

An **envelope**, and the postcard is exact in all three ways that matter.

**Anyone who handles it can read it**, unless you sealed the contents. A relay
sees who it is for, roughly how big it is, and when it was written. Sealing hides
the message; it does not hide that a message happened.

**Anyone can write one.** Addresses are public and nothing stops a stranger
posting to you. What they cannot do is write one *as you*: the signature covers
the whole card, and forging it means finding your key.

**It is handed along by people who are not couriers.** Every device that sees a
postcard for someone else may pocket it and carry it — that is
[custody](GLOSSARY.md#custody), and it is the whole protocol.

What is on the card: to, from, **when it was written**, the payload, and a
signature over all of it. Not an expiry — that changed deliberately, and the
reason is [below](#why-a-message-says-when-it-was-born-not-when-it-should-die).

> The exact bytes: [SPEC §2](SPEC.md). Pinned in `reference/vectors.json`, which
> CI refuses to edit.

## "It catches up later"

There is no route. A node does not know where you are and never learns.

When two devices meet — over Wi-Fi, a cable, a radio, a shared folder, a USB
stick physically carried across a valley — they say what they have and ask for
what they lack. That is [INV / WANT](GLOSSARY.md#inv--want), and it is the entire
delivery mechanism.

A message therefore travels **as fast as people and devices move**, which on a
good day is the speed of the internet and on a bad day is walking pace. Both are
the same protocol; neither is a fallback mode.

> [A worked trace of a message crossing an hour of the recipient being switched
> off](KERNEL_FLOWS.md).

## "Your device is the address"

Your identity is one Ed25519 keypair. Your address is the first eight bytes of
the hash of its public key. There is no registration, so there is nothing to be
refused and nothing to be revoked.

The cost is written down rather than hidden: **a stolen key is you, permanently.**
There is no recovery, because recovery means an authority, and an authority is
the thing this does not have. Treat a device backup containing a seed as key
material. [Threat model](THREAT_MODEL.md).

## Why a message says when it was born, not when it should die

An envelope used to carry an expiry that every relay was expected to honour.

That is a sender telling strangers how long to spend their storage — so a single
stranger could reserve a week of everybody's disk by writing a large number in a
field nobody checked. It was replaced with `created_at`: the message says *when
it was written*, and each node decides how much age it is willing to carry.

The same change made a **[courier](GLOSSARY.md#courier)** expressible without any
new message type or wire feature. A courier is a node that sets one local number
high. A phone might carry an hour of other people's traffic; a solar box on a
hill might carry a month. Neither announces it, and neither has to be trusted.

This is the shape of most decisions here: *push a choice from the wire into local
policy, and the protocol gets smaller while the implementations get more room.*

## Why a relay forwards what it cannot verify

The rule is **[verify before binding trust state; do not verify to
forward](GLOSSARY.md#verify-before-binding-trust-state)**, and it reads backwards
until you see what the alternative costs.

A relay moves bytes it cannot read toward a destination it is not. It has no key
for most of what it carries — that is what carrying other people's mail *means*.
An implementation that "hardened" by dropping what it cannot verify would drop
almost everything, and the mesh would stop working for exactly the traffic it
exists to move.

So a bad signature costs the sender **attribution**, not carriage: the envelope
binds no path, names no neighbour, and reaches the app flagged for the app to
check. Signatures are verified before anything is *believed*, never merely to
pass something on.

## Why there is no group membership list

An open group is a [topic](GLOSSARY.md#topic): a name, hashed to an address.
Anyone who knows the name can follow it, publish to it, and read it. Nobody can
be removed, because there is no list to remove them from.

A keyed group adds a shared secret, so the mesh carries the traffic without
reading it — but the address is still visible, so **that** the group exists and
roughly how busy it is are not secret. Membership is whoever holds the key.

The documentation says this plainly rather than implying moderation that does not
exist. If you need a group where someone can be excluded, this is not yet that,
and [the roadmap says so](ROADMAP.md).

## What is honestly still open

Not a disclaimer — a reading list, because these are the things most likely to
matter to a decision:

- **[Security maturity](SECURITY_MATRIX.md)** — per component, what has actually
  been fuzzed, property-tested, reviewed and run on hardware. The "independently
  reviewed" column is empty for every component, and saying so is the point.
- **[Hardware verification](HARDWARE.md)** — which radios and devices have been
  run with a person present, and which are code that compiles and has never met
  an antenna.
- **[Threat model](THREAT_MODEL.md)** — six chapters of adversary, what stops
  each, and the residual risk where nothing fully does.

---

## Where to go next

| If you want | Read |
|---|---|
| the words, defined | [Glossary](GLOSSARY.md) |
| a message moving, step by step | [Kernel flows](KERNEL_FLOWS.md) |
| the rules, normatively | [SPEC](SPEC.md) |
| to rebuild it from nothing | [Rebuild](REBUILD.md) |
| what it promises and does not | [Threat model](THREAT_MODEL.md) |
