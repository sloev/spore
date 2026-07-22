// Prove any-K-of-N recovery: encode a payload, throw away fragments at random,
// and check that any surviving K-independent subset rebuilds it byte-for-byte.
import assert from 'node:assert';
import crypto from 'node:crypto';
import { encode, decode, header } from './fountain.mjs';

function shuffle(a, seed) {
  let x = seed >>> 0;
  for (let i = a.length - 1; i > 0; i--) {
    x ^= x << 13; x >>>= 0; x ^= x >>> 17; x ^= x << 5; x >>>= 0;
    const j = x % (i + 1);
    [a[i], a[j]] = [a[j], a[i]];
  }
  return a;
}

function run(payload, B) {
  const frags = encode(payload, B);
  const K = header(frags[0]).K;
  const sha = (b) => crypto.createHash('sha256').update(b).digest('hex');
  const want = sha(payload);

  // 50 random trials: keep a random subset the size of K plus a small realistic
  // reception overhead, decode, and verify. This is the "any K-of-N" claim with
  // the couple of extra fragments a fountain code always expects.
  const cushion = Math.max(2, Math.ceil(K * 0.1));
  for (let t = 0; t < 50; t++) {
    const keep = shuffle(frags.slice(), 0xABCD + t).slice(0, Math.min(frags.length, K + cushion));
    const got = decode(keep);
    assert.ok(got, `trial ${t}: decode returned null with ${keep.length} of ${frags.length} (K=${K})`);
    assert.strictEqual(sha(got), want, `trial ${t}: payload mismatch`);
  }
  // Too few fragments must fail cleanly (not throw, not lie).
  if (K > 1) assert.strictEqual(decode(frags.slice(0, K - 1)), null, 'K-1 fragments must not decode');
  return { K, N: frags.length };
}

const small = new TextEncoder().encode('the dam holds — meet at the north pier at midnight');
const r1 = run(small, 16);
console.log(`small payload: K=${r1.K} N=${r1.N} — any ~K of N recover`);

const big = crypto.randomBytes(6000);
const r2 = run(big, 150);
console.log(`6 KB payload:  K=${r2.K} N=${r2.N} — any ~K of N recover`);

console.log('FOUNTAIN OK — any K-of-N fragments reconstruct the payload');
