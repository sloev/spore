// Build a single self-contained HTML file that is a complete, *functional* SPORE
// node: the wasm router, every browser transport, the design system and the app
// are inlined, so the one file runs offline from a USB stick, an email
// attachment, or a scanned QR bundle — no server, no install, no network. Open
// it and you get one live node you can wire to real links at runtime, and it
// remembers its identity in the browser's local storage, so it comes back the
// same node next time. A copy of this file is a seed the whole node regrows
// from, and it is also the live demo served on the site.
//
//   cargo build --release --lib --target wasm32-unknown-unknown
//   node web/build-standalone.mjs
//   # -> web/spore-standalone.html
//
// M10-D: the app used to live *in this file*, as ~1580 lines inside one template
// literal. It is now `web/app/`, read from disk like every other module. Two
// things fall out of that. The old backtick hazard is gone — a backtick in the
// app source used to end the literal early and break the build — and the app is
// now editable, greppable and testable as ordinary files. This script's job is
// only to gather and inline.

import fs from 'node:fs';
import crypto from 'node:crypto';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import vm from 'node:vm';

const here = path.dirname(fileURLToPath(import.meta.url));
const read = (p) => fs.readFileSync(path.join(here, p), 'utf8');

// Inline a module: drop its import lines and `export` keywords so every name
// lands in one shared module scope when concatenated. There are no name
// collisions across the 28 modules this inlines — worth re-checking if you add
// one, because a collision here fails at runtime rather than at build time.
function inlineModule(src) {
  return src
    .split('\n')
    .filter((l) => !/^\s*import\s.+from\s.+;?\s*$/.test(l))
    .map((l) => l.replace(/^\s*export\s+/, ''))
    .join('\n');
}

// -- wasm --------------------------------------------------------------------

const wasmPath = path.join(here, '../target/wasm32-unknown-unknown/release/spore.wasm');
const wasmBytes = fs.readFileSync(wasmPath);
const wasmB64 = wasmBytes.toString('base64');
const wasmHash = crypto.createHash('sha256').update(wasmBytes).digest('hex');

// -- HARDBRUT/3 --------------------------------------------------------------
//
// The import order is read out of the design system's own styles.css rather than
// restated here, so it cannot drift from upstream: tokens, base, patterns, then
// the theme last so it can win.

const DS = 'vendor/hardbrut3/';

