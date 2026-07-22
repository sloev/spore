// A small random-linear fountain code over GF(256) — the paper twin of SPORE's
// own §3 fragmentation. Split a payload into K source blocks; emit N fragments,
// each a random linear combination of the K blocks (coefficients derived from
// the fragment's 4-byte seed, so they aren't stored). ANY K linearly-independent
// fragments rebuild the whole payload via Gauss-Jordan elimination — so a torn,
// smudged, or partially-scanned printout still recovers, as long as enough codes
// survive.
//
// Fragment bytes: "SP" | ver(1) | origLen(4 BE) | K(2 BE) | B(2 BE) | seed(4 BE) | block(B)

// ---- GF(256), primitive polynomial 0x11d, generator 2 (Reed-Solomon field) ---
const EXP = new Uint8Array(512);
const LOG = new Uint8Array(256);
(() => {
  let x = 1;
  for (let i = 0; i < 255; i++) {
    EXP[i] = x;
    LOG[x] = i;
    x <<= 1;
    if (x & 0x100) x ^= 0x11d;
  }
  for (let i = 255; i < 512; i++) EXP[i] = EXP[i - 255];
})();
const mul = (a, b) => (a === 0 || b === 0 ? 0 : EXP[LOG[a] + LOG[b]]);
const inv = (a) => EXP[255 - LOG[a]];

const HEADER = 15;
const MAGIC0 = 0x53, MAGIC1 = 0x50; // "SP"

// Deterministic coefficient row for a fragment seed (both sides derive it).
// splitmix32 so consecutive seeds give well-decorrelated rows — with near-random
// GF(256) coefficients, any K fragments are linearly independent with very high
// probability (a couple of extra fragments cover the rare dependent draw).
function coeffRow(seed, K) {
  let x = (seed + 0x9e3779b9) >>> 0;
  const row = new Uint8Array(K);
  let nonzero = false;
  for (let i = 0; i < K; i++) {
    x = (x + 0x9e3779b9) >>> 0;
    let z = x;
    z = Math.imul(z ^ (z >>> 16), 0x21f0aaad) >>> 0;
    z = Math.imul(z ^ (z >>> 15), 0x735a2d97) >>> 0;
    z = (z ^ (z >>> 15)) >>> 0;
    row[i] = z & 0xff;
    if (row[i]) nonzero = true;
  }
  if (!nonzero) row[seed % K] = 1;
  return row;
}

function u32(view, o) { return ((view[o] << 24) | (view[o + 1] << 16) | (view[o + 2] << 8) | view[o + 3]) >>> 0; }
function put32(arr, o, v) { arr[o] = (v >>> 24) & 255; arr[o + 1] = (v >>> 16) & 255; arr[o + 2] = (v >>> 8) & 255; arr[o + 3] = v & 255; }
function put16(arr, o, v) { arr[o] = (v >>> 8) & 255; arr[o + 1] = v & 255; }

/** Encode `payload` (Uint8Array) into `count` fragments of block size `B`. */
export function encode(payload, B = 150, count = null) {
  const origLen = payload.length;
  const K = Math.max(1, Math.ceil(origLen / B));
  const N = count ?? Math.ceil(K * 1.5) + 4; // ~50% redundancy + a cushion
  const padded = new Uint8Array(K * B);
  padded.set(payload);
  const blocks = [];
  for (let i = 0; i < K; i++) blocks.push(padded.subarray(i * B, (i + 1) * B));

  const frags = [];
  for (let seed = 0; seed < N; seed++) {
    const row = coeffRow(seed, K);
    const enc = new Uint8Array(B);
    for (let i = 0; i < K; i++) {
      const c = row[i];
      if (!c) continue;
      const blk = blocks[i];
      for (let j = 0; j < B; j++) enc[j] ^= mul(c, blk[j]);
    }
    const frag = new Uint8Array(HEADER + B);
    frag[0] = MAGIC0; frag[1] = MAGIC1; frag[2] = 1;
    put32(frag, 3, origLen); put16(frag, 7, K); put16(frag, 9, B); put32(frag, 11, seed);
    frag.set(enc, HEADER);
    frags.push(frag);
  }
  return frags;
}

/** Parse one fragment's header. */
export function header(frag) {
  if (frag[0] !== MAGIC0 || frag[1] !== MAGIC1 || frag[2] !== 1) return null;
  return { origLen: u32(frag, 3), K: (frag[7] << 8) | frag[8], B: (frag[9] << 8) | frag[10], seed: u32(frag, 11) };
}

/** Decode any collection of fragments; returns the payload or null if too few. */
export function decode(frags) {
  const h0 = header(frags[0]);
  if (!h0) return null;
  const { origLen, K, B } = h0;
  const rows = []; // reduced rows in row-echelon form: {coef, val, pivot}

  for (const frag of frags) {
    const h = header(frag);
    if (!h || h.K !== K || h.B !== B) continue;
    let coef = coeffRow(h.seed, K).slice();
    let val = frag.slice(HEADER, HEADER + B);
    // Reduce against existing pivots.
    for (const r of rows) {
      const f = coef[r.pivot];
      if (f) {
        for (let j = 0; j < K; j++) coef[j] ^= mul(f, r.coef[j]);
        for (let j = 0; j < B; j++) val[j] ^= mul(f, r.val[j]);
      }
    }
    let pivot = -1;
    for (let j = 0; j < K; j++) if (coef[j]) { pivot = j; break; }
    if (pivot < 0) continue; // linearly dependent — no new information
    const iv = inv(coef[pivot]);
    for (let j = 0; j < K; j++) coef[j] = mul(coef[j], iv);
    for (let j = 0; j < B; j++) val[j] = mul(val[j], iv);
    rows.push({ coef, val, pivot });
    if (rows.length === K) break;
  }
  if (rows.length < K) return null;

  // Gauss-Jordan: clear each pivot column above.
  for (let p = rows.length - 1; p >= 0; p--) {
    const piv = rows[p].pivot;
    for (let q = 0; q < p; q++) {
      const f = rows[q].coef[piv];
      if (f) {
        for (let j = 0; j < K; j++) rows[q].coef[j] ^= mul(f, rows[p].coef[j]);
        for (let j = 0; j < B; j++) rows[q].val[j] ^= mul(f, rows[p].val[j]);
      }
    }
  }
  const out = new Uint8Array(K * B);
  for (const r of rows) out.set(r.val, r.pivot * B);
  return out.slice(0, origLen);
}
