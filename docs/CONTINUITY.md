# Continuity — SPORE as a seed

SPORE is named for a reason. A spore is a small, hardy, self-contained capsule
that can lie dormant and later **regrow the whole organism** from a single
survivor. This document is about making the *software* behave the same way: so
that any one copy — a repository clone, a single HTML file, a printed booklet, a
scanned code, a message typed off paper — carries enough to understand the
system, prove it's authentic, rebuild a running node, and reproduce the other
copies. No step here needs a server, a package registry, or a working internet.

Three properties we design for:

- **Readable** — a person can understand it from the artifact in hand.
- **Reconstructable** — a person can rebuild a working node from that artifact.
- **Self-propagating** — the network can carry its own code, tools, and manuals.

<details>
<summary>Why "every copy is a seed" is a design constraint, not a slogan</summary>

Most software assumes a supply chain: a registry to install from, a CDN to load
from, a build farm, an app store, an always-on network to bootstrap trust. Each of
those is a single point of failure over a long enough horizon. SPORE's job — moving
messages when the usual infrastructure is degraded or gone — is undermined if
*getting* SPORE depends on that same infrastructure. So the constraint is: the
means of understanding, verifying, and rebuilding SPORE must travel *with* SPORE,
in forms that outlive any one host. That pushes specific engineering choices —
a dependency-light core, a spec independent of the Rust code, reproducible builds,
paper-friendly formats, and the network distributing its own installer.
</details>

## What's a seed today

Concrete artifacts that already carry the node, ranked by how little they assume:

| Seed | Assumes | Gets you |
|---|---|---|
| **Single-file HTML node** (`web/spore-standalone.html`) | a browser | a full node, offline, from one file — no install, no network |
| **Seed Sheet** (`web/spore-seedsheet.html`, printed) | a camera + patience | scan any ~K of N QR codes to recover the reimplementation guide; a torn sheet still works |
| **Repo clone** | a Rust toolchain | build the daemon + all bridges + bindings, offline |
| **The spec** (`docs/SPEC.md`) + by-hand examples | pen, paper, patience | reimplement a compatible node in any language |
| **Pure-Python T0 node** (`reference/spore_t0.py`) | Python 3 | receive + verify public messages, no packages, no toolchain |
| **An armored envelope** (`~S1.…~`) | a human who can type | inject one message into a node from paper or voice |

The single-file node is the flagship: `node web/build-standalone.mjs` inlines the
wasm and every script into one `.html` that runs from a USB stick or an email
attachment and makes **zero network requests** (CI asserts this). Save it, copy it,
carry it — a copy is enough to rejoin or restart a mesh. The page can even
re-serialize itself ("Download a copy"), so one seed makes the next.

<details>
<summary>Build the seeds</summary>

```sh
# Single-file browser node (needs the wasm first):
cargo build --release --lib --target wasm32-unknown-unknown
node web/build-standalone.mjs          # -> web/spore-standalone.html

# Printable, fountain-coded Seed Sheet (any ~K of N QR codes rebuild it):
cd site && npm install && node seed/build-seedsheet.mjs   # -> web/spore-seedsheet.html

# The daemon and everything else:
cargo build --release                  # -> target/release/spore

# The language bindings (Python / Go / JS) live in bindings/.
```

The published site (GitHub Pages) also hosts `spore-standalone.html` and the live
demo, so the seed is one download away as long as *any* mirror survives.
</details>

## Cold-start playbooks

What to do when you have less than the usual everything.

<details>
<summary>Only a browser</summary>

Open `spore-standalone.html`. You have a full node. To reach others: paste a
WebSocket relay URL (any tab running a relay, any surviving `ws`/`wss` endpoint)
into "Reach other copies". Two laptops with mics and speakers can pair with **no
network and no cables** once the audio transport is wired to WebAudio — same wasm.
</details>

<details>
<summary>Only radios (LoRa, ham, Meshtastic)</summary>