function designSystemCss() {
  const entry = read(DS + 'styles.css');
  const order = [...entry.matchAll(/@import\s+url\("([^"]+)"\)/g)].map((m) => m[1]);
  if (order.length === 0) throw new Error('no @imports found in ' + DS + 'styles.css');

  // Inline Inter 900 as a data URL. The design system is display-only about this
  // font (headings, buttons, labels) but it is the face the whole visual
  // language is drawn in, and a standalone that fetches it would break the
  // zero-external-request guarantee CI greps for.
  const font = fs.readFileSync(path.join(here, DS, 'assets/fonts/inter-900-latin.woff2'));
  const fontUrl = 'data:font/woff2;base64,' + font.toString('base64');

  return order
    .map((rel) => '/* ' + rel + ' */\n' + read(DS + rel))
    .join('\n')
    .replace(/url\("\.\.\/assets\/fonts\/inter-900-latin\.woff2"\)/g, 'url("' + fontUrl + '")');
}

const dsCss = designSystemCss();
const appCss = read('app/app.css');

// -- app modules -------------------------------------------------------------
//
// Order matters in a shared scope, for the two things evaluated at load time:
// `DESTINATIONS` in shell.mjs reads `ICONS`, so icons comes first; and main.mjs
// boots on load, so it comes last. Everything else is functions and classes,
// referenced only when called.

const MODULES = [
  // infrastructure
  'spore.mjs',
  'ui/markdown.mjs',
  // transports (kiss before the serial/BLE ones that use it)
  'transports/kiss.mjs',
  'transports/loopback.mjs',
  'transports/websocket.mjs',
  'transports/webrtc.mjs',
  'transports/nostr.mjs',
  'transports/webserial.mjs',
  'transports/webbluetooth.mjs',
  'transports/audio.mjs',
  'transports/webtorrent.mjs',
  'transports/meshtastic.mjs',
  'transports/reticulum.mjs',
  // app: helpers before the screens that use them
  'app/ui/dom.mjs',
  'app/ui/icons.mjs',
  'app/ui/format.mjs',
  'app/ui/shell.mjs',
  'app/stores/threads.mjs',
  'app/stores/contacts.mjs',
  'app/stores/topics.mjs',
  'app/screens/onboarding.mjs',
  'app/screens/chat.mjs',
  'app/screens/contacts.mjs',
  'app/screens/settings.mjs',
  'app/screens/files.mjs',
  'app/screens/blogs.mjs',
  'app/transports.mjs',
  'app/spore-client.mjs',
  // boots on load — must be last
  'app/main.mjs',
];

// Every app module on disk must be in MODULES. Forgetting one does not fail the
// build — it produces a standalone that is silently missing a screen, which is
// exactly how `app/screens/settings.mjs` nearly shipped as a file nobody loaded.
// The transports and infrastructure above are listed deliberately (order and
// membership are chosen), so only `app/` is swept.
function assertEveryAppModuleIsInlined() {
  const listed = new Set(MODULES);
  const onDisk = [];
  (function walk(dir) {
    for (const f of fs.readdirSync(path.join(here, dir))) {
      const rel = dir + '/' + f;
      if (fs.statSync(path.join(here, rel)).isDirectory()) walk(rel);
      else if (f.endsWith('.mjs') && !f.includes('.test.')) onDisk.push(rel);
    }
  })('app');
  const missing = onDisk.filter((f) => !listed.has(f));
  if (missing.length) {
    console.error('\nThese app modules exist but are not inlined:\n  ' + missing.join('\n  '));
    console.error('\nAdd them to MODULES (order matters — see the note above it).\n');
    process.exit(1);
  }
}
assertEveryAppModuleIsInlined();

const modules = MODULES.map((m) => '// ---- ' + m + '\n' + inlineModule(read(m))).join('\n\n');

// -- emit --------------------------------------------------------------------

const html = `<!doctype html>
<html lang="en" data-accent="red">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover">
<title>SPORE — node</title>
<style>
${dsCss}
/* ---- app adapter */
${appCss}
</style>
</head>
<body>
<a class="skip-link" href="#app">Skip to content</a>
<div id="app"></div>
<script>
// The wasm module, inlined. Decoded once here and handed to the app, so the
// page makes no request of any kind.
const WASM_B64 = "${wasmB64}";
globalThis.SPORE_WASM_BYTES = (() => {
  const bin = atob(WASM_B64);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out.buffer;
})();

${modules}
</script>
</body>
</html>
`;

// -- self-check --------------------------------------------------------------
//
// The emitted app is one *classic* script, and a syntax error in a classic
// script fails **silently**: the page loads, the script never runs, and you get
// a blank shell with nothing in the console unless you look for an exception.
// `node --check` on the modules does not catch it, because each module is valid
// on its own — it is the classic-script context that changes what is legal.
//
// This caught `import.meta` (legal in a module, a syntax error here) only after
// a browser run. Parsing the output before writing it turns that into a build
// failure with a line number.
function assertParsesAsClassicScript(source) {
  try {
    new vm.Script(source, { filename: 'spore-standalone inline script' });
  } catch (err) {
    console.error('\nThe generated app is not a valid classic script:\n  ' + err.message);
    console.error('\nCommon cause: module-only syntax (import.meta, import(), top-level await)');
    console.error('reached the inlined app. Use a module-free equivalent.\n');
    process.exit(1);
  }
}

const inlineScript = html.match(/<script>\n([\s\S]*?)\n<\/script>/);
if (!inlineScript) throw new Error('could not find the inline script to verify');
assertParsesAsClassicScript(inlineScript[1]);

const outPath = path.join(here, 'spore-standalone.html');
fs.writeFileSync(outPath, html);
const kb = (n) => (n / 1024).toFixed(0) + ' KB';
console.log('wrote', path.relative(path.join(here, '..'), outPath), '(' + kb(Buffer.byteLength(html)) + ')');
console.log('embedded wasm sha256:', wasmHash);
console.log('inlined', MODULES.length, 'modules,', kb(Buffer.byteLength(dsCss)), 'of design system CSS');
