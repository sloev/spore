// Build a single self-contained HTML file that is a complete, *functional* SPORE
// node: the wasm router and every transport are inlined, so the one file runs
// offline from a USB stick, an email attachment, or a scanned QR bundle — no
// server, no install, no network. Open it and you get one live node you can wire
// to real links at runtime — WebSocket, WebRTC, Nostr, Web Serial, Web Bluetooth,
// an audio modem, a WebTorrent swarm, or a Meshtastic / Reticulum LoRa radio —
// and it remembers its identity and its bridges in the browser's local storage,
// so it comes back the same node next time. A copy of this file is a seed the
// whole node regrows from, and it is also the live demo served on the site.
//
//   cargo build --release --lib --target wasm32-unknown-unknown
//   node web/build-standalone.mjs
//   # -> web/spore-standalone.html
import fs from 'node:fs';
import crypto from 'node:crypto';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { requireHardbrutCss } from './hardbrut-import.mjs';

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

// HARDBRUT (Milestone 7): the framework is imported at build time and inlined,
// not copied. Local checkout first (fast dev loop), vendored copy for CI.
const hardbrutCss = requireHardbrutCss();

// Order matters only for readability; everything shares one scope. kiss before
// the serial/BLE transports that use it.
const modules = [
  inlineModule(read('spore.mjs')),
  inlineModule(read('ui/markdown.mjs')),
  inlineModule(read('transports/kiss.mjs')),
  inlineModule(read('transports/loopback.mjs')),
  inlineModule(read('transports/websocket.mjs')),
  inlineModule(read('transports/webrtc.mjs')),
  inlineModule(read('transports/nostr.mjs')),
  inlineModule(read('transports/webserial.mjs')),
  inlineModule(read('transports/webbluetooth.mjs')),
  inlineModule(read('transports/audio.mjs')),
  inlineModule(read('transports/webtorrent.mjs')),
  inlineModule(read('transports/meshtastic.mjs')),
  inlineModule(read('transports/reticulum.mjs')),
].join('\n\n');

