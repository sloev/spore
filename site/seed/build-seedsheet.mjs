// Build the printable "Seed Sheet": a two-sided A4 that carries a bootstrap
// payload (default: the reimplementation guide) as fountain-coded QR codes, plus
// a human-readable core so the sheet teaches you both the wire format and how to
// decode its own codes. Any ~K of the N codes rebuild the payload, so a torn or
// partially-scanned sheet still recovers.
//
//   node site/seed/build-seedsheet.mjs [payload.md]   -> web/spore-seedsheet.html
import fs from 'node:fs';
import zlib from 'node:zlib';
import crypto from 'node:crypto';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import QRCode from 'qrcode';
import { encode, header } from './fountain.mjs';

const here = path.dirname(fileURLToPath(import.meta.url));
const repo = path.resolve(here, '../..');
const payloadPath = process.argv[2] ? path.resolve(process.argv[2]) : path.join(repo, 'docs/REBUILD.md');
const B = 150; // fragment block size — keeps each QR comfortably scannable

const raw = fs.readFileSync(payloadPath);
const gz = zlib.gzipSync(raw, { level: 9 });
const sha = crypto.createHash('sha256').update(raw).digest('hex');
const frags = encode(new Uint8Array(gz), B);
const K = header(frags[0]).K;
const N = frags.length;
const b64 = (u8) => Buffer.from(u8).toString('base64');

// Render every fragment as an inline SVG QR (byte mode, medium ECC).
const qrs = await Promise.all(
  frags.map(async (f, i) => {
    const svg = await QRCode.toString(b64(f), { type: 'svg', errorCorrectionLevel: 'M', margin: 1 });
    return `<figure class="qr"><div class="code">${svg}</div><figcaption>#${i}</figcaption></figure>`;
  })
);

