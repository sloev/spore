# Apps & daemons — get a node

Four ways to run the same node. Pick by what the machine in front of you has.

| | Runs on | Distinguishing property |
|---|---|---|
| **Communicator** | Android | Full node in a background service, not a thin client |
| **Web node** | Any browser | One HTML file, zero network requests until you add a bridge |
| **Daemon** | Linux, macOS, Windows | Many bridges in one process |
| **Seed Sheet** | Paper | Rebuilds the guide from any ~K of N fountain QR codes |

<div class="grid">

<div class="col-3"><div class="card"><div class="card-body">
<strong>📱 SPORE Communicator</strong>
<p class="text-muted">Android phone — full node in a background service, not a thin client.</p>
<details>
<summary>Install, verify, what it does</summary>
<p>Permanent rolling link; rebuilt on every merge. Version
<code>&lt;major&gt;.&lt;minor&gt;.&lt;stamp&gt;+&lt;sha&gt;</code>. Verify if you care:</p>
<pre><code>curl -LO https://github.com/sloev/spore/releases/download/rolling/spore-android.apk
curl -LO https://github.com/sloev/spore/releases/download/rolling/spore-android.apk.sha256
sha256sum -c spore-android.apk.sha256</code></pre>
<p>Allow installs from the browser/files app. Builds are debug-signed until a
release keystore is configured — Android will warn about an unknown developer.</p>
<p><strong>Rollback:</strong> <code>nightly-YYYY.MM.DD</code> keeps the last five dated
builds. <strong>Stable:</strong>
<a href="https://github.com/sloev/spore/releases/latest/download/spore-android.apk">/releases/latest/download/spore-android.apk</a>
— only as good as the last tagged build that actually has assets.</p>
<p>Messages, petnames, feed and files over UDP, Wi-Fi Direct, the audio modem,
BLE and WebView bridges. Radios stay 🧪 until <a href="hardware.html">HARDWARE</a>
records a device run.</p>
</details>
</div>
<div class="card-footer cluster">
<a class="btn" href="https://github.com/sloev/spore/releases/download/rolling/spore-android.apk">⬇ Download APK</a>
<a class="btn btn-cancel" href="https://github.com/sloev/spore/releases/tag/rolling">Release notes</a>
</div></div></div>

<div class="col-3"><div class="card"><div class="card-body">
<strong>🌐 Single-file web node</strong>
<p class="text-muted">One HTML file — wasm + UI + transports. Zero network until you add a bridge.</p>
<details>
<summary>Offline, USB, self-copy</summary>
<p>Save it, mail it, put it on a stick. CI asserts <strong>zero</strong> external requests, so
<code>file://</code> works with no internet. “Download a copy” re-serializes the page so one
seed makes the next. Identity and bridges live in <code>localStorage</code>.</p>
<p>Add WebSocket, WebRTC, Nostr, WebTorrent, Serial, Bluetooth, or the audio modem
from the page. Web Serial / Web Bluetooth need desktop Chromium-family browsers.</p>
<p>Every <a href="https://github.com/sloev/spore/releases/tag/rolling">release</a> carries
the same file as a permanent asset
(<a href="https://github.com/sloev/spore/releases/latest/download/spore-standalone.html">latest</a>),
so a copy need not depend on this site.</p>
</details>
</div>
<div class="card-footer cluster">
<a class="btn" href="spore-standalone.html">Open standalone</a>
<a class="btn btn-cancel" href="demo/">Live demo</a>
</div></div></div>

<div class="col-3"><div class="card"><div class="card-body">
<strong>🖥 Desktop daemon</strong>
<p class="text-muted">One binary, many bridges — LAN + USB folder + radio in one process.</p>
<details>
<summary>Build &amp; run</summary>
<pre><code>cargo build --release        # → target/release/spore
cargo run                    # in-memory mesh demo
cargo run -- node.yaml       # bridges from a config file</code></pre>
<p>Per-bridge reference: <a href="bridges.html">Bridges</a>.</p>
<p><strong>No network to reach crates.io?</strong> Every
<a href="https://github.com/sloev/spore/releases/latest/download/spore-offline-bundle.tar.gz">release</a>
also carries this whole source tree with every dependency already vendored in
— it unpacks flat (no wrapping folder), so give it one:</p>
<pre><code>mkdir spore-offline &amp;&amp; cd spore-offline
curl -LO https://github.com/sloev/spore/releases/latest/download/spore-offline-bundle.tar.gz
tar xzf spore-offline-bundle.tar.gz
cargo build --release --offline</code></pre>
</details>
</div></div></div>

<div class="col-3"><div class="card"><div class="card-body">
<strong>🖨 Seed Sheet</strong>
<p class="text-muted">Printable A4 — any ~K of N fountain QR codes rebuild the guide.</p>
<details>
<summary>Torn sheet still works</summary>
<p>One side: wire format by hand. Other side: reimplementation guide as
fountain-coded QR. A stained or partial print can still recover the payload —
SPORE’s own erasure coding turned on itself. Build it with
<code>node site/seed/build-seedsheet.mjs</code> after wasm is available.</p>
</details>
</div>
<div class="card-footer cluster">
<a class="btn" href="spore-seedsheet.html">Open Seed Sheet</a>
<a class="btn btn-cancel" href="continuity.html">Why continuity</a>
</div></div></div>

</div>

## Building the seeds yourself

```sh
# Single-file browser node (needs the wasm first):
cargo build --release --lib --target wasm32-unknown-unknown
node web/build-standalone.mjs                            # -> web/spore-standalone.html

# Printable fountain-coded Seed Sheet:
cd site && npm install && node seed/build-seedsheet.mjs   # -> web/spore-seedsheet.html
```

Language bindings (Python / Go / JS over one C ABI) are in
[bindings](bindings.html).