const html = `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8" />
<meta name="viewport" content="width=device-width, initial-scale=1" />
<title>SPORE — a whole node in one file</title>
<style>
/* HARDBRUT (imported at build time) — see above. */
${hardbrutCss}
/* ---- SPORE adapter — app-shell styles HARDBRUT does not define. Uses only
   HARDBRUT tokens; never redefines them. Kept minimal on purpose. ---- */
:root {
  /* Back-compat aliases so existing markup/JS var() lookups keep working.
     --edge/--mono are gone — every call site now spells the real HARDBRUT
     token (--ink/--font-mono) directly. */
  --accent2: var(--accent);
  --yellow: var(--accent);
  --ok: var(--ink);
  --warn: var(--accent);
  --bad: var(--ink);
  --display: var(--font-display);
  --border-w: 3px;
  --control-h: 48px;
  --touch-min: 48px;
}
/* Layout shell: page + persistent identity header centered like .container. */
.persistent-header, header.page-header, main, footer {
  max-width: 1100px; margin: 0 auto; padding: 0 var(--space);
}
.persistent-header {
  background: var(--paper); border: var(--border); box-shadow: var(--shadow);
  padding: var(--space-sm) var(--space); margin: var(--space) auto; position: sticky;
  top: 0; z-index: 100;
}
.header-brand { display: flex; align-items: center; gap: var(--space-sm); margin-bottom: 4px; }
.brand-name { font-weight: 900; font-size: 1.6rem; text-transform: uppercase; letter-spacing: -0.04em; }
.identity-row, .status-line { display: flex; align-items: center; gap: var(--space-sm); flex-wrap: wrap; }
.peer-count, .address, .status-line { font-family: var(--font-mono); font-size: 0.8rem; color: var(--muted); }
.petname { font-weight: 800; }
.avatar-placeholder {
  display: inline-flex; align-items: center; justify-content: center;
  width: 28px; height: 28px; border: var(--border); background: var(--bg);
  font-family: var(--font-display); font-weight: 900; font-size: 0.8rem; flex-shrink: 0;
}

/* The conversation transcript. HARDBRUT's .chat is a bare flex column; the
   scroll box around it is SPORE's, since that's a layout choice HARDBRUT
   doesn't make for you. Cross-cutting notices (bridge/file/group events —
   see logLine()) share this box with real messages, so they get a plain
   centered line rather than a .chat-msg bubble of their own. */
#chat-msgs.chat { height: 240px; overflow-y: auto; border: var(--border);
  padding: var(--space-sm); background: var(--bg); }
.chat-notice { align-self: center; font-family: var(--font-mono); font-size: 0.72rem; color: var(--muted); }
.chat-notice-bad { color: var(--warn); font-weight: 800; }

/* Selected conversation in the chat list — .list-row has no built-in
   persistent-selection state, only :hover/:active. */
.list-row[aria-current="true"] { background: var(--accent); color: var(--accent-ink); }

/* Cards double as the panel shell; HARDBRUT provides .card. */
.card.panel { padding: var(--space); }

/* Rows / inputs are HARDBRUT already; keep SPORE's flex helpers + chips. */
.row { display: flex; gap: var(--space-sm); margin-top: var(--space-sm); flex-wrap: wrap; align-items: center; }
.cnt { font-size: 0.72rem; color: var(--muted); font-family: var(--font-mono); }
.note { color: var(--muted); font-size: 0.85rem; }

/* WYSIWYG toolbar (W12) + embeds. */
.fmt-bar { display: flex; gap: 6px; margin-top: var(--space-sm); }
.fmt-bar button.fmt { min-width: 30px; padding: 4px 8px; font-size: 0.8rem; }
.preview { margin-top: var(--space-sm); padding: var(--space-sm); border: var(--border);
  background: var(--paper); font-size: 0.85rem; word-break: break-word; }
.inline-img { max-width: 100%; max-height: 220px; border: var(--border); display: block; margin: 4px 0; }
.img-embed { display: inline-block; border: var(--border); padding: 6px 10px; background: var(--paper); margin: 2px 0; }
.img-embed .img-name { font-family: var(--font-mono); font-size: 0.7rem; color: var(--muted); }
.file-chip { display: inline-block; cursor: pointer; border: var(--border); background: var(--paper);
  padding: 4px 10px; margin: 2px 0; font-family: var(--font-mono); font-size: 0.75rem; box-shadow: var(--shadow-sm); }
.file-chip:hover { background: var(--accent); color: var(--accent-ink); }

/* Bridges + misc. */
.bridge { border: var(--border); padding: var(--space-sm); margin-top: var(--space-sm); background: var(--paper); }
.bridge .hd { display: flex; align-items: center; gap: var(--space-sm); }
.bridge .ttl { font-weight: 800; flex: 1; }

mark { background: var(--accent); color: var(--accent-ink); }

/* Long unbreakable tokens must not widen the document.
   A node's UI is full of them — a 64-hex wasm digest, an address, a magnet, a
   group invite — and a single one that cannot wrap makes the *whole page* wider
   than the screen. Every other element then lays out in a narrow column with
   dead space to pan into, which reads as a broken app rather than as one long
   string. Measured before this rule at 390px: scrollWidth 599 against a 390
   viewport, all of it the footer's SHA-256.
   Uses overflow-wrap:anywhere rather than word-break:break-all so ordinary
   prose still breaks at spaces and only the unbreakable run is split. */
   The wrap properties alone are not enough: HARDBRUT sets white-space:nowrap
   on code, which defeats them both, so the offender needs it back to normal.
   Scoped to the footer digest rather than applied to every code element,
   because nowrap is the right default for the short inline code in prose. */
code, .mono, .pill { overflow-wrap: anywhere; }
footer code { white-space: normal; word-break: break-all; }

/* Stacked hard shadows need somewhere to land. HARDBRUT throws a solid
   5px 5px 0 shadow with no blur, and the header's two rows sit flush, so COPY's
   shadow is drawn into SHARE's border. It only *looks* wrong on a narrow
   screen: at desktop widths the two buttons are ~700px apart horizontally and
   the shadow falls on empty paper, so this is a spacing bug that only becomes
   visible once both rows compress and the buttons align right above each other. */
.identity-row + .status-line { margin-top: 6px; }
</style>
</head>
<body>
<header class="persistent-header">
  <div class="header-brand">
    <span class="brand-name">SPORE</span>
    <span class="peer-count" id="peer-count">0 peers</span>
  </div>
  <div class="header-identity">
    <div class="identity-row">
      <span class="avatar-placeholder" id="avatar-mono" aria-hidden="true"></span>
      <span class="petname" id="petname">Anonymous</span>
      <span class="address" id="persistent-addr">address\u2026</span>
      <button class="copy-button" id="copy-addr">COPY</button>
    </div>
    <div class="status-line">
      <span class="status-indicator" id="alive-status">starting\u2026</span>
      <span id="compact-status" style="margin-left:auto">0 peers \u00b7 0 stored</span>
      <button class="share-button" id="share-button">SHARE</button>
    </div>
  </div>
</header>
<div class="page-header">
  <h1>A whole node in one file</h1>
  <p class="tag">One file, one node — no server, no network needed to start. Add a
     bridge below to reach other copies.</p>
  <div class="bar">
    <span class="pill" id="status">loading…</span>
    <span class="pill addr" id="addr">addr —</span>
    <span class="pill">ed25519 + ChaCha20-Poly1305</span>
    <span class="pill" title="SHA-256 of the embedded wasm">wasm ${wasmHash.slice(0, 16)}…</span>
  </div>
</div>
<main>
  <!-- Tab bar — HARDBRUT's .tab-list/.tab, aria-selected drives the fill (#9) -->
  <nav class="tab-list" role="tablist">
    <button class="tab" role="tab" data-panel="chats" aria-selected="true">Chats</button>
    <button class="tab" role="tab" data-panel="feed" aria-selected="false">Feed</button>
    <button class="tab" role="tab" data-panel="files" aria-selected="false">Files</button>
    <button class="tab" role="tab" data-panel="bridges" aria-selected="false">Bridges</button>
    <button class="tab" role="tab" data-panel="seed" aria-selected="false">Seed</button>
  </nav>

  <!-- Chats panel: one unified conversation list (W9) -->
  <section class="card panel" id="panel-chats">
    <div style="display:flex;align-items:center;gap:8px;margin-bottom:8px">
      <button id="chat-new" style="font-size:12px;padding:4px 10px">New chat</button>
      <span class="cnt">One-to-one chats, open groups, and private groups — in one list.</span>
    </div>
    <div id="chat-new-picker" style="display:none;border:1px solid var(--ink);padding:10px;margin-bottom:8px">
      <div class="row" style="margin-top:0">
        <input type="text" id="new-dm-hex" placeholder="16-hex address (1:1)" style="max-width:180px" />
        <button id="new-dm-btn" class="ghost">Start 1:1</button>
      </div>
      <div class="row">
        <input type="text" id="new-open-name" placeholder="open group name, e.g. spore/news" style="flex:1" />
        <button id="new-open-btn" class="ghost">Join open group</button>
      </div>
      <div class="row">
        <input type="text" id="new-sealed-name" placeholder="private group name, or paste a spore-group: invite" style="flex:1" />
        <input type="text" id="new-sealed-key" placeholder="64-hex key (blank = generate)" style="font-family:var(--font-mono);font-size:11px;max-width:220px" />
        <button id="new-sealed-btn" class="ghost">Create private</button>
      </div>
    </div>
    <div id="chat-list" style="margin-bottom:10px"></div>
    <!-- Active conversation thread + composer -->
    <div id="chat-view" style="display:none;border:1px solid var(--ink);padding:10px">
      <div id="chat-view-head" style="display:flex;align-items:center;gap:8px;margin-bottom:6px"></div>
      <div id="chat-invite" style="display:none;margin-bottom:6px;padding:8px;border:1px solid var(--warn)"></div>
      <div id="chat-msgs" class="chat"></div>
      <div class="fmt-bar" id="chat-fmt">
        <button type="button" class="fmt" data-fmt="bold" title="bold">B</button>
        <button type="button" class="fmt" data-fmt="italic" title="italic">I</button>
        <button type="button" class="fmt" data-fmt="code" title="code">&lt;/&gt;</button>
        <button type="button" class="fmt" data-fmt="link" title="link">🔗</button>
        <button type="button" class="fmt" data-fmt="attach" title="attach a file">📎</button>
        <button type="button" class="fmt" data-fmt="image" title="attach an image">🖼</button>
      </div>
      <div class="row" id="chat-compose-row">
        <input type="text" id="chat-input" placeholder="message&#x2026;" />
        <button id="chat-send">Send</button>
        <span class="cnt" id="chat-seal-state"></span>
      </div>
      <div id="chat-preview" class="preview" style="display:none"></div>
    </div>
  </section>

  <!-- Feed panel (W10): personal microblog + subscribed feeds -->
  <section class="card panel" id="panel-feed" style="display:none">
    <div style="display:flex;align-items:center;gap:8px;margin-bottom:8px">
      <span class="badge open" style="font-size:10px">PUBLIC</span>
      <span class="cnt">Your feed is public. Anyone who knows your address can read it.</span>
    </div>
    <div class="row" style="margin-top:0">
      <input type="text" id="feed-msg" placeholder="what's happening&#x2026;" />
      <button id="feed-publish">Publish</button>
    </div>
    <div class="fmt-bar" id="feed-fmt">
      <button type="button" class="fmt" data-fmt="bold" title="bold">B</button>
      <button type="button" class="fmt" data-fmt="italic" title="italic">I</button>
      <button type="button" class="fmt" data-fmt="code" title="code">&lt;/&gt;</button>
      <button type="button" class="fmt" data-fmt="link" title="link">🔗</button>
      <button type="button" class="fmt" data-fmt="image" title="attach an image">🖼</button>
    </div>
    <div id="feed-preview" class="preview" style="display:none"></div>
    <div class="row">
      <input type="text" id="feed-follow" placeholder="follow a feed: paste a 16-hex address" style="max-width:220px" />
      <button id="feed-follow-btn" class="ghost">Follow</button>
      <span class="cnt" id="feed-following"></span>
    </div>
    <div id="feed-list" style="display:flex;flex-direction:column;gap:6px;margin-top:10px;max-height:440px;overflow-y:auto"></div>
  </section>

  <!-- Files panel (W5) -->
  <section class="card panel" id="panel-files" style="display:none">
    <details style="margin-bottom:8px" open>
      <summary style="cursor:pointer;font-weight:600;font-size:13px">Publish a file</summary>
      <div class="row">
        <input type="text" id="file-name" placeholder="filename.txt" style="max-width:200px;font-family:var(--font-mono);font-size:12px" />
        <input type="file" id="file-input" style="flex:1" />
      </div>
      <div class="row">
        <button id="file-publish">Publish</button>
        <span id="file-result" class="cnt" style="margin-left:8px"></span>
      </div>
    </details>
    <details style="margin-bottom:8px">
      <summary style="cursor:pointer;font-weight:600;font-size:13px">Fetch by magnet</summary>
      <div class="row">
        <input type="text" id="file-magnet" placeholder="paste 32 hex chars (16-byte magnet id)" style="flex:1;font-family:var(--font-mono);font-size:12px" />
        <button id="file-fetch">Fetch</button>
      </div>
      <div id="file-fetch-result" class="cnt"></div>
    </details>
    <details open>
      <summary style="cursor:pointer;font-weight:600;font-size:13px">Local files</summary>
      <div id="local-files" style="display:flex;flex-direction:column;gap:4px;margin-top:4px;max-height:280px;overflow-y:auto"></div>
    </details>
  </section>

  <!-- Bridges panel -->
  <section class="card panel" id="panel-bridges" style="display:none">
    <details style="margin-bottom:10px;color:var(--muted);font-size:13px">
      <summary style="cursor:pointer">About bridges</summary>
      Each bridge is a real link. The node signs what you send, relays what it hears, and delivers what is addressed to it — across every bridge at once. Some need a permission prompt (mic, serial, Bluetooth) or a relay URL.
    </details>
    <div class="row">
      <select id="btype"></select>
      <button id="add">Add bridge</button>
    </div>
    <div id="bridges"></div>
  </section>

  <!-- Seed panel -->
  <section class="card panel" id="panel-seed" style="display:none">
    <details style="margin-bottom:10px;color:var(--muted);font-size:13px">
      <summary style="cursor:pointer">About seeding</summary>
      The button re-serialises the running page — wasm and all — into a fresh copy. Identity and bridges live in this browser's local storage.
    </details>
    <div class="row">
      <button id="save">Download a copy</button>
      <button id="forget" class="ghost">Forget saved state</button>
    </div>
  </section>
</main>
<footer>
  Self-contained SPORE node · public domain (Unlicense). Embedded wasm SHA-256
  <code>${wasmHash}</code>; verify a copy by hashing the decoded <code>WASM_B64</code>
  constant in this file's source.
</footer>

<script type="module">
// ---- inlined SPORE modules (router + every transport), exports stripped ------
${modules}

// ---- embedded wasm (base64) --------------------------------------------------
const WASM_B64 = "${wasmB64}";
function b64ToBytes(b64) {
  const bin = atob(b64);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}

// ---- tiny UI + storage helpers ----------------------------------------------
const $ = (id) => document.getElementById(id);
const stamp = () => new Date().toLocaleTimeString();
const hexOf = (u8) => Array.from(u8).map((b) => b.toString(16).padStart(2, '0')).join('');
function hexToBytes(h) {
  const u = new Uint8Array(h.length / 2);
  for (let i = 0; i < u.length; i++) u[i] = parseInt(h.substr(i * 2, 2), 16);
  return u;
}
// Was $('log'), pointing at an id the W9 unified-chat refactor renamed to
// chat-msgs without updating this lookup — every call (16+ sites: DM rx/tx,
// feed posts, file transfers, group joins) was throwing on the null result,
// aborting whatever init/handler called it. chat-msgs is the open
// conversation's transcript, so a cross-cutting notice (e.g. fetching a
// magnet) lands there rather than in a dedicated activity log, which no
// longer exists as a separate panel — imperfect scoping, but visible and
// non-crashing beats throwing.
function logLine(cls, text) {
  const el = $('chat-msgs');
  const line = document.createElement('div');
  line.className = 'chat-notice' + (cls === 'bad' ? ' chat-notice-bad' : '');
  line.textContent = stamp() + '  ' + text;
  el.appendChild(line);
  el.scrollTop = el.scrollHeight;
}
const LS = {
  get(k) { try { return localStorage.getItem(k); } catch (e) { return null; } },
  set(k, v) { try { localStorage.setItem(k, v); } catch (e) { /* private mode / file:// */ } },
  del(k) { try { localStorage.removeItem(k); } catch (e) { /* */ } },
};
const K_SEED = 'spore.seed', K_BRIDGES = 'spore.bridges';
// The prekey ring is secret material and must be persisted with the seed: the
// seed restores the identity, the ring restores the ability to open mail already
// sealed to us. Saving only the seed silently loses inbound mail after a rotation.
const K_RING = 'spore.ring';

const K_CONVOS = 'spore.convos';     // unified conversations (W9)
const K_FEEDS = 'spore.feeds';       // followed feed addresses (W10)

let spore, hub;
let saved = [];      // [{type, fields}] persisted bridges

// Unified conversation model (W9). A conversation is one row in the Chats list;
// its 'type' decides the transport: 1:1 DM, open group, or private group.
//   key     — unique storage key (hex addr for DMs, '/name' for groups)
//   type    — 'dm' | 'open' | 'sealed'
//   name    — display name ('/spore/news', or an 8-hex peer label for a DM)
//   addr    — 16-hex address (DMs only)
//   keyHex  — 64-hex PSK (sealed groups only)
//   msgs    — [{text, fromMe, from, ts}]
let convos = {};     // {key: conversation}
let activeChat = null; // current conversation key
let feedItems = [];   // [{from, topicHash, text, ts}]
let followedFeeds = []; // subscribed feed addresses (16-hex strings)
let feedInterval = null;

// ---- boot the one real node (called last, once every def below exists) -------
async function boot() {
  try {
    spore = await loadSpore(b64ToBytes(WASM_B64));

    // Identity: restore a persisted seed, or make one and remember it.
    let restored = false;
    const seedHex = LS.get(K_SEED);
    if (seedHex && /^[0-9a-fA-F]{64}$/.test(seedHex)) {
      hub = new Hub(spore.newNode(hexToBytes(seedHex)));
      restored = true;
      // Prekey ring (§7). Absent or corrupt is survivable — the node keeps its
      // identity and mints a new prekey — so a bad blob is not fatal, it just
      // means mail sealed to an older prekey can no longer be opened.
      const ringHex = LS.get(K_RING);
      if (ringHex && /^[0-9a-fA-F]+$/.test(ringHex) && ringHex.length % 2 === 0) {
        if (!hub.node.restorePrekeyRing(hexToBytes(ringHex))) LS.del(K_RING);
      }
    } else {
      hub = new Hub(spore.newNode());
      LS.set(K_SEED, hexOf(hub.node.seed()));
    }
    // Save the ring now and whenever it may have rotated. Rotation is driven by
    // the router's sweep, so any tick can change it; writing the same bytes twice
    // is cheap next to losing a secret we still need.
    const saveRing = () => { try { LS.set(K_RING, hexOf(hub.node.prekeyRing())); } catch (e) {} };
    saveRing();
    setInterval(saveRing, 60000);
    $('addr').textContent = 'addr ' + hexOf(hub.node.addr());

    // Update persistent header with node identity
    const addrHex = hexOf(hub.node.addr());
    $('persistent-addr').textContent = addrHex.substring(0, 8) + '\u2026';
    $('petname').textContent = restored ? 'Node' : 'Anonymous';
    $('avatar-mono').textContent = restored ? 'N' : 'A';
    const storeSize = hub.node.storeSize ? hub.node.storeSize() : 0;
    $('compact-status').textContent = '0 peers \u00b7 ' + storeSize + ' stored';

    hub.onDeliver = (env) => {
      const ok = spore.verify(env);
      const flags = spore.flags(env);
      const isEncrypted = (flags & FLAG_ENCRYPTED) !== 0;
      const isRatchet = (flags & FLAG_RATCHET) !== 0;
      const sender = spore.src(env);

      // Only 1:1 DMs arrive here (encrypted, addressed to us). Group and feed
      // traffic rides the pub/sub layer and is drained by pollFeed(), so there
      // is no plaintext-broadcast fallback to demux in this handler.
      if (isEncrypted && sender) {
        const opened = hub.node.openDm(sender, env, isRatchet);
        if (opened !== null) {
          let text;
          try { text = new TextDecoder().decode(opened); }
          catch (e) { text = '<' + opened.length + ' bytes>'; }
          const hex = hexOf(sender);
          ensureConvo('dm', hex, hex.slice(0, 8) + '\u2026', { addr: hex });
          convos[hex].msgs.push({ text, fromMe: false, from: hex.slice(0, 8), ts: Date.now() });
          saveConvos();
          renderChatList();
          if (activeChat === hex) renderChatView();
          logLine('rx', 'DM from ' + hex.slice(0, 8) + ': ' + JSON.stringify(text));
        } else {
          logLine('bad', 'could not decrypt DM from ' + hexOf(sender).slice(0, 8) + ' (key may have expired)');
        }
      } else {
        logLine(ok ? 'rx' : 'bad', 'received ' + spore.payload(env).length + ' bytes (sig ' + (ok ? 'OK' : 'BAD') + ') \u2014 not addressed mail');
      }
    };

    // Restore conversations (W9) and re-subscribe groups.
    try { convos = JSON.parse(LS.get(K_CONVOS) || '{}'); } catch (e) { convos = {}; }
    // One-time migration from the pre-W9 stores so a returning node keeps history.
    if (Object.keys(convos).length === 0) {
      try {
        const oldThreads = JSON.parse(LS.get('spore.threads') || '{}');
        for (const [hex, msgs] of Object.entries(oldThreads)) {
          convos[hex] = { type: 'dm', name: hex.slice(0, 8) + '\u2026', addr: hex,
            msgs: msgs.map((m) => ({ text: m.text, fromMe: m.fromMe, from: m.fromMe ? 'me' : hex.slice(0, 8), ts: m.ts })) };
        }
      } catch (e) { /* leave empty */ }
      try {
        const oldSealed = JSON.parse(LS.get('spore.sealedTopics') || '{}');
        for (const [name, keyHex] of Object.entries(oldSealed)) {
          convos['/' + name] = { type: 'sealed', name: '/' + name, keyHex, msgs: [] };
        }
      } catch (e) { /* */ }
    }
    saveConvos();
    for (const c of Object.values(convos)) {
      if (c.type !== 'dm' && c.name) hub.node.subscribe(c.name.replace(/^[/]/, ''));
    }

    // Restore followed feeds (W10).
    try { followedFeeds = JSON.parse(LS.get(K_FEEDS) || '[]'); } catch (e) { followedFeeds = []; }
    for (const addr of followedFeeds) hub.node.subscribe(feedTopicOf(addr));

    renderChatList();
    renderChatView();
    renderFeedFollowing();
    startFeedPoll();
    renderLocalFiles();

    $('status').textContent = restored ? 'ready \u2014 identity restored' : 'ready \u2014 new identity';
    $('status').style.color = 'var(--accent)';

    // Update persistent header status
    $('alive-status').textContent = 'alive';
    $('alive-status').style.color = 'var(--ok)';

    logLine('sys', 'node ready \u2014 ' + (restored ? 'identity restored from local storage' : 'new identity created and saved'));
    wireCompose();
    buildBridgeMenu();

    // Restore saved bridges.
    try { saved = JSON.parse(LS.get(K_BRIDGES) || '[]'); } catch (e) { saved = []; }
    for (const entry of saved.slice()) spinUp(entry, false);
    updateBridgesEmptyState();
  } catch (e) {
    $('status').textContent = 'error: ' + e.message;
    $('status').style.color = 'var(--warn)';
    console.error(e);
  }
}

// ---- conversations (W9) ----------------------------------------------------
function saveConvos() {
  LS.set(K_CONVOS, JSON.stringify(convos));
}

function ensureConvo(type, key, name, extra) {
  if (!convos[key]) convos[key] = Object.assign({ type, name, msgs: [] }, extra);
}

// Personal-feed topic naming (W10): feed:: + addr, distinct from the bare
// group-chat topic names so the two namespaces can never collide (see
// docs/ROADMAP.md's "personal feed" note).
function feedTopicOf(addr) {
  return 'feed::' + addr;
}

// Plain text empty state — no mascot; SPORE's only brand mark is the wordmark.
function emptyState(message, action1, action2) {
  return '<div class="empty-state" style="text-align:center;padding:20px 0;color:var(--muted)">' +
    '<p style="margin:4px 0 12px;font-size:13px">' + message + '</p>' +
    '<div style="display:flex;gap:12px;justify-content:center">' +
    (action1 ? '<button class="x" style="text-transform:uppercase">' + action1 + '</button>' : '') +
    (action2 ? '<button class="ghost" style="text-transform:uppercase">' + action2 + '</button>' : '') +
    '</div></div>';
}

// Track bridge count and show the empty state when none are active.
function updateBridgesEmptyState() {
  const container = $('bridges');
  const existing = container.querySelector('.empty-state');
  const hasBridges = container.children.length > (existing ? 1 : 0);
  if (hasBridges) {
    if (existing) existing.remove();
  } else {
    if (!existing) {
      const h = document.createElement('div');
      h.innerHTML = emptyState('No bridges yet. Add one above.', 'ADD BRIDGE');
      const btn = h.querySelector('.x');
      if (btn) btn.addEventListener('click', () => $('add').focus());
      container.prepend(h);
    }
  }
}

// Type badge metadata: color + label for each conversation kind.
const CONVO_BADGES = {
  dm:     { label: '1:1',   color: 'var(--accent2)' },
  open:   { label: 'OPEN',  color: 'var(--ok)' },
  sealed: { label: 'PRIVATE', color: 'var(--accent)' },
};

// Markdown + magnet + attachment rendering (W11/W12) live in web/ui/markdown.mjs,
// inlined as a module. It holds escapeHtml / mdInline / mdWithMagnet /
// mdWithAttachments so the regex backslashes are not rewritten by the template
// literal that carries this script. mdWithAttachments parses the Appendix A
// markers (image ![name](spore:<magnet>) and file 📎 name | spore:<magnet> | mime).


function renderChatList() {
  const el = $('chat-list');
  const entries = Object.entries(convos).sort((a, b) => {
    const lastA = a[1].msgs.length ? a[1].msgs[a[1].msgs.length - 1].ts : 0;
    const lastB = b[1].msgs.length ? b[1].msgs[b[1].msgs.length - 1].ts : 0;
    return lastB - lastA;
  });
  if (!entries.length) {
    el.innerHTML = emptyState('No chats yet. Start a 1:1, join an open group, or create a private group.');
    return;
  }
  el.innerHTML = entries.map(([key, c]) => {
    const b = CONVO_BADGES[c.type] || CONVO_BADGES.dm;
    const last = c.msgs.length ? c.msgs[c.msgs.length - 1] : null;
    const preview = last ? ((last.fromMe ? 'You: ' : '') + (last.text.length > 40 ? last.text.slice(0, 40) + '\u2026' : last.text)) : '';
    const isActive = key === activeChat;
    return '<div class="list-row" data-key="' + escapeHtml(key) + '"' + (isActive ? ' aria-current="true"' : '') + '>' +
      '<div class="list-row-avatar" style="color:' + b.color + ';border-color:' + b.color + '">' + b.label.slice(0, 1) + '</div>' +
      '<div class="list-row-body">' +
        '<span class="list-row-title">' + escapeHtml(c.name) + '</span>' +
        '<span class="list-row-subtitle">' + escapeHtml(preview) + '</span>' +
      '</div>' +
      (last ? '<span class="list-row-meta">' + new Date(last.ts).toLocaleTimeString() + '</span>' : '') +
    '</div>';
  }).join('');
  for (const row of el.querySelectorAll('.list-row')) {
    row.onclick = () => { activeChat = row.dataset.key; renderChatList(); renderChatView(); };
  }
}

function renderChatView() {
  const view = $('chat-view');
  const head = $('chat-view-head');
  const msgsEl = $('chat-msgs');
  const sealState = $('chat-seal-state');
  if (!activeChat || !convos[activeChat]) {
    view.style.display = 'none';
    return;
  }
  const c = convos[activeChat];
  view.style.display = 'block';
  const b = CONVO_BADGES[c.type] || CONVO_BADGES.dm;
  head.innerHTML = '<span class="badge" style="font-size:10px;border-color:' + b.color + ';color:' + b.color + '">' + b.label + '</span>' +
    '<span style="font-family:var(--font-mono);font-size:13px;font-weight:600">' + escapeHtml(c.name) + '</span>' +
    (c.type === 'open' ? '<span class="cnt">anyone can read this</span>' : '') +
    (c.type === 'sealed' ? '<span class="cnt">anyone with the key can read this</span>' +
      '<button id="chat-invite-btn" class="ghost" style="margin-left:auto">Invite</button>' : '') +
    (c.type === 'dm' ? '<span class="cnt" id="dm-seal-hint"></span>' : '');

  // Private-group invite (W7). Hidden until asked for: the string *is* the key,
  // so it must not sit on screen where a screenshot or a shoulder collects it.
  const inviteBox = $('chat-invite');
  inviteBox.style.display = 'none';
  inviteBox.innerHTML = '';
  if (c.type === 'sealed') {
    $('chat-invite-btn').onclick = () => {
      if (inviteBox.style.display === 'block') { inviteBox.style.display = 'none'; return; }
      const line = spore.groupInviteEncode(c.name.replace(/^[/]/, ''), hexToBytes(c.keyHex));
      inviteBox.style.display = 'block';
      inviteBox.innerHTML =
        '<div class="cnt" style="margin-bottom:4px"><strong>This link is the key.</strong> ' +
        'Anyone who reads it can read this group — a screenshot or a forwarded copy works ' +
        'as well as being told. It cannot be recalled: a copy already taken keeps opening ' +
        'everything sealed under this key. You can move the group to a fresh key later and ' +
        'hand it only to the people you still want, but SPORE keeps no member list, so ' +
        'knowing who those are is yours to track — nothing here can verify it.</div>' +
        '<textarea readonly rows="2" style="width:100%;font-family:var(--font-mono);font-size:11px">' +
        escapeHtml(line) + '</textarea>' +
        '<button id="chat-invite-copy" class="ghost" style="margin-top:4px">Copy</button>';
      const ta = inviteBox.querySelector('textarea');
      ta.onclick = () => ta.select();
      $('chat-invite-copy').onclick = () => {
        ta.select();
        navigator.clipboard?.writeText(line).catch(() => {});
        logLine('sys', 'invite copied — it carries the key, so share it the way you would the key');
      };
    };
  }

  if (!c.msgs.length) {
    msgsEl.innerHTML = '<span class="cnt" style="display:block;text-align:center;padding:20px 0">No messages yet.</span>';
  } else {
    msgsEl.innerHTML = c.msgs.map((m) => {
      const selfCls = m.fromMe ? ' chat-msg-self' : '';
      const initial = (m.fromMe ? 'Me' : (m.from || '?')).slice(0, 1).toUpperCase();
      const who = m.fromMe ? '' : escapeHtml(m.from || '?') + ': ';
      return '<div class="chat-msg' + selfCls + '">' +
        '<div class="chat-avatar">' + escapeHtml(initial) + '</div>' +
        '<div class="chat-bubble">' + who + mdWithAttachments(m.text) +
        '<span class="chat-time">' + new Date(m.ts).toLocaleTimeString() + '</span></div></div>';
    }).join('');
  }
  msgsEl.scrollTop = msgsEl.scrollHeight;
  // Wire any magnet links / embeds in the history to fetch on click.
  for (const a of msgsEl.querySelectorAll('.magnet-link')) {
    a.onclick = (e) => { e.preventDefault(); fetchMagnetHex(a.dataset.magnet, a); };
  }
  hydrateEmbeds(msgsEl);

  // Composer: for 1:1, mirror the live seal state honestly (never a bare padlock).
  if (c.type === 'dm') {
    sealState.textContent = '';
    const hint = $('dm-seal-hint');
    if (c.addr) {
      const dest = hexToBytes(c.addr);
      const canSeal = hub.node.canSealTo(dest);
      if (hint) {
        hint.textContent = canSeal ? '\u2705 sealed' : '\u26a0 no prekey \u2014 will be sent in the clear';
        hint.style.color = canSeal ? 'var(--ok)' : 'var(--warn)';
      }
    }
  } else {
    sealState.textContent = '';
  }
}

function sendChatMessage() {
  if (!activeChat || !convos[activeChat]) return;
  const c = convos[activeChat];
  const text = $('chat-input').value.trim();
  if (!text) return;

  if (c.type === 'dm') {
    if (!c.addr) { logLine('bad', 'no address for this chat'); return; }
    const dest = hexToBytes(c.addr);
    const canSeal = hub.node.canSealTo(dest);
    const payload = new TextEncoder().encode(text);
    const { forwards } = hub.node.sendDirect(dest, payload);
    hub._dispatch(forwards, null);
    c.msgs.push({ text, fromMe: true, from: 'me', ts: Date.now() });
    saveConvos();
    renderChatList(); renderChatView();
    logLine('tx', 'DM to ' + c.addr.slice(0, 8) + (canSeal ? ' (sealed)' : ' (cleartext)'));
  } else if (c.type === 'open') {
    const topic = c.name.replace(/^[/]/, '');
    hub.node.publish(topic, new TextEncoder().encode(text));
    c.msgs.push({ text, fromMe: true, from: 'me', ts: Date.now() });
    saveConvos();
    renderChatList(); renderChatView();
    logLine('tx', 'posted to /' + topic);
  } else if (c.type === 'sealed') {
    const topic = c.name.replace(/^[/]/, '');
    const keyBytes = hexToBytes(c.keyHex);
    const ct = spore.topicSeal(new TextEncoder().encode(text), keyBytes);
    hub.node.publish(topic, ct);
    c.msgs.push({ text, fromMe: true, from: 'me', ts: Date.now() });
    saveConvos();
    renderChatList(); renderChatView();
    logLine('tx', 'posted (sealed) to /' + topic);
  }
  $('chat-input').value = '';
}

// ---- feed (W10): personal microblog + subscribed feeds ---------------------
function renderFeedFollowing() {
  $('feed-following').textContent = followedFeeds.length
    ? 'following ' + followedFeeds.length + ' feed' + (followedFeeds.length === 1 ? '' : 's')
    : 'you are not following anyone yet';
}

function startFeedPoll() {
  if (feedInterval) clearInterval(feedInterval);
  feedInterval = setInterval(() => {
    if (!hub) return;
    const events = hub.node.pollFeed();
    for (const ev of events) {
      // The author is the authenticated sender; a null 'from' is never claimed.
      const from = ev.from ? hexOf(ev.from) : null;
      const topicHash = hexOf(ev.topic);

      // Demux by topic: a group message lands in its conversation; a feed
      // event lands in the merged timeline. Both ride the same pub/sub layer.
      const convo = convoForTopicHash(topicHash);
      if (convo) {
        let text;
        if (convo.type === 'sealed' && convo.keyHex) {
          const opened = spore.topicOpen(ev.data, hexToBytes(convo.keyHex));
          text = opened !== null ? new TextDecoder().decode(opened) : '(could not decrypt \u2014 wrong key?)';
        } else {
          try { text = new TextDecoder().decode(ev.data); } catch (e) { text = '<' + ev.data.length + ' bytes>'; }
        }
        convo.msgs.push({ text, fromMe: false, from: from ? from.slice(0, 8) : '?', ts: Date.now() });
        saveConvos();
        renderChatList();
        if (activeChat !== null && convos[activeChat] === convo) renderChatView();
        logLine('rx', convo.name + ' <' + (from ? from.slice(0, 8) : '?') + '>: ' + JSON.stringify(text));
      } else {
        // A feed post (or an event for a topic we no longer follow).
        let text;
        try { text = new TextDecoder().decode(ev.data); } catch (e) { text = '<' + ev.data.length + ' bytes>'; }
        feedItems.unshift({ from, topic: ev.topic, text, ts: Date.now() });
      }
    }
    if (feedItems.length > 200) feedItems = feedItems.slice(0, 200);
    renderFeed();
  }, 2000);
}

// Map a topic hash (hex of the 8-byte topic address) back to the open/sealed
// conversation it belongs to, if any.
function convoForTopicHash(topicHash) {
  for (const c of Object.values(convos)) {
    if (c.type === 'dm') continue;
    const name = c.name.replace(/^[/]/, '');
    if (hexOf(spore.topicOf(name)) === topicHash) return c;
  }
  return null;
}

function renderFeed() {
  const el = $('feed-list');
  if (!feedItems.length) {
    el.innerHTML = '<span class="cnt" style="padding:20px;display:block;text-align:center">Nothing yet. Publish above, or follow someone.</span>';
    return;
  }
  el.innerHTML = feedItems.slice(0, 60).map((item) => {
    const author = item.from ? item.from.slice(0, 8) : 'anonymous';
    const isMine = item.from === hexOf(hub.node.addr());
    return '<div style="display:flex;align-items:flex-start;gap:8px;padding:8px;border:1px solid var(--ink);font-size:12.5px">' +
      '<span class="badge open" style="font-size:9px;flex-shrink:0;margin-top:1px;border-color:' + (isMine ? 'var(--accent2)' : 'var(--ok)') + ';color:' + (isMine ? 'var(--accent2)' : 'var(--ok)') + '">' + (isMine ? 'you' : author) + '</span>' +
      '<span style="flex:1;color:var(--ink);word-break:break-word">' + mdWithAttachments(item.text) + '</span>' +
      '<span class="cnt" style="flex-shrink:0">' + new Date(item.ts).toLocaleTimeString() + '</span>' +
    '</div>';
  }).join('');
  for (const a of el.querySelectorAll('.magnet-link')) {
    a.onclick = (e) => { e.preventDefault(); fetchMagnetHex(a.dataset.magnet, a); };
  }
  hydrateEmbeds(el);
}

// hub.node.fileBytes() is complete-or-null, not have/count \u2014 there is no
// chunk-level progress signal in the wasm API to show a determinate bar
// against (that would need new exports, not just a new HARDBRUT primitive).
// A spinner is the honest fit for "waiting, duration unknown": swapped into
// triggerEl (the link/chip the person clicked) for the duration of the poll.
const SPINNER_HTML = '<span class="spinner"><span></span><span></span><span></span><span></span></span>';
function fetchMagnetHex(hex, triggerEl) {
  if (!/^[0-9a-fA-F]{32}$/.test(hex)) { logLine('bad', 'bad magnet'); return; }
  const magnet = hexToBytes(hex);
  logLine('sys', 'fetching magnet ' + hex.slice(0, 8) + '\u2026');
  const restore = triggerEl ? triggerEl.innerHTML : null;
  if (triggerEl) triggerEl.innerHTML = SPINNER_HTML;
  const done = () => { clearInterval(poll); if (triggerEl) triggerEl.innerHTML = restore; };
  const poll = setInterval(() => {
    const bytes = hub.node.fileBytes(magnet);
    if (bytes) {
      done();
      const name = hub.node.fileName(magnet) || 'unnamed.bin';
      const blob = new Blob([bytes]);
      const a = document.createElement('a');
      a.href = URL.createObjectURL(blob); a.download = name; a.click();
      URL.revokeObjectURL(a.href);
      logLine('sys', 'downloaded ' + name + ' (' + bytes.length + ' bytes)');
      renderLocalFiles();
    }
  }, 1000);
  setTimeout(done, 30000);
}

// ---- embeds (W12): inline image thumbnails + file chips ---------------------

// After a render inserts .img-embed / .file-chip spans, hydrate them: turn an
// image embed into an inline <img> once its bytes are local, and make file chips
// click-to-download. Purely additive — until the bytes arrive the span shows
// the filename, not a broken image.
function hydrateEmbeds(scope) {
  for (const img of scope.querySelectorAll('.img-embed')) {
    const magnet = hexToBytes(img.dataset.magnet);
    const name = img.dataset.name;
    // Poll for the bytes (they arrive through the mesh transfer path); on hit,
    // swap the placeholder for an <img> backed by a data: URL.
    const poll = setInterval(() => {
      const bytes = hub.node.fileBytes(magnet);
      if (bytes) {
        clearInterval(poll);
        const url = URL.createObjectURL(new Blob([bytes]));
        const el = document.createElement('img');
        el.src = url;
        el.alt = name;
        el.className = 'inline-img';
        el.onerror = () => { img.textContent = name + ' (won\u2019t load)'; };
        img.replaceWith(el);
      }
    }, 1000);
    setTimeout(() => clearInterval(poll), 30000);
  }
  for (const chip of scope.querySelectorAll('.file-chip')) {
    chip.onclick = (e) => { e.preventDefault(); fetchMagnetHex(chip.dataset.magnet, chip); };
  }
}

// ---- WYSIWYG toolbar (W12) --------------------------------------------------

// Apply a markdown span to an <input>, wrapping the current selection (or the
// word under the cursor for code). Kept out of the template-literal hazards by
// using \x60 for the code fences.
function applyFmt(input, kind) {
  const el = input;
  const start = el.selectionStart ?? el.value.length;
  const end = el.selectionEnd ?? start;
  let text = el.value;

  const wrap = (a, b) => {
    el.value = text.slice(0, start) + a + text.slice(start, end) + b + text.slice(end);
    const sel = start + a.length;
    el.focus(); el.setSelectionRange(sel, sel + (end - start));
  };

  switch (kind) {
    case 'bold': wrap('**', '**'); break;
    case 'italic': wrap('*', '*'); break;
    case 'code': wrap('\x60', '\x60'); break;
    case 'link': {
      const label = text.slice(start, end) || 'link';
      wrap('[', '](https://)');
      break;
    }
    case 'attach':
    case 'image': {
      // Stage a real file, publish it, and insert the canonical marker:
      //   image -> ![name](spore:<magnet>)
      //   file  -> 📎 name | spore:<magnet> | mime   (Appendix A)
      const input = document.createElement('input');
      input.type = 'file';
      input.onchange = async () => {
        const f = input.files && input.files[0];
        if (!f) return;
        const buf = new Uint8Array(await f.arrayBuffer());
        const magnet = hexOf(hub.node.publishFile(f.name, buf));
        const marker = kind === 'image'
          ? '![' + f.name + '](spore:' + magnet + ')'
          : '📎 ' + f.name + ' | spore:' + magnet + ' | ' + (f.type || 'application/octet-stream');
        const at = el.selectionStart ?? el.value.length;
        el.value = el.value.slice(0, at) + ' ' + marker + ' ' + el.value.slice(at);
        el.dispatchEvent(new Event('input'));
        logLine('tx', (kind === 'image' ? 'attached image ' : 'attached file ') + f.name + ' (' + buf.length + ' bytes)');
      };
      input.click();
      break;
    }
  }
  el.dispatchEvent(new Event('input'));
}

// Bind a toolbar (bold/italic/code/link/attach/image) to an input + live preview.
function wireFormatting(toolbarId, inputId, previewId) {
  const input = $(inputId);
  const preview = $(previewId);
  if (!input || !toolbarId) return;
  const bar = $(toolbarId);
  if (!bar) return;
  for (const btn of bar.querySelectorAll('button.fmt')) {
    btn.addEventListener('click', () => applyFmt(input, btn.dataset.fmt));
  }
  input.addEventListener('input', () => {
    const t = input.value;
    if (!t) { if (preview) preview.style.display = 'none'; return; }
    if (preview) {
      preview.style.display = 'block';
      preview.innerHTML = mdWithAttachments(t);
      for (const a of preview.querySelectorAll('.magnet-link')) {
        a.onclick = (e) => { e.preventDefault(); fetchMagnetHex(a.dataset.magnet, a); };
      }
      hydrateEmbeds(preview);
    }
  });
}

// ---- compose wiring (W9) --------------------------------------------------
function wireCompose() {
  const doSend = () => sendChatMessage();
  $('chat-send').onclick = doSend;
  $('chat-input').addEventListener('keydown', (e) => { if (e.key === 'Enter') doSend(); });
  wireFormatting('chat-fmt', 'chat-input', 'chat-preview');
  wireFormatting('feed-fmt', 'feed-msg', 'feed-preview');

  // New-chat picker toggle.
  $('chat-new').onclick = () => {
    const p = $('chat-new-picker');
    p.style.display = p.style.display === 'none' ? 'block' : 'none';
  };

  // 1:1
  $('new-dm-btn').onclick = () => {
    const hex = $('new-dm-hex').value.trim().replace(/[^0-9a-fA-F]/g, '');
    if (hex.length !== 16) { logLine('bad', 'a 1:1 address is 16 hex chars'); return; }
    ensureConvo('dm', hex, hex.slice(0, 8) + '\u2026', { addr: hex });
    saveConvos();
    activeChat = hex;
    $('new-dm-hex').value = '';
    renderChatList(); renderChatView();
  };

  // Open group
  $('new-open-btn').onclick = () => {
    const name = $('new-open-name').value.trim().replace(/^[/]/, '');
    if (!name) { logLine('bad', 'enter a group name'); return; }
    hub.node.subscribe(name);
    const key = '/' + name;
    ensureConvo('open', key, '/' + name);
    saveConvos();
    activeChat = key;
    $('new-open-name').value = '';
    renderChatList(); renderChatView();
    logLine('sys', 'joined open group /' + name);
  };

  // Private (sealed) group
  $('new-sealed-btn').onclick = () => {
    const raw = $('new-sealed-name').value.trim();
    let name, keyHex;
    // A pasted spore-group: invite carries both halves, so it wins over the
    // two fields. Anything that merely looks like one and fails its checksum
    // is refused rather than treated as a group name — joining a mistyped key
    // would silently open a room with one member in it.
    if (raw.startsWith('spore-group:')) {
      const g = spore.groupInviteDecode(raw);
      if (!g) { logLine('bad', 'that invite is damaged — ask for it again'); return; }
      name = g.name.replace(/^[/]/, '');
      keyHex = hexOf(g.key);
      if (!name) { logLine('bad', 'that invite has no group name'); return; }
    } else {
      name = raw.replace(/^[/]/, '');
      if (!name) { logLine('bad', 'enter a group name'); return; }
      keyHex = $('new-sealed-key').value.trim().replace(/[^0-9a-fA-F]/g, '');
      if (!keyHex) { const k = new Uint8Array(32); crypto.getRandomValues(k); keyHex = hexOf(k); $('new-sealed-key').value = keyHex; }
      if (keyHex.length !== 64) { logLine('bad', 'the key must be 64 hex chars (32 bytes)'); return; }
    }
    hub.node.subscribe(name);
    const key = '/' + name;
    ensureConvo('sealed', key, '/' + name, { keyHex });
    saveConvos();
    activeChat = key;
    $('new-sealed-name').value = '';
    renderChatList(); renderChatView();
    logLine('sys', 'private group /' + name + ' ready \u2014 anyone with the key can read');
  };

  // Feed follow (W10): subscribe to a peer's feed::<addr>.
  $('feed-follow-btn').onclick = () => {
    const addr = $('feed-follow').value.trim().replace(/[^0-9a-fA-F]/g, '');
    if (addr.length !== 16) { logLine('bad', 'a feed address is 16 hex chars'); return; }
    if (followedFeeds.includes(addr)) { $('feed-follow').value = ''; return; }
    hub.node.subscribe(feedTopicOf(addr));
    followedFeeds.push(addr);
    LS.set(K_FEEDS, JSON.stringify(followedFeeds));
    renderFeedFollowing();
    logLine('sys', 'following feed ' + addr.slice(0, 8) + '\u2026');
    $('feed-follow').value = '';
  };

  // Feed publish (W10): publish to your own feed::<your_addr>.
  $('feed-publish').onclick = () => {
    const text = $('feed-msg').value.trim();
    if (!text) return;
    const myAddr = hexOf(hub.node.addr());
    hub.node.publish(feedTopicOf(myAddr), new TextEncoder().encode(text));
    feedItems.unshift({ from: myAddr, topic: spore.topicOf(feedTopicOf(myAddr)), text, ts: Date.now() });
    if (feedItems.length > 200) feedItems = feedItems.slice(0, 200);
    renderFeed();
    logLine('tx', 'published to your feed (' + myAddr.slice(0, 8) + '\u2026)');
    $('feed-msg').value = '';
  };
  $('feed-msg').addEventListener('keydown', (e) => { if (e.key === 'Enter') $('feed-publish').onclick(); });
}

// ---- bridge registry ---------------------------------------------------------
// flags: gesture=needs a user gesture to (re)connect; autoReconnect=safe to open
// on load without a gesture; persist=remember across reloads (default true).
const RADIO_FIELDS = [
  { k: 'freq', ph: 'freq MHz', val: '867.2' },
  { k: 'bw', ph: 'bandwidth kHz', val: '125' },
  { k: 'sf', ph: 'spreading factor', val: '8' },
  { k: 'cr', ph: 'coding rate', val: '5' },
  { k: 'tx', ph: 'TX dBm', val: '0' },
];
const radioOpts = (f) => ({
  freqHz: Math.round(parseFloat(f.freq) * 1e6),
  bwHz: Math.round(parseFloat(f.bw) * 1e3),
  sf: parseInt(f.sf, 10),
  cr: parseInt(f.cr, 10),
  txDbm: parseInt(f.tx, 10),
});

const BRIDGES = {
  loopback: {
    label: 'Loopback — offline self-test (spawns a 2nd node in this page)',
    avail: () => true, autoReconnect: true,
    make: () => spawnPartner(),
  },
  websocket: {
    label: 'WebSocket relay',
    fields: [{ k: 'url', ph: 'wss://relay.example/spore' }],
    avail: () => 'WebSocket' in window, autoReconnect: true, watchWs: (t) => t.ws,
    make: (f) => new WebSocketTransport(f.url),
  },
  webtorrent: {
    label: 'WebTorrent swarm — browser P2P via tracker rendezvous',
    fields: [{ k: 'name', ph: 'swarm name', val: 'spore/public' }],
    avail: () => 'RTCPeerConnection' in window && 'WebSocket' in window, autoReconnect: true,
    make: (f) => WebTorrentTransport.join(f.name),
    onmeta: (t, set) => { t.onpeer = (n) => set(n + ' peer' + (n === 1 ? '' : 's')); },
  },
  nostr: {
    label: 'Nostr relay (kind-30078, tag spore-v1)',
    fields: [{ k: 'url', ph: 'wss://relay.damus.io' }],
    avail: () => 'WebSocket' in window, autoReconnect: true, watchWs: (t) => t.ws,
    make: (f) => new NostrTransport(f.url, window.nostr ? (e) => window.nostr.signEvent(e) : null),
  },
  meshtastic_serial: {
    label: 'Meshtastic — USB serial (LoRa mesh)',
    avail: () => !!navigator.serial, gesture: true, setAddr: true,
    make: () => MeshtasticSerialTransport.open(),
  },
  meshtastic_ble: {
    label: 'Meshtastic — Bluetooth (LoRa mesh)',
    avail: () => !!navigator.bluetooth, gesture: true, setAddr: true,
    make: () => MeshtasticBLETransport.open(),
  },
  reticulum_serial: {
    label: 'Reticulum / RNode — USB serial (LoRa)',
    fields: RADIO_FIELDS,
    avail: () => !!navigator.serial, gesture: true,
    make: (f) => ReticulumSerialTransport.open(radioOpts(f)),
  },
  reticulum_ble: {
    label: 'Reticulum / RNode — Bluetooth (LoRa)',
    fields: RADIO_FIELDS,
    avail: () => !!navigator.bluetooth, gesture: true,
    make: (f) => ReticulumBLETransport.open(radioOpts(f)),
  },
  webserial: {
    label: 'Web Serial — generic KISS TNC',
    avail: () => !!navigator.serial, gesture: true,
    make: () => WebSerialTransport.open({ baudRate: 115200 }),
  },
  webbluetooth: {
    label: 'Web Bluetooth — generic Nordic UART (KISS)',
    avail: () => !!navigator.bluetooth, gesture: true,
    make: () => WebBluetoothTransport.open(),
  },
  audio: {
    label: 'Audio modem — mic + speaker, 16-FSK (interops with the Rust bridge)',
    avail: () => !!(navigator.mediaDevices && navigator.mediaDevices.getUserMedia), gesture: true,
    make: () => AudioModemTransport.open(),
  },
  webrtc: {
    label: 'WebRTC — direct peer, copy/paste invite (no server)',
    avail: () => 'RTCPeerConnection' in window, manual: true, persist: false,
  },
};

function buildBridgeMenu() {
  const sel = $('btype');
  for (const [key, b] of Object.entries(BRIDGES)) {
    const opt = document.createElement('option');
    opt.value = key;
    // Keep unsupported entries *selectable* (a disabled <option> makes the
    // dropdown snap back to its previous value when clicked); we explain why on
    // Add instead.
    opt.textContent = b.label + (b.avail() ? '' : ' — unsupported here');
    sel.appendChild(opt);
  }
  $('add').onclick = () => spinUp({ type: sel.value, fields: {} }, true);
}

// A styled row for one live bridge. Returns handles the caller updates.
function bridgeRow(title) {
  const el = document.createElement('div');
  el.className = 'bridge';
  const hd = document.createElement('div');
  hd.className = 'hd';
  const ttl = document.createElement('span');
  ttl.className = 'ttl';
  ttl.textContent = title;
  const badge = document.createElement('span');
  badge.className = 'badge';
  badge.textContent = 'starting';
  const cnt = document.createElement('span');
  cnt.className = 'cnt';
  const rm = document.createElement('button');
  rm.className = 'x';
  rm.textContent = '×';
  hd.append(ttl, cnt, badge, rm);
  const bd = document.createElement('div');
  bd.className = 'bd';
  el.append(hd, bd);
  $('bridges').prepend(el);
  return {
    body: bd,
    remove: rm,
    _t: null,
    setState: (s, kind) => { badge.textContent = s; badge.className = 'badge' + (kind ? ' ' + kind : ''); },
    setMeta: (s) => { cnt.textContent = s; },
    destroy: () => el.remove(),
  };
}

// Attach a transport to the hub, counting frames each way for the row.
function attachCounted(t, ui) {
  hub.addTransport(t);
  let inN = 0, outN = 0;
  const upd = () => ui.setMeta('↑' + outN + ' ↓' + inN);
  const hubRx = t.receive;
  t.receive = (b) => { inN++; upd(); return hubRx(b); };
  const origSend = t.send.bind(t);
  t.send = (b) => { outN++; upd(); return origSend(b); };
  upd();
}

// Best-effort live connection state for a bridge.
function wireState(t, b, ui) {
  ui._t = t;
  ui.setState('open', 'open');
  if (b.onmeta) b.onmeta(t, ui.setMeta);
  const ws = b.watchWs && b.watchWs(t);
  if (ws) {
    const set = () => {
      const s = ['connecting', 'open', 'closing', 'closed'][ws.readyState] || '?';
      ui.setState(s, ws.readyState === 1 ? 'open' : ws.readyState >= 2 ? 'err' : '');
    };
    ws.addEventListener('open', set);
    ws.addEventListener('close', set);
    ws.addEventListener('error', set);
    set();
  }
}

function persistBridges() {
  LS.set(K_BRIDGES, JSON.stringify(saved.map((e) => ({ type: e.type, fields: e.fields || {} }))));
}
function removeSaved(entry) {
  const i = saved.indexOf(entry);
  if (i >= 0) { saved.splice(i, 1); persistBridges(); }
}

function shouldStartNow(b, fresh) {
  const hasFields = b.fields && b.fields.length;
  if (!hasFields && !b.gesture) return true;          // trivial (loopback)
  if (fresh && b.gesture && !hasFields) return true;  // the Add click is the gesture
  if (!fresh && b.autoReconnect) return true;         // restore a network bridge
  return false;                                        // otherwise wait for a click
}

// Create, wire, and (optionally) start one bridge. entry = {type, fields};
// fresh = added from the menu (vs. restored from storage).
function spinUp(entry, fresh) {
  const b = BRIDGES[entry.type];
  if (!b) return;
  if (b.manual) { addWebRTC(); return; }

  const ui = bridgeRow(b.label);
  if (!b.avail()) {
    ui.setState('unsupported', 'err');
    logLine('bad', b.label + ' — not available in this browser. Web Serial/Bluetooth '
      + 'need a Chromium browser (Chrome/Edge); some APIs also need a secure page (https or file://).');
    ui.remove.onclick = () => { removeSaved(entry); ui.destroy(); };
    return;
  }

  const getters = {};
  if (b.fields) {
    const form = document.createElement('div');
    form.className = 'row';
    for (const f of b.fields) {
      const inp = document.createElement('input');
      inp.type = 'text';
      inp.placeholder = f.ph || f.k;
      inp.value = entry.fields && entry.fields[f.k] != null ? entry.fields[f.k] : (f.val || '');
      inp.style.minWidth = '90px';
      form.appendChild(inp);
      getters[f.k] = () => inp.value.trim();
    }
    ui.body.appendChild(form);
  }

  const start = async () => {
    const vals = {};
    for (const k in getters) vals[k] = getters[k]();
    if (b.fields && b.fields.some((f) => !vals[f.k])) { ui.setState('need input', 'err'); return; }
    ui.setState('connecting');
    try {
      const t = await b.make(vals);
      if (b.setAddr && t.setAddr) t.setAddr(hub.node.addr());
      entry.fields = vals;
      if (b.persist !== false) {
        if (!saved.includes(entry)) saved.push(entry);
        persistBridges();
      }
      attachCounted(t, ui);
      wireState(t, b, ui);
      logLine('sys', 'bridge up: ' + b.label);
    } catch (e) {
      ui.setState('error', 'err');
      logLine('bad', 'bridge failed: ' + (e && e.message ? e.message : e));
    }
  };

  ui.remove.onclick = () => {
    removeSaved(entry);
    if (ui._t) { hub.removeTransport(ui._t); if (ui._t.close) try { ui._t.close(); } catch (e) { /* */ } }
    ui.destroy();
    logLine('sys', 'bridge removed');
    updateBridgesEmptyState();
  };

  if (shouldStartNow(b, fresh)) {
    start();
  } else {
    const go = document.createElement('button');
    go.textContent = fresh ? 'Connect' : 'Reconnect';
    go.onclick = () => { go.disabled = true; start().finally(() => { go.disabled = false; }); };
    const wrap = document.createElement('div');
    wrap.className = 'row';
    wrap.appendChild(go);
    ui.body.appendChild(wrap);
    if (!fresh) ui.setState('saved · reconnect');
  }
}

// The offline self-test: a second in-process node linked by loopback, so you can
// watch a signed envelope leave, arrive, and verify with zero network.
function spawnPartner() {
  const pnode = spore.newNode();
  const phub = new Hub(pnode);
  phub.onDeliver = (env) => {
    const ok = spore.verify(env);
    let text;
    try { text = new TextDecoder().decode(spore.payload(env)); } catch (e) { text = '?'; }
    logLine('relay', 'partner node received ' + JSON.stringify(text) + ' (sig ' + (ok ? 'OK' : 'BAD') + ')');
  };
  const pair = loopbackPair();
  phub.addTransport(pair[1]);
  logLine('sys', 'partner node ' + hexOf(pnode.addr()) + ' linked by loopback');
  return pair[0];
}

// WebRTC: two people swap two short blobs by any out-of-band channel.
function addWebRTC() {
  const ui = bridgeRow('WebRTC — direct peer (copy/paste invite)');
  ui.remove.onclick = () => {
    if (ui._t) { hub.removeTransport(ui._t); if (ui._t.close) try { ui._t.close(); } catch (e) { /* */ } }
    ui.destroy();
  };
  const info = document.createElement('p');
  info.className = 'note';
  info.textContent = 'Pick a role. Pass the blobs to the other side by any channel (chat, QR, voice). Not remembered across reloads.';
  const rowBtns = document.createElement('div');
  rowBtns.className = 'row';
  const bCreate = document.createElement('button');
  bCreate.className = 'ghost';
  bCreate.textContent = 'Create invite';
  const bAccept = document.createElement('button');
  bAccept.className = 'ghost';
  bAccept.textContent = 'Accept invite';
  rowBtns.append(bCreate, bAccept);
  ui.body.append(info, rowBtns);

  const ta = (label) => {
    const l = document.createElement('p');
    l.className = 'note';
    l.textContent = label;
    const t = document.createElement('textarea');
    ui.body.append(l, t);
    return t;
  };

  bCreate.onclick = async () => {
    rowBtns.remove();
    ui.setState('gathering');
    try {
      const o = await manualOffer();
      attachCounted(o.transport, ui);
      ui._t = o.transport;
      const out = ta('1. Send this invite to the other side:');
      out.value = o.offer; out.readOnly = true; out.focus(); out.select();
      const inp = ta('2. Paste their answer here, then Connect:');
      const go = document.createElement('button');
      go.textContent = 'Connect';
      const wrap = document.createElement('div'); wrap.className = 'row'; wrap.appendChild(go);
      ui.body.appendChild(wrap);
      go.onclick = async () => {
        try { await o.accept(inp.value); ui.setState('open', 'open'); logLine('sys', 'WebRTC link established'); }
        catch (e) { ui.setState('bad answer', 'err'); }
      };
    } catch (e) { ui.setState('error', 'err'); }
  };

  bAccept.onclick = async () => {
    rowBtns.remove();
    const inp = ta('1. Paste the invite you were given:');
    const go = document.createElement('button');
    go.textContent = 'Generate answer';
    const wrap = document.createElement('div'); wrap.className = 'row'; wrap.appendChild(go);
    ui.body.appendChild(wrap);
    go.onclick = async () => {
      go.disabled = true;
      ui.setState('gathering');
      try {
        const a = await manualAnswer(inp.value);
        const out = ta('2. Send this answer back; the link opens when they Connect:');
        out.value = a.answer; out.readOnly = true; out.focus(); out.select();
        const t = await a.transport;
        attachCounted(t, ui);
        ui._t = t;
        ui.setState('open', 'open');
        logLine('sys', 'WebRTC link established');
      } catch (e) { ui.setState('bad invite', 'err'); go.disabled = false; }
    };
  };
}

// ---- save a copy / forget --------------------------------------------------
$('save').onclick = () => {
  const blob = new Blob(['<!doctype html>\\n' + document.documentElement.outerHTML], { type: 'text/html' });
  const a = document.createElement('a');
  a.href = URL.createObjectURL(blob);
  a.download = 'spore-standalone.html';
  a.click();
  URL.revokeObjectURL(a.href);
  // Show a brief completion state
  const seedPanel = document.getElementById('panel-seed');
  if (seedPanel) {
    const existing = seedPanel.querySelector('.complete-state');
    if (!existing) {
      const c = document.createElement('div');
      c.className = 'complete-state';
      c.innerHTML = emptyState('Seed saved! This node can regrow from that file.');
      c.querySelector('.empty-state').style.padding = '8px 0';
      seedPanel.querySelector('details')?.after(c);
      setTimeout(() => c.remove(), 5000);
    }
  }
};
$('forget').onclick = () => {
  if (!confirm('Forget the saved identity and bridges in this browser? This node will come back as a new stranger.')) return;
  LS.del(K_SEED); LS.del(K_BRIDGES); LS.del(K_RING); LS.del(K_CONVOS); LS.del(K_FEEDS);
  location.reload();
};

// ---- persistent header interactions -----------------------------------------
document.addEventListener('DOMContentLoaded', () => {
  const copyAddrBtn = $('copy-addr');
  if (copyAddrBtn) {
    copyAddrBtn.onclick = () => {
      const addrElement = $('persistent-addr');
      if (addrElement) {
        navigator.clipboard.writeText(addrElement.textContent.replace('\u2026', ''))
          .then(() => {
            const originalText = copyAddrBtn.textContent;
            copyAddrBtn.textContent = 'COPIED';
            setTimeout(() => { copyAddrBtn.textContent = originalText; }, 1000);
          })
          .catch(err => { console.error('Failed to copy address:', err); });
      }
    };
  }

  const shareBtn = $('share-button');
  if (shareBtn) {
    shareBtn.onclick = () => {
      alert('Share functionality would be implemented here');
    };
  }
});

// ---- files (W5) ---------------------------------------------------------
document.addEventListener('DOMContentLoaded', () => {
  // Publish file
  const publishBtn = $('file-publish');
  if (publishBtn) {
    publishBtn.onclick = () => {
      const input = $('file-input');
      const fileNameInput = $('file-name');
      const file = input.files && input.files[0];
      if (!file) { logLine('bad', 'Select a file first'); return; }
      const name = fileNameInput.value.trim() || file.name;
      const reader = new FileReader();
      reader.onload = () => {
        const data = new Uint8Array(reader.result);
        const magnet = hub.node.publishFile(name, data);
        const hex = Array.from(magnet).map(b => b.toString(16).padStart(2, '0')).join('');
        $('file-result').textContent = 'published: ' + hex;
        logLine('sys', 'published file ' + name + ' with magnet ' + hex);
        renderLocalFiles();
      };
      reader.readAsArrayBuffer(file);
    };
  }

  // Fetch file by magnet
  const fetchBtn = $('file-fetch');
  if (fetchBtn) {
    fetchBtn.onclick = () => {
      const magnetHex = $('file-magnet').value.trim().replace(/[^0-9a-fA-F]/g, '');
      if (magnetHex.length !== 32) { logLine('bad', 'Magnet needs 32 hex chars'); return; }
      const magnet = new Uint8Array(16);
      for (let i = 0; i < 16; i++) magnet[i] = parseInt(magnetHex.substr(i * 2, 2), 16);
      const fr = hub.node.fetchFile(magnet);
      $('file-fetch-result').textContent = 'fetch initiated, result: ' + JSON.stringify(fr?.length || 0) + ' forwards';
      // Poll for the file
      const poll = setInterval(() => {
        const bytes = hub.node.fileBytes(magnet);
        if (bytes) {
          clearInterval(poll);
          const name = hub.node.fileName(magnet) || 'unnamed.bin';
          const blob = new Blob([bytes]);
          const a = document.createElement('a');
          a.href = URL.createObjectURL(blob); a.download = name; a.click();
          logLine('sys', 'downloaded ' + name + ' (' + bytes.length + ' bytes)');
          $('file-fetch-result').textContent = 'downloaded ' + name + ' (' + bytes.length + ' bytes)';
          renderLocalFiles();
        }
      }, 1000);
      setTimeout(() => clearInterval(poll), 30000);
    };
  }
});

function renderLocalFiles() {
  const el = $('local-files');
  if (!hub) return;
  const files = hub.node.listFiles();
  if (!files.length) {
    el.innerHTML = '<span class="cnt" style="padding:20px;display:block;text-align:center">No files stored locally.</span>';
    return;
  }
  el.innerHTML = files.map(f => {
    const hex = Array.from(f.magnet).map(b => b.toString(16).padStart(2, '0')).join('');
    return '<div style="display:flex;align-items:center;gap:6px;padding:4px 6px;border:1px solid var(--ink);font-size:12px">' +
      '<span style="flex:1">' + f.name + '</span>' +
      '<span class="cnt" style="font-size:10px;font-family:var(--font-mono)">' + hex.substring(0, 16) + '&hellip;</span>' +
    '</div>';
  }).join('');
}

// ---- tab navigation (WV1) ---------------------------------------------------
function switchTab(name) {
  const panels = document.querySelectorAll('.panel');
  for (const p of panels) p.style.display = 'none';
  const target = document.getElementById('panel-' + name);
  if (target) target.style.display = '';
  const tabs = document.querySelectorAll('.tab');
  for (const t of tabs) {
    t.setAttribute('aria-selected', t.dataset.panel === name ? 'true' : 'false');
  }
}
document.addEventListener('DOMContentLoaded', () => {
  for (const tab of document.querySelectorAll('.tab')) {
    tab.onclick = () => switchTab(tab.dataset.panel);
  }
  // Start on Chats
  switchTab('chats');
});

// Everything is defined; start the node.
boot();
</script>
</body>
</html>
`;

const outPath = path.join(here, 'spore-standalone.html');
fs.writeFileSync(outPath, html);
const kb = (n) => (n / 1024).toFixed(0) + ' KB';
console.log('wrote', path.relative(path.join(here, '..'), outPath), '(' + kb(Buffer.byteLength(html)) + ')');
console.log('embedded wasm sha256:', wasmHash);
