# State machines

*This page is generated:* `cargo run --release --example state_machines > docs/STATE_MACHINES.md`.
CI fails if it is stale.

Every edge below was **observed** — `examples/state_machines.rs` drives the real types and
records the transitions they take, so an edge the code can no longer take disappears from the
page. A hand-drawn diagram is a claim nobody re-checks, and this repository has shipped one of
those before: the router flow said "id seen, or expired?" for months after expiry stopped
existing.

These are the two machines the [September audit](ROADMAP.md) named as unreviewed, and writing
the tests that produce them turned up a live fault in each.

## Double Ratchet — what happens to a received frame

```mermaid
stateDiagram-v2
  Established --> Refused: fails to authenticate — nothing committed
  Established --> Refused: gap past MAX_SKIP — refuse without computing
  Established --> Refused: n < nr — already consumed
  Established --> Opened: n == nr — derive from chain
  Established --> SkippedBanked: n > nr — bank the gap
  NoSendingChain --> Established: first message opens — DH ratchet
  SkippedBanked --> Refused: banked key already spent
  SkippedBanked --> Refused: older than the skip TTL — dropped
  SkippedBanked --> Opened: the banked key opens it
  [*] --> NoSendingChain: init_bob
```

- A responder has no sending chain until it has received: `can_send()` is false, and its own earlier messages go as one-shot seals instead.

- Receiving one message is what gives the responder a sending chain.

- `Refused` is a dead end by design: **no path out of it changes state**. That is the whole of the M1 fix — the machine used to commit the DH step and the chain advance on the way to `Refused`, so one forged frame moved the session somewhere no genuine message could reach.

- The TTL edge is forward secrecy doing its job: a key banked for a straggler is deleted once the window closes, so a message that arrives too late is unreadable *by design* rather than by failure.

## Mix — the release queue

```mermaid
stateDiagram-v2
  BelowThreshold --> BelowThreshold: fewer than min_batch due — hold
  BelowThreshold --> BelowThreshold: held >= min_batch but not enough *due* — hold
  BelowThreshold --> Empty: min_batch due — shuffle and release
  Empty --> BelowThreshold: add
  [*] --> Empty: new(min_batch)
```

- The second self-loop is the bug this machine used to have. The threshold was checked against how many were *held* rather than how many were *due*, so three held with one due released that one alone — a singleton an observer can follow straight through, which is exactly what a batch exists to prevent.

- Release shuffles. A mix that emits in arrival order lets an observer correlate by position without needing any timing at all — the delays hide *when* a message left, the batch hides *which* one it was, and without the shuffle the second buys nothing.

## Hub — an interface's life

```mermaid
stateDiagram-v2
  NoInterfaces --> Sending: register
  NoInterfaces --> PullOnly: register_pull (no sender)
  PullOnly --> PullOnly: nothing is pushed to it, it answers WANT from the store
  Retired --> Retired: a flood is not delivered to it
  Retired --> Sending: a new register takes a fresh id, never this hole
  Retired --> Retired: unregister again, or on an unknown id — no-op
  Sending --> Sending: a flood reaches it
  Sending --> Retired: unregister — sender dropped
  [*] --> NoInterfaces: Hub new
```

- A retired slot is emptied rather than removed, so **iface ids are never recycled within a process**. `Forward::Flood`'s `except` addresses interfaces by index, so reusing a hole would silently misroute the one thing that must not be misrouted.

- Retiring an interface also retires the pulls it was the reason for (M11-K): anything this node adopted an interest in *on behalf of* a peer behind that link has lost its waiter, and hunting for it is now work for nobody.

- Not drawn, because it is not observable from outside: a hub holds **one of its own locks at a time**. Every method that needs both the node and the outbound table scopes the first guard and lets it drop before taking the second — with at most one held there is no pair to invert, which is why there is no lock *order* to get wrong. That was a comment until #278; it is now asserted on every take, in test builds, across every public method.

