// Reference decoder for the Seed Sheet: read scanned QR contents (base64, one per
// line) from a file or stdin, rebuild the payload, gunzip it, and verify its
// SHA-256. Proves the printed codes are recoverable with a real, independent
// decoder — no magic in the encoder.
//
//   node site/seed/decode-seedsheet.mjs scanned.txt [--sha <hex>] > payload
//   scan-tool | node site/seed/decode-seedsheet.mjs        # from stdin
import fs from 'node:fs';
import zlib from 'node:zlib';
import crypto from 'node:crypto';
import { decode } from './fountain.mjs';

const args = process.argv.slice(2);
const shaIdx = args.indexOf('--sha');
const wantSha = shaIdx >= 0 ? args[shaIdx + 1] : null;
const fileArg = args.find((a, i) => !a.startsWith('--') && i !== shaIdx + 1);

const text = fileArg ? fs.readFileSync(fileArg, 'utf8') : fs.readFileSync(0, 'utf8');
const frags = text
  .split(/\r?\n/)
  .map((l) => l.trim())
  .filter(Boolean)
  .map((l) => new Uint8Array(Buffer.from(l, 'base64')));

if (frags.length === 0) {
  console.error('no fragments on input (expected base64, one per line)');
  process.exit(2);
}

const gz = decode(frags);
if (!gz) {
  console.error(`decode failed: not enough independent fragments (${frags.length} given)`);
  process.exit(1);
}
let payload;
try {
  payload = zlib.gunzipSync(Buffer.from(gz));
} catch {
  payload = Buffer.from(gz); // payload may not have been gzipped
}
const got = crypto.createHash('sha256').update(payload).digest('hex');
if (wantSha && got !== wantSha) {
  console.error(`SHA-256 mismatch: got ${got}, expected ${wantSha}`);
  process.exit(1);
}
process.stderr.write(`recovered ${payload.length} bytes from ${frags.length} fragments · sha256 ${got}\n`);
process.stdout.write(payload);
