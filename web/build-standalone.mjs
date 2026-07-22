// Build a single self-contained HTML file that is a complete SPORE node: the
// wasm and every JS module are inlined, so the one file runs offline from a USB
// stick, an email attachment, or a scanned QR bundle — no server, no install, no
// network. A copy of this file is a seed the whole node regrows from.
//
//   cargo build --release --lib --target wasm32-unknown-unknown
//   node web/build-standalone.mjs
//   # -> web/spore-standalone.html
import fs from 'node:fs';
import crypto from 'node:crypto';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const read = (p) => fs.readFileSync(path.join(here, p), 'utf8');

// Inline a module: drop its import lines and `export` keywords so every name
// lands in one shared module scope when concatenated.
function inlineModule(src) {
  return src
    .split('\n')
    .filter((l) => !/^\s*import\s.+from\s.+;?\s*$/.test(l))
    .map((l) => l.replace(/^\s*export\s+/, ''))
    .join('\n');
}

const wasmPath = path.join(here, '../target/wasm32-unknown-unknown/release/spore.wasm');
const wasmBytes = fs.readFileSync(wasmPath);
const wasmB64 = wasmBytes.toString('base64');
const wasmHash = crypto.createHash('sha256').update(wasmBytes).digest('hex');

const modules = [
  inlineModule(read('spore.mjs')),
  inlineModule(read('transports/loopback.mjs')),
  inlineModule(read('transports/websocket.mjs')),
].join('\n\n');

