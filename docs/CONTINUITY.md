# Continuity — SPORE as a seed

<p align="center">
  <a href="spore-continuity.png"><img src="spore-continuity.png" alt="SPORE continuity on one page" width="820" /></a>
</p>

<p align="center"><em>Poster summary —
<a href="spore-continuity.png">full size</a>. The story cards and sections below
are the living text; the poster can lag.</em></p>

A **spore** is a small capsule that can regrow the whole organism from one
survivor. This page is about the *software* doing the same: one HTML file, one
printout, or one offline bundle is enough to understand, verify, and run a node —
without depending on the same infrastructure the mesh is meant to outlast.

<div class="story story-continuity" role="list">

<figure class="story-card crate" role="listitem">
  <div class="story-art" aria-hidden="true">
    <svg class="ill ill-seed" viewBox="0 0 280 120" xmlns="http://www.w3.org/2000/svg">
      <ellipse class="seed-body" cx="140" cy="64" rx="36" ry="48"/>
      <path class="seed-seam" d="M140 20 Q155 64 140 108"/>
      <g class="seed-sprout">
        <path d="M140 28 Q130 10 120 18" fill="none"/>
        <path d="M140 28 Q150 8 160 16" fill="none"/>
      </g>
    </svg>
  </div>
  <figcaption>
    <strong>Three properties</strong>
    <span class="story-lead">Readable · Reconstructable · Self-propagating</span>
  </figcaption>
  <details>
    <summary>Why “every copy is a seed” is a constraint</summary>
    <p>Most software assumes a registry, CDN, or app store. SPORE’s job is moving
    messages when that infrastructure is degraded — so getting SPORE must not depend
    on it. Understanding, verifying, and rebuilding travel <em>with</em> the node:
    dependency-light core, a spec independent of Rust, paper-friendly formats, and
    the network able to carry its own installer.</p>
  </details>
</figure>

<figure class="story-card crate" role="listitem">
  <div class="story-art" aria-hidden="true">
    <svg class="ill ill-seeds-row" viewBox="0 0 280 120" xmlns="http://www.w3.org/2000/svg">
      <g class="chip" transform="translate(30,40)"><rect width="50" height="40" rx="3"/><text x="25" y="25" text-anchor="middle">HTML</text></g>
      <g class="chip" transform="translate(90,40)"><rect width="50" height="40" rx="3"/><text x="25" y="25" text-anchor="middle">QR</text></g>
      <g class="chip" transform="translate(150,40)"><rect width="50" height="40" rx="3"/><text x="25" y="25" text-anchor="middle">repo</text></g>
      <g class="chip" transform="translate(210,40)"><rect width="50" height="40" rx="3"/><text x="25" y="25" text-anchor="middle">spec</text></g>
    </svg>
  </div>
  <figcaption>
    <strong>What’s a seed today</strong>
    <span class="story-lead">From one browser file to paper QR to a vendored offline build.</span>
  </figcaption>
  <details>
    <summary>Artifact table</summary>
    <table>
      <thead><tr><th>Seed</th><th>Assumes</th><th>Gets you</th></tr></thead>
      <tbody>
        <tr><td><strong>Single-file HTML</strong></td><td>a browser</td><td>full node offline, zero network</td></tr>
        <tr><td><strong>Seed Sheet</strong></td><td>camera + patience</td><td>any ~K of N QR → reimplementation guide</td></tr>
        <tr><td><strong>Repo + vendor/</strong></td><td>Rust toolchain</td><td>daemon + bridges offline after one online vendor step</td></tr>
        <tr><td><strong>SPEC + by-hand examples</strong></td><td>pen, paper</td><td>reimplement in any language</td></tr>
        <tr><td><strong>Pure-Python T0</strong></td><td>Python 3</td><td>receive + verify public mail, no packages</td></tr>
        <tr><td><strong>Armored envelope</strong></td><td>typing</td><td>inject one message from paper or voice</td></tr>
      </tbody>
    </table>
    <p>Links and build commands: <a href="apps.html">Apps &amp; daemons</a>.</p>
  </details>
</figure>

