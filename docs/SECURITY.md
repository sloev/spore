# Security policy

## Reporting a vulnerability

Open a [security advisory](https://github.com/sloev/spore/security/advisories/new)
on this repository, which is private until published. If that is unavailable to
you, open a normal issue saying only that you have a security report and asking
for a contact — do not put the details in a public issue.

There is no bug bounty. This is a public-domain project maintained by volunteers,
so expect a reply in days rather than hours.

**What is useful in a report**, roughly in order:

1. What an attacker gains, and what they need to start — "any peer on the medium,
   one packet, no key" is a very different report from "a session partner, over
   hours".
2. A reproduction. A failing test or a byte sequence beats a description of a code
   path, because several plausible readings of this code have turned out to be
   already handled — see [Investigated and not a
   finding](SECURITY_FINDINGS.md#investigated-and-not-a-finding).
3. Which file and function.

Every accepted finding is written up in
[`docs/SECURITY_FINDINGS.md`](SECURITY_FINDINGS.md) with its reproduction,
root cause, patch, and the test that keeps it fixed. You are welcome to be
credited there or not, as you prefer.

## What is in scope

The protocol and its implementations: the Rust core, the bridges, the wasm/browser
node, the C ABI and its language bindings, and the Android app. Also in scope, and
treated as real findings rather than documentation nits:

- **A claim in the docs that the code does not honour.** Two of the findings on
  record are exactly this — an offline build that could not run offline, and a
  minimum Rust version that could not compile the project. A promise nobody has
  executed is a defect, because someone will rely on it in the one situation where
  they cannot check.
- **A test that asserts the wrong thing.** One existing test asserted that a
  nearly-free proof-of-work stamp should bypass congestion control; the assertion
  encoded the bug.

## What is not in scope

- **A hostile link.** Every bridge is assumed compromised. A link that drops,
  delays, reorders, duplicates or floods is the design, not a bug — envelopes are
  signed and sealed end to end and the router survives all of it. What *is* in
  scope is a link that makes a node spend unbounded memory, CPU, disk or airtime,
  or that gets a node to believe something it should not.
- **Unsigned mail being forgeable.** SPORE permits unsigned envelopes. They are
  forgeable by anyone and are documented as such; a node that trusts one is
  misusing the API. That the *router* can be made to misbehave by one is in scope.
- **Metadata visible to a bridge.** A bridge sees that traffic happened, its size
  and its timing. Mix mode reduces this and is not Tor. Per-bridge exposure is
  documented in [`docs/BRIDGES.md`](BRIDGES.md); an undocumented leak is worth
  reporting, a documented one is a known limitation.
- **8-byte address collisions.** Addresses are 64-bit hashes. A collision is
  possible with effort, which is why the full public key travels whenever a frame
  is not `SRC8` and why nothing binds an address without a verified signature.

## Known limitations, stated plainly

Report these only if you can show something *worse* than what is written here.

- **Hardware-unverified bridges.** Anything marked 🧪 in
  [`docs/BRIDGES.md`](BRIDGES.md) has tested codecs and an untested hardware
  loop. Radio, BLE and audio paths are template-grade until someone runs the
  procedure in [`docs/HARDWARE.md`](HARDWARE.md) on real devices.
- **The ratchet and mix layers** have had one bounds review, not a cryptographic
  audit. Nobody has analysed the mix for traffic-analysis resistance beyond its
  size classes.
- **No third-party audit** has been performed on any part of this.
- **`with_node` is not reentrant** — an embedder whose closure calls back into the
  hub used to deadlock that thread silently. It now panics naming the bug instead
  (per-hub, per-thread guard), which is a diagnosable failure rather than a hang,
  but the underlying call shape is still not supported.
- **The remaining still-open items** are listed at the end of
  [`docs/SECURITY_FINDINGS.md`](SECURITY_FINDINGS.md). They are carried
  deliberately and are not secrets.

## Supported versions

The v1 wire format is frozen; `master` is the only supported branch. Fixes land
there and reach releases from there — there are no maintained back-branches to
backport to.

## Operator notes

- **The seed is the identity.** `node.seed()`, the Android and wasm persistence
  APIs, and the invite formats all expose a raw 32-byte secret. Never log it, and
  treat a device backup containing it as key material.
- **Run with the least you need.** The ICMP bridge wants `CAP_NET_RAW`; grant it
  with `setcap` on the binary rather than running the node as root.
- **Verify what you rebuilt.** Releases publish a SHA-256; the offline bundle
  publishes one too, and the single-file node prints the hash of its embedded wasm
  in its own footer.
