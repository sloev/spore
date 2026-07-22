// Full-loop Seed Sheet test: build the sheet's fragments from a real doc, drop a
// third of them at random, and confirm the reference decoder CLI rebuilds the
// exact bytes. Exercises encode -> gzip -> fragment -> (drop) -> decode CLI.
import fs from 'node:fs';
import zlib from 'node:zlib';
import crypto from 'node:crypto';
import assert from 'node:assert';
import { execFileSync } from 'node:child_process';
import path from 'node:path';
import os from 'node:os';
import { fileURLToPath } from 'node:url';
import { encode, header } from './fountain.mjs';

const here = path.dirname(fileURLToPath(import.meta.url));
const repo = path.resolve(here, '../..');
const doc = path.join(repo, 'docs/REBUILD.md');

const payload = fs.readFileSync(doc);
const gz = zlib.gzipSync(payload, { level: 9 });
const frags = encode(new Uint8Array(gz), 150);
const K = header(frags[0]).K;
const sha = crypto.createHash('sha256').update(payload).digest('hex');

// Deterministic shuffle, keep K + 10% (i.e. lose ~a third of the sheet).
let x = 0x1234;
const shuffled = frags.slice().sort(() => { x ^= x << 13; x >>>= 0; x ^= x >>> 17; x ^= x << 5; x >>>= 0; return (x & 1) ? 1 : -1; });
const keep = shuffled.slice(0, K + Math.max(2, Math.ceil(K * 0.1)));
const lines = keep.map((f) => Buffer.from(f).toString('base64')).join('\n');

const tmp = path.join(os.tmpdir(), `spore-scanned-${process.pid}.txt`);
fs.writeFileSync(tmp, lines);
const out = execFileSync('node', [path.join(here, 'decode-seedsheet.mjs'), tmp, '--sha', sha]);
fs.rmSync(tmp, { force: true });

assert.ok(out.equals(payload), 'decoder output must equal the original doc');
console.log(`SEEDSHEET OK — kept ${keep.length}/${frags.length} codes (K=${K}), decoder rebuilt ${payload.length} B, sha matches`);
