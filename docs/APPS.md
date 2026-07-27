# Apps & daemons — get a node

Four ways to run SPORE. They are the same node: one Rust core, one wire format,
one router. Pick by what the machine in front of you has.

| | Needs | Get it |
|---|---|---|
| **📱 SPORE Communicator** (Android) | a phone | [**⬇ spore-android.apk**][apk] — direct download ([release notes][apk-rolling]) |
| **🌐 Single-file web node** | a browser | [`spore-standalone.html`][standalone] — one file, works offline |
| **🖥 Desktop daemon** | Rust toolchain | `cargo build --release` → `target/release/spore` |
| **🖨 Seed Sheet** | a printer | [`spore-seedsheet.html`][seedsheet] — print it; QR codes rebuild the guide |

[apk]: https://github.com/sloev/spore/releases/download/rolling/spore-android.apk
[apk-rolling]: https://github.com/sloev/spore/releases/tag/rolling
[apk-stable]: https://github.com/sloev/spore/releases/latest/download/spore-android.apk
[standalone]: https://sloev.github.io/spore/demo/spore-standalone.html
[seedsheet]: https://sloev.github.io/spore/demo/spore-seedsheet.html

No app store, no account, no server. Two copies of any of these on the same
Wi-Fi find each other in seconds; with a radio or a speaker they don't even need
that.

## 📱 SPORE Communicator (Android)

A real node in a background service — not a client talking to one. Instant
messages with petnames, a microblog feed, and file sharing, over every bridge at
once: UDP and Wi-Fi Direct, the audio modem, Bluetooth Meshtastic and Reticulum
radios, and WebSocket / Nostr / WebTorrent through a headless WebView.

Built to feel familiar to Meshtastic users: simple by default, advanced options
one tap away, a little kawaii.

**[⬇ Download spore-android.apk][apk]** — a permanent link that always serves the
newest build. Rebuilt on every merge to `master` and versioned
`<major>.<minor>.<YYYYMMDDHHMM>+<short sha>`; the
[release notes][apk-rolling] name the version and carry its SHA-256. Verify before
installing if you care to:

```sh
curl -LO https://github.com/sloev/spore/releases/download/rolling/spore-android.apk
curl -LO https://github.com/sloev/spore/releases/download/rolling/spore-android.apk.sha256
sha256sum -c spore-android.apk.sha256
```

Install it as you would any APK; you may need to allow installs from your browser
or files app. Builds are debug-signed until a release keystore is configured, so
Android will warn about an unknown developer.

**Going back a version.** `rolling` is a moving pointer and holds only the current
APK, which is no help when a build breaks something. The `nightly-<date>` releases
keep the **last five dated builds** for exactly that — pick one and install it over
the top. Older nightlies are pruned automatically, so this is a rollback window,
not an archive.

**Stable releases** are tagged `vX.Y.Z` (either case), and their permanent link is
[`/releases/latest/download/spore-android.apk`][apk-stable].

Be warned that this link is only as good as the last tagged *build*. A tag with no
build behind it produces a release page that looks real and serves nothing — which
is exactly what `V0.1.0` and `V0.2.0` did, because the workflow matched `v*` and
those tags start with a capital. Fixed, but the lesson stands: check the release has
assets before pointing anyone at it. `curl -fsI` on the link above is the check.

<details>
<summary>What it does, and what still needs your hardware</summary>

**Connect (👋)** shows an invite QR carrying your address, the name you announce,
and the relay/swarm bridges you are on. A friend scans it, confirms a petname —
prefilled with your announced name, shown as a *claim*, since anyone may announce
anything — and can opt in to your bridges with a tick.

**Messages** to a peer are sealed to their key once you have heard their announce
(🔒) and ask for a delivery receipt (✓). Broadcasts and topic posts are signed but
public by nature, and the UI says so rather than implying otherwise.

**Files** ride the protocol's own manifest + chunk layer, so a transfer resumes
rather than restarts, survives the app being killed, and to a known peer is
sealed — contents *and* filename. Bounded by storage (256 MB by default, ~120 MB
per file), not by the wire format or the heap.

The radios, BLE, mic and live-peer paths are honest templates — the codecs are
tested, the hardware loop is not. Verify with [`HARDWARE.md`](HARDWARE.md).
Build instructions: [`android/README.md`](../android/README.md).
</details>

## 🌐 Single-file web node

[`spore-standalone.html`][standalone] is a complete node in one file: the wasm
core, every transport, the UI. Save it to a USB stick or mail it as an
attachment. It makes **zero network requests** — CI asserts that — so it runs
from `file://` with no internet at all.

It can also re-serialize itself ("Download a copy"), so one seed makes the next.

<details>
<summary>Reaching other copies</summary>

Add bridges at runtime from the page itself: WebSocket, WebRTC, Nostr,
WebTorrent, Web Serial and Web Bluetooth (Meshtastic radios, Reticulum RNodes,
generic KISS TNCs), and the audio modem. Settings and bridges persist in
`localStorage`, so a reopened tab is the same node with the same address.

Browser support varies — Web Serial and Web Bluetooth are desktop
Chromium-family only. Details in [`web/README.md`](../web/README.md).
</details>

## 🖥 Desktop daemon

One binary, every bridge, any mix at once — so one process is a gateway between
a LAN, a USB stick, and a radio mesh.

```sh
cargo build --release        # -> target/release/spore
cargo run                    # in-memory mesh demo (A — B — C — D)
cargo run -- node.yaml       # a real node with the bridges in a config file
```

Configuration and the bridge list are in the
[README](https://github.com/sloev/spore#build--run);
what each bridge speaks is in [`BRIDGES.md`](BRIDGES.md).

Plain `std`, so one source targets Linux, macOS, Windows and Android; the core
also builds for `wasm32` and `esp-idf`.

## 🖨 Seed Sheet

A two-sided A4 you can print and put on a shelf: the wire format worked by hand
on one side, the whole reimplementation guide as fountain-coded QR on the other.
**Any ~K of N codes** rebuild the payload, so a torn or stained sheet still
works — SPORE's own erasure coding, turned on itself.

Why that matters, and the other cold-start paths, is
[`CONTINUITY.md`](CONTINUITY.md).

## Building the seeds yourself

```sh
# Single-file browser node (needs the wasm first):
cargo build --release --lib --target wasm32-unknown-unknown
node web/build-standalone.mjs                            # -> web/spore-standalone.html

# Printable fountain-coded Seed Sheet:
cd site && npm install && node seed/build-seedsheet.mjs   # -> web/spore-seedsheet.html
```

Language bindings (Python / Go / JS over one C ABI) are in
[`bindings/`](../bindings/README.md).