const html = `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8" />
<meta name="viewport" content="width=device-width, initial-scale=1" />
<title>SPORE — self-contained node (one file)</title>
<style>
  :root {
    --bg:#0e1116; --panel:#171b22; --edge:#262c36; --ink:#e6edf3;
    --dim:#8b98a9; --accent:#57c785; --accent2:#4aa3ff; --warn:#e0a030;
    --mono: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  }
  @media (prefers-color-scheme: light) {
    :root { --bg:#f6f8fa; --panel:#fff; --edge:#d8dee4; --ink:#1f2328; --dim:#5b6670; }
  }
  * { box-sizing: border-box; }
  body { margin:0; background:var(--bg); color:var(--ink);
    font:15px/1.55 system-ui,-apple-system,Segoe UI,Roboto,sans-serif; }
  header, main { max-width:960px; margin:0 auto; padding:0 20px; }
  header { padding-top:28px; }
  h1 { margin:0 0 4px; font-size:26px; letter-spacing:-.02em; }
  h1 .s { color:var(--accent); }
  .tag { color:var(--dim); margin:0 0 14px; }
  .bar { display:flex; gap:8px; flex-wrap:wrap; align-items:center; margin:6px 0 20px; }
  .pill { font-size:12px; color:var(--dim); border:1px solid var(--edge);
    border-radius:999px; padding:3px 10px; }
  .grid { display:grid; grid-template-columns:1fr 1fr; gap:16px; }
  @media (max-width:700px){ .grid{ grid-template-columns:1fr; } }
  .card { background:var(--panel); border:1px solid var(--edge); border-radius:12px; padding:16px; }
  .card h2 { margin:0 0 4px; font-size:15px; text-transform:uppercase; letter-spacing:.08em; color:var(--dim); }
  .addr { font-family:var(--mono); font-size:12px; color:var(--accent2); word-break:break-all; margin:0 0 12px; }
  .log { font-family:var(--mono); font-size:12.5px; background:var(--bg); border:1px solid var(--edge);
    border-radius:8px; height:180px; overflow-y:auto; padding:10px; white-space:pre-wrap; }
  .log .rx{color:var(--accent)} .log .tx{color:var(--accent2)} .log .sys{color:var(--dim)} .log .bad{color:var(--warn)}
  .row { display:flex; gap:8px; margin-top:12px; flex-wrap:wrap; }
  input[type=text]{ flex:1; min-width:180px; background:var(--bg); color:var(--ink);
    border:1px solid var(--edge); border-radius:8px; padding:8px 10px; font:inherit; }
  button{ background:var(--accent); color:#05130b; border:0; font-weight:600; border-radius:8px;
    padding:8px 14px; cursor:pointer; font:inherit; }
  button.ghost{ background:transparent; color:var(--ink); border:1px solid var(--edge); }
  .link{ text-align:center; color:var(--dim); margin:22px 0 6px; display:flex; align-items:center; gap:10px; justify-content:center; }
  .link::before,.link::after{ content:""; height:1px; background:var(--edge); flex:1; }
  .note{ color:var(--dim); font-size:13px; }
  a{ color:var(--accent2); }
  code{ font-family:var(--mono); font-size:.9em; background:var(--panel); border:1px solid var(--edge); border-radius:5px; padding:1px 5px; }
  footer{ max-width:960px; margin:0 auto; padding:24px 20px 60px; color:var(--dim); font-size:12.5px; }
</style>
</head>
<body>
<header>
  <h1><span class="s">SPORE</span> — a whole node in one file</h1>
  <p class="tag">This page carries the router (compiled to WebAssembly) and all its
     code inline. It needs no server, no install, and no network to run — open it
     from a USB stick or an offline copy and two full nodes come alive below. A
     single copy of this file is enough to rejoin, or restart, the mesh.</p>
  <div class="bar">
    <span class="pill" id="status">loading…</span>
    <span class="pill">no external requests</span>
    <span class="pill">ed25519 + ChaCha20-Poly1305</span>
    <span class="pill" title="SHA-256 of the embedded wasm">wasm ${wasmHash.slice(0, 16)}…</span>
  </div>
</header>
<main>
  <div class="grid">
    <section class="card">
      <h2>Node A</h2><p class="addr" id="addrA">—</p><div class="log" id="logA"></div>
      <div class="row">
        <input type="text" id="msgA" placeholder="message from A…" value="the dam holds" />
        <button id="sendA">Send →</button>
      </div>
    </section>
    <section class="card">
      <h2>Node B</h2><p class="addr" id="addrB">—</p><div class="log" id="logB"></div>
      <div class="row">
        <input type="text" id="msgB" placeholder="message from B…" value="acknowledged" />
        <button id="sendB">Send →</button>
      </div>
    </section>
  </div>

  <p class="link">in-memory loopback (A ⇄ B)</p>

  <section class="card">
    <h2>Reach other copies</h2>
    <p class="note">Point node A at a WebSocket relay (or another browser tab running
      a relay) and it will exchange signed envelopes with every other copy connected
      there — the same node, now on a real link. Leave blank to stay offline.</p>
    <div class="row">
      <input type="text" id="relay" placeholder="wss://relay.example/spore" />
      <button id="connect" class="ghost">Connect A</button>
    </div>
  </section>

  <section class="card" style="margin-top:16px">
    <h2>Reproduce this seed</h2>
    <p class="note">Save this page and you carry the node with you. The button below
      re-serializes the running page — wasm and all — back into a fresh copy, so one
      seed can make the next.</p>
    <div class="row"><button id="save">Download a copy</button></div>
  </section>
</main>
<footer>
  Self-contained SPORE node · public domain (Unlicense). The embedded wasm is
  SHA-256 <code>${wasmHash}</code>; verify a copy by hashing the decoded
  <code>WASM_B64</code> constant in this file's source.
</footer>

<script type="module">
// ---- inlined SPORE modules (spore.mjs + transports), exports stripped --------
${modules}

// ---- embedded wasm (base64) --------------------------------------------------
const WASM_B64 = "${wasmB64}";
function b64ToBytes(b64) {
  const bin = atob(b64);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}

// ---- UI ----------------------------------------------------------------------
const $ = (id) => document.getElementById(id);
const stamp = () => new Date().toLocaleTimeString();
function log(which, cls, text) {
  const el = $('log' + which);
  const line = document.createElement('div');
  line.className = cls; line.textContent = stamp() + '  ' + text;
  el.appendChild(line); el.scrollTop = el.scrollHeight;
}
const hex = (u8) => Array.from(u8).map((b) => b.toString(16).padStart(2, '0')).join('');

try {
  const spore = await loadSpore(b64ToBytes(WASM_B64));
  const hubA = new Hub(spore.newNode());
  const hubB = new Hub(spore.newNode());
  $('addrA').textContent = 'addr ' + hex(hubA.node.addr());
  $('addrB').textContent = 'addr ' + hex(hubB.node.addr());

  const deliver = (which) => (env) => {
    const ok = spore.verify(env);
    const text = new TextDecoder().decode(spore.payload(env));
    log(which, ok ? 'rx' : 'bad', '◀ received ' + JSON.stringify(text) + '  (sig ' + (ok ? 'OK' : 'BAD') + ')');
  };
  hubA.onDeliver = deliver('A');
  hubB.onDeliver = deliver('B');

  const [ta, tb] = loopbackPair();
  hubA.addTransport(ta); hubB.addTransport(tb);

  const wire = (hub, which, id) => () => {
    const text = $(id).value; if (!text) return;
    hub.send(ZERO_DEST, new TextEncoder().encode(text));
    log(which, 'tx', '▶ sent ' + JSON.stringify(text));
  };
  $('sendA').onclick = wire(hubA, 'A', 'msgA');
  $('sendB').onclick = wire(hubB, 'B', 'msgB');
  $('msgA').addEventListener('keydown', (e) => { if (e.key === 'Enter') $('sendA').click(); });
  $('msgB').addEventListener('keydown', (e) => { if (e.key === 'Enter') $('sendB').click(); });

  $('connect').onclick = () => {
    const url = $('relay').value.trim(); if (!url) return;
    try {
      hubA.addTransport(new WebSocketTransport(url));
      log('A', 'sys', 'connecting to relay ' + url);
    } catch (e) { log('A', 'bad', 'relay error: ' + e.message); }
  };

  $('save').onclick = () => {
    const blob = new Blob(['<!doctype html>\\n' + document.documentElement.outerHTML], { type: 'text/html' });
    const a = document.createElement('a');
    a.href = URL.createObjectURL(blob); a.download = 'spore-standalone.html'; a.click();
    URL.revokeObjectURL(a.href);
  };

  $('status').textContent = 'ready — two nodes live, no network used';
  $('status').style.color = 'var(--accent)';
  log('A', 'sys', 'node ready; loopback linked to B');
  log('B', 'sys', 'node ready; loopback linked to A');
} catch (e) {
  $('status').textContent = 'error: ' + e.message;
  $('status').style.color = 'var(--warn)';
  console.error(e);
}
</script>
</body>
</html>
`;

const outPath = path.join(here, 'spore-standalone.html');
fs.writeFileSync(outPath, html);
const kb = (n) => (n / 1024).toFixed(0) + ' KB';
console.log('wrote', path.relative(path.join(here, '..'), outPath), '(' + kb(Buffer.byteLength(html)) + ')');
console.log('embedded wasm sha256:', wasmHash);