const payloadName = path.basename(payloadPath);
const html = `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8" />
<title>SPORE — Seed Sheet</title>
<style>
  @page { size: A4; margin: 12mm; }
  * { box-sizing: border-box; }
  body { margin: 0; color: #111; background: #fff;
    font: 11pt/1.4 system-ui, -apple-system, "Segoe UI", Roboto, sans-serif; }
  .sheet { padding: 16px 20px; }
  .side2 { page-break-before: always; }
  h1 { font-size: 20pt; margin: 0 0 2px; }
  h1 .s { color: #1a7f4b; }
  h2 { font-size: 12pt; margin: 16px 0 6px; border-bottom: 1px solid #bbb; padding-bottom: 3px; }
  .lede { color: #333; margin: 4px 0 10px; }
  table { border-collapse: collapse; width: 100%; font-size: 9pt; margin: 6px 0; }
  th, td { border: 1px solid #bbb; padding: 3px 6px; text-align: left; }
  th { background: #f0f0f0; }
  code, .mono { font-family: ui-monospace, "SFMono-Regular", Menlo, Consolas, monospace; font-size: 8.5pt; }
  .rules li { margin: 3px 0; }
  .meta { font-size: 9pt; background: #f6f6f6; border: 1px solid #ccc; border-radius: 0; padding: 8px 10px; }
  .backlink { font: 10pt system-ui, sans-serif; margin: 10px 20px 0; }
  @media print { .backlink { display: none; } }
  .meta .hash { word-break: break-all; }
  .grid { display: grid; grid-template-columns: repeat(6, 1fr); gap: 6px; margin-top: 8px; }
  @media (max-width: 700px) { .grid { grid-template-columns: repeat(4, 1fr); } }
  figure.qr { margin: 0; text-align: center; }
  figure.qr .code svg { width: 100%; height: auto; display: block; }
  figure.qr figcaption { font: 7pt ui-monospace, monospace; color: #444; margin-top: 1px; }
  .fine { color: #555; font-size: 8.5pt; }
  @media print { a { color: inherit; text-decoration: none; } }
</style>
</head>
<body>

<p class="backlink"><a href="apps.html">&larr; Back to Get a node</a></p>

<section class="sheet side1">
  <h1><span class="s">SPORE</span> — Seed Sheet</h1>
  <p class="lede">A cold-start capsule. This side teaches the wire format by hand;
     the reverse side carries the full reimplementation guide as fountain-coded QR
     codes. Scan <b>any ~${K}</b> of the ${N} codes to rebuild it — a torn or
     partly-scanned sheet still recovers.</p>

  <h2>Primitives</h2>
  <table>
    <tr><th>Purpose</th><th>Algorithm</th><th>Reference</th></tr>
    <tr><td>Signatures / identity</td><td>Ed25519</td><td>RFC 8032</td></tr>
    <tr><td>Key agreement</td><td>X25519</td><td>RFC 7748</td></tr>
    <tr><td>Sealed-box AEAD</td><td>XSalsa20-Poly1305 (NaCl crypto_box)</td><td>NaCl</td></tr>
    <tr><td>Symmetric AEAD</td><td>ChaCha20-Poly1305 / XChaCha20</td><td>RFC 8439</td></tr>
    <tr><td>Hashing (IDs, addresses)</td><td>SHA-256</td><td>FIPS 180-4</td></tr>
    <tr><td>Ratchet KDF</td><td>BLAKE2b</td><td>RFC 7693</td></tr>
  </table>

  <h2>The envelope (all integers big-endian)</h2>
  <table>
    <tr><th>Off</th><th>Size</th><th>Field</th><th>Notes</th></tr>
    <tr><td>0</td><td>1</td><td>ver</td><td>0x01</td></tr>
    <tr><td>1</td><td>1</td><td>typ</td><td>0 DATA · 1 INV · 2 WANT · 3 ANNOUNCE</td></tr>
    <tr><td>2</td><td>1</td><td>flags</td><td>0x01 ENC · 0x02 SIGNED · 0x04 FRAG · 0x08 ACKREQ · 0x10 FLOOD · 0x20 SRC8</td></tr>
    <tr><td>3</td><td>1</td><td>hops</td><td>TTL, decremented by relays</td></tr>
    <tr><td>4</td><td>4</td><td>expiry</td><td>unix seconds</td></tr>
    <tr><td>8</td><td>8</td><td>dest</td><td>address; all-zero = public</td></tr>
    <tr><td>16</td><td>32/8/0</td><td>src</td><td>if SIGNED: 32-byte key (or 8-byte addr if SRC8)</td></tr>
    <tr><td>…</td><td>2</td><td>plen</td><td>payload length</td></tr>
    <tr><td>…</td><td>plen</td><td>payload</td><td></td></tr>
    <tr><td>…</td><td>64/0</td><td>sig</td><td>if SIGNED</td></tr>
  </table>

  <h2>Four rules</h2>
  <ul class="rules">
    <li><b>Address</b> = <code>SHA-256(pubkey)[..8]</code>. <b>Topic</b> = <code>SHA-256(name)[..8]</code>.</li>
    <li><b>ID</b> = <code>SHA-256(envelope with hops byte = 0)[..16]</code> (over the bytes incl. sig).</li>
    <li><b>Sign</b> = <code>Ed25519(sk, body with hops=0, no sig)</code>; append the 64-byte signature.</li>
    <li><b>Armor</b> = <code>~S1.base32(wire).base32(SHA-256(wire)[..4])~</code> (RFC 4648, no padding).</li>
  </ul>

  <h2>This seed</h2>
  <div class="meta">
    payload: <code>${payloadName}</code> · original ${raw.length} B, gzip ${gz.length} B ·
    fragments <b>K=${K}, N=${N}</b>, block ${B} B ·<br />
    SHA-256(payload): <span class="hash mono">${sha}</span>
  </div>
  <p class="fine">Decode: each QR is base64 of a binary fragment
    <code>"SP" · ver(1) · origLen(4) · K(2) · B(2) · seed(4) · block(B)</code>. Any ~K
    independent fragments solve a K×K linear system over GF(256) (primitive poly
    0x11d): derive each fragment's K coefficients from its seed (splitmix32),
    Gauss-Jordan eliminate, concatenate the K blocks, trim to origLen, gunzip, and
    check the SHA-256 above. The reference decoder is
    <code>site/seed/decode-seedsheet.mjs</code>.</p>
</section>

<section class="sheet side2">
  <h2>Reverse — ${payloadName} as ${N} fountain QR codes (any ~${K} rebuild it)</h2>
  <div class="grid">
    ${qrs.join('\n    ')}
  </div>
</section>

</body>
</html>
`;

const outPath = path.join(repo, 'web/spore-seedsheet.html');
fs.writeFileSync(outPath, html);
console.log(`wrote web/spore-seedsheet.html — payload ${payloadName}, K=${K} N=${N}, ${(Buffer.byteLength(html) / 1024).toFixed(0)} KB`);
console.log(`payload sha256: ${sha}`);