<figure class="story-card crate" role="listitem">
  <div class="story-art" aria-hidden="true">
    <svg class="ill ill-cold" viewBox="0 0 280 120" xmlns="http://www.w3.org/2000/svg">
      <circle class="cold-core" cx="140" cy="60" r="18"/>
      <g class="cold-rings">
        <circle cx="140" cy="60" r="32" fill="none"/>
        <circle cx="140" cy="60" r="46" fill="none"/>
      </g>
      <text class="cold-label" x="140" y="64" text-anchor="middle">go</text>
    </svg>
  </div>
  <figcaption>
    <strong>Cold-start playbooks</strong>
    <span class="story-lead">Only a browser · only radios · only paper · only one laptop offline.</span>
  </figcaption>
  <details>
    <summary>Only a browser</summary>
    <p>Open <code>spore-standalone.html</code>. Add a bridge from the page (WebSocket, WebRTC,
    Nostr, WebTorrent) or the <strong>audio modem</strong> so two laptops pair with no network.
    Get the file from <a href="apps.html">Apps</a>.</p>
  </details>
  <details>
    <summary>Only radios</summary>
    <p>Daemon + matching bridge (<a href="bridges.html">BRIDGES</a>). Router and fragmentation
    are medium-independent; the radio is a thin send/recv shim.</p>
  </details>
  <details>
    <summary>Only paper (and voice)</summary>
    <p>Armor form <code>~S1.&lt;base32&gt;.&lt;checksum&gt;~</code> survives SMS, handwriting, or a
    read-aloud. Seed Sheet fountain QR recovers bulk guides from a partial print.</p>
  </details>
  <details>
    <summary>Only one laptop, no internet</summary>
    <p>Clone + toolchain rebuild offline <strong>after</strong> you vendor while still online:</p>
    <pre><code>./scripts/make-offline-bundle.sh
./scripts/make-offline-bundle.sh --tar</code></pre>
    <p>MSRV is enforced by CI (see Cargo.toml / security findings for the floor).
    <code>Cargo.lock</code> pins versions; <code>vendor/</code> carries the sources. HTML node, printed
    spec, and Seed Sheet need no vendor step.</p>
  </details>
</figure>

<figure class="story-card crate" role="listitem">
  <div class="story-art" aria-hidden="true">
    <svg class="ill ill-trust" viewBox="0 0 280 120" xmlns="http://www.w3.org/2000/svg">
      <path class="shield" d="M140 20 L180 36 V68 Q140 100 100 68 V36 Z"/>
      <path class="shield-check" d="M124 62 L136 74 L160 48" fill="none"/>
    </svg>
  </div>
  <figcaption>
    <strong>Trust without infrastructure</strong>
    <span class="story-lead">Hash is identity. Signatures bind path learning. Seed ≠ full inbox backup.</span>
  </figcaption>
  <details>
    <summary>Anchors</summary>
    <ul>
      <li><strong>Content addressing</strong> — envelope ID is the hash of its bytes; node address is the hash of its key.</li>
      <li><strong>Signed everything</strong> — path learning from signed frames; releases as signed manifests.</li>
      <li><strong>Seed restores identity, not the prekey ring</strong> — carrying the ring keeps old sealed mail readable and lengthens the theft window. Continuity of identity and forward secrecy of content pull opposite ways; you choose.</li>
      <li><strong>Reproducible builds</strong> — compare hashes offline; the single-file node prints the wasm SHA-256 in its footer.</li>
    </ul>
  </details>
</figure>

</div>

## Roadmap

What exists and what’s next:

- [x] **Single-file browser node** — zero network, CI-checked
- [x] **Config-driven daemon + bridge matrix**
- [x] **Language bindings** — Python / Go / JS from one C ABI
- [x] **This continuity guide** with cold-start playbooks
- [x] **Reimplementation guide** ([Rebuild](rebuild.html))
- [x] **Printable Seed Sheet** + fountain-coded QR
- [x] **Tiny reference decoders** (Python, C, shell)
- [◑] **Network carries its own genome** — bootstrap bundle on a well-known topic; still open: signed self-update of the binary
- [ ] **Codex** — full source as hash-stamped booklet
- [ ] **Trust roots on paper** — multi-signature release anchors

## Help re-seed

Keep a copy somewhere the others aren’t: offline disk, phone HTML, printed sheet,
another host. Continuity is redundancy that outlives its sources — and it only
works if the copies are already scattered before they’re needed.