Run the daemon with the matching bridge (see `docs/BRIDGES.md`). The router,
address resolution, and fragmentation are medium-independent; a radio bridge is a
thin `recv`/`send` shim. A Meshtastic WiFi-UDP node, an RNode over serial, or an
AX.25 TNC all carry SPORE envelopes unchanged.
</details>

<details>
<summary>Only paper (and voice)</summary>

Envelopes have a text **armor** form: `~S1.<base32>.<checksum>~`. It survives SMS,
handwriting, a read-aloud phone call, or a photograph. Type an armored envelope
into any node and it enters the mesh; the checksum catches transcription errors.
For bulk (source, manuals), the roadmap adds fountain-coded QR so a *partial*
printout still rebuilds the whole (SPORE's own erasure coding, turned on itself).
</details>

<details>
<summary>Only one laptop, no internet</summary>

A repo clone plus a Rust toolchain rebuilds everything offline — the build vendors
its dependencies and needs no registry. Verify what you rebuilt against the printed
/ signed hashes (below). One machine can then seed others by folder sync (a USB
stick is a bridge), by sound, or by handing over the single-file node.
</details>

## Trust without infrastructure

Authenticity can't depend on a live server either. The anchors:

- **Content addressing.** An envelope's ID *is* the hash of its bytes, and a node's
  address *is* the hash of its key. Nothing is trusted for being in the right place
  — only for hashing/verifying correctly.
- **Signed everything.** Path learning binds only from signed frames; releases are
  signed manifests; the manual can carry the maintainers' public keys.
- **Reproducible builds.** A rebuilt binary should hash to a published value, so a
  copy is verifiable offline against a number you can print, sign, or memorize. The
  single-file node prints the SHA-256 of its embedded wasm in its own footer.

## Roadmap

What exists and what's next, from the fuller design discussion.

- [x] **Single-file browser node** — one self-contained HTML file, tested to use
      zero network.
- [x] **Config-driven daemon + bridge matrix** — one binary, many media
      (`docs/BRIDGES.md`).
- [x] **Language bindings** — Python / Go / JS from one C ABI (`bindings/`).
- [x] **This continuity guide** with cold-start playbooks.
- [x] **Reimplementation guide** ([`REBUILD.md`](REBUILD.md)) — the wire format
      with worked-by-hand examples (address, envelope bytes, signature, armor)
      generated from the real code, so any language can rebuild a compatible node.
- [x] **Printable Seed Sheet** (`site/seed/build-seedsheet.mjs`) — a two-sided A4:
      the wire format by hand on one side, the full reimplementation guide as
      fountain-coded QR on the other, with the payload's SHA-256 printed on it.
- [x] **Fountain-coded print** — a random-linear code over GF(256)
      (`site/seed/fountain.mjs`, the paper twin of §3) where *any ~K of N* QR codes
      reconstruct the whole; a reference decoder (`decode-seedsheet.mjs`) and a
      full drop-a-third round-trip test ship with it.
- [ ] **Codex** — the complete source as a hash-stamped booklet (the Seed Sheet
      scaled up), for machines with no toolchain to retype or rescan from.
- [ ] **The network carries its own genome** — a well-known bootstrap-bundle magnet
      (source + binaries + this manual), signed self-update over the mesh, and
      opt-in seed-vault nodes that pin it and never evict it.
- [ ] **Trust roots on paper** — maintainer keys and a mnemonic-encoded release
      hash you can carry in memory; multi-signature releases so no single custodian
      is a point of failure.
- [x] **Tiny reference decoder** ([`reference/spore_t0.py`](../reference/)) — a
      dependency-free pure-Python T0 node (parse + address + ID + Ed25519 verify)
      for machines without a Rust toolchain, checked against the Rust vectors in
      CI. Pure-C and shell ports are the natural next tiers.

## Help re-seed

The most useful thing anyone can do is **keep a copy somewhere the others aren't**:
a clone on an offline disk, the single-file node on a phone, a printed spec on a
shelf, a mirror on a different host. Continuity is just redundancy that outlives its
sources — and it only works if the copies are already scattered before they're
needed.
