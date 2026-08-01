# Apps & daemons — get a node

Four ways to run the same node. Pick by what the machine in front of you has.

| | Runs on | Distinguishing property |
|---|---|---|
| **Communicator** | Android | Full node in a background service, not a thin client |
| **Web node** | Any browser | One HTML file, zero network requests until you add a bridge |
| **Daemon** | Linux, macOS, Windows | Many bridges in one process |
| **Seed Sheet** | Paper | Rebuilds the guide from any ~K of N fountain QR codes |

<div class="story story-apps" role="list">

<figure class="story-card crate" role="listitem">
  <div class="story-art" aria-hidden="true">
    <svg class="ill ill-phone" viewBox="0 0 280 120" xmlns="http://www.w3.org/2000/svg">
      <rect class="phone-body" x="108" y="12" width="64" height="96" rx="8"/>
      <rect class="phone-screen" x="116" y="28" width="48" height="68" rx="2"/>
      <circle class="phone-dot" cx="140" cy="22" r="2"/>
      <g class="phone-wave">
        <path d="M178 50 Q195 60 178 70" fill="none"/>
        <path d="M186 42 Q212 60 186 78" fill="none"/>
      </g>
    </svg>
  </div>
  <figcaption>
    <strong>📱 SPORE Communicator</strong>
    <span class="story-lead">Android phone — full node in a background service, not a thin client.</span>
  </figcaption>
  <p class="story-cta"><a class="cta-primary" href="https://github.com/sloev/spore/releases/download/rolling/spore-android.apk">⬇ Download APK</a>
  <a class="cta-secondary" href="https://github.com/sloev/spore/releases/tag/rolling">Release notes</a></p>
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
</figure>

<figure class="story-card crate" role="listitem">
  <div class="story-art" aria-hidden="true">
    <svg class="ill ill-html" viewBox="0 0 280 120" xmlns="http://www.w3.org/2000/svg">
      <rect class="doc" x="90" y="20" width="100" height="80" rx="3"/>
      <text class="doc-tag" x="140" y="48" text-anchor="middle">&lt;/&gt;</text>
      <path class="doc-line" d="M110 62 H170 M110 72 H155"/>
      <g class="doc-spark">
        <circle cx="200" cy="40" r="3"/>
        <circle cx="210" cy="55" r="2"/>
        <circle cx="198" cy="68" r="2.5"/>
      </g>
    </svg>
  </div>
  <figcaption>
    <strong>🌐 Single-file web node</strong>
    <span class="story-lead">One HTML file — wasm + UI + transports. Zero network until you add a bridge.</span>
  </figcaption>
  <p class="story-cta"><a class="cta-primary" href="spore-standalone.html">Open standalone</a>
  <a class="cta-secondary" href="demo/">Live demo</a></p>
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
</figure>

<figure class="story-card crate" role="listitem">
  <div class="story-art" aria-hidden="true">
    <svg class="ill ill-daemon" viewBox="0 0 280 120" xmlns="http://www.w3.org/2000/svg">
      <rect class="term" x="40" y="24" width="200" height="72" rx="4"/>
      <text class="term-prompt" x="56" y="56">$</text>
      <text class="term-cmd" x="72" y="56">spore node.yaml</text>
      <rect class="term-cursor" x="200" y="46" width="8" height="14"/>
      <circle class="term-led" cx="56" cy="36" r="3"/>
    </svg>
  </div>
  <figcaption>
    <strong>🖥 Desktop daemon</strong>
    <span class="story-lead">One binary, many bridges — LAN + USB folder + radio in one process.</span>
  </figcaption>
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
</figure>


<figure class="story-card crate" role="listitem">
  <div class="story-art" aria-hidden="true">
    <svg class="ill ill-sheet" viewBox="0 0 280 120" xmlns="http://www.w3.org/2000/svg">
      <rect class="paper" x="70" y="16" width="70" height="90" rx="2"/>
      <rect class="paper paper2" x="140" y="16" width="70" height="90" rx="2"/>
      <g class="qr">
        <rect x="150" y="28" width="50" height="50"/>
        <rect class="qr-hole" x="158" y="36" width="12" height="12"/>
        <rect class="qr-hole" x="180" y="36" width="12" height="12"/>
        <rect class="qr-hole" x="158" y="58" width="12" height="12"/>
      </g>
      <path class="paper-lines" d="M82 36 H128 M82 48 H120 M82 60 H124"/>
    </svg>
  </div>
  <figcaption>
    <strong>🖨 Seed Sheet</strong>
    <span class="story-lead">Printable A4 — any ~K of N fountain QR codes rebuild the guide.</span>
  </figcaption>
  <p class="story-cta"><a class="cta-primary" href="spore-seedsheet.html">Open Seed Sheet</a>
  <a class="cta-secondary" href="continuity.html">Why continuity</a></p>
  <details>
    <summary>Torn sheet still works</summary>
    <p>One side: wire format by hand. Other side: reimplementation guide as
    fountain-coded QR. A stained or partial print can still recover the payload —
    SPORE’s own erasure coding turned on itself. Build it with
    <code>node site/seed/build-seedsheet.mjs</code> after wasm is available.</p>
  </details>
</figure>

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
