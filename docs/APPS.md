# Get a node — apps & daemons

Four ways to run the same node. Pick by what the machine in front of you has.

<div class="grid">

<div class="col-6"><div class="card"><div class="card-body">
<h2 class="text-h5">📱 SPORE Communicator</h2>
<p class="text-muted">Android phone — full node in a background service, not a thin client.</p>
<p class="text-muted">Debug-signed, not on the Play Store — sideload it. Verify the
download against the <code>.sha256</code> published alongside it; see
<a href="dev-guide.html#install--verify-a-release">Install &amp; verify a release</a>.</p>
</div>
<div class="card-footer cluster">
<a class="btn" href="https://github.com/sloev/spore/releases/download/rolling/spore-android.apk">⬇ Download APK</a>
<a class="btn btn-cancel" href="https://github.com/sloev/spore/releases/tag/rolling">Release notes</a>
</div></div></div>

<div class="col-6"><div class="card"><div class="card-body">
<h2 class="text-h5">🌐 Single-file web node</h2>
<p class="text-muted">One HTML file — wasm + UI + transports. Zero network until you add a bridge.</p>
</div>
<div class="card-footer cluster">
<a class="btn" href="demo/">Try the web node</a>
<a class="btn btn-cancel" href="demo/" download="spore-standalone.html">Download</a>
</div></div></div>

<div class="col-6"><div class="card"><div class="card-body">
<h2 class="text-h5">🖥 Desktop daemon</h2>
<p class="text-muted">One binary, many bridges — LAN + USB folder + radio in one process. No
prebuilt binary is published yet; build it from source.</p>
</div>
<div class="card-footer cluster">
<a class="btn" href="dev-guide.html#install--verify-a-release">Build from source</a>
<a class="btn btn-cancel" href="bridges.html">Bridge reference</a>
</div></div></div>

<div class="col-6"><div class="card"><div class="card-body">
<h2 class="text-h5">🖨 Seed Sheet</h2>
<p class="text-muted">Printable A4 — any ~23 of 39 fountain QR codes rebuild the guide.</p>
</div>
<div class="card-footer cluster">
<a class="btn" href="spore-seedsheet.html">Open Seed Sheet</a>
<a class="btn btn-cancel" href="continuity.html">Why continuity</a>
</div></div></div>

</div>

Verifying a download, building from source, or working with no network to
crates.io? See <a href="dev-guide.html">Dev guide</a>. Language bindings
(Python / Go / JS over one C ABI) are in <a href="bindings.html">Bindings</a>.
