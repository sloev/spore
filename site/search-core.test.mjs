// The docs search ranking.
//
//   node site/search-core.test.mjs
import assert from 'node:assert';
import { rank, score, terms } from './search-core.mjs';

let failures = 0;
function test(name, fn) {
  try { fn(); console.log(`  ok  ${name}`); }
  catch (e) { failures++; console.log(`FAIL  ${name}\n      ${e.message}`); }
}

const INDEX = [
  { p: 'glossary.html', a: '#envelope', t: 'Envelope', s: 'The unit that crosses the wire: a signed postcard. To, from, when it was written.' },
  { p: 'spec.html', a: '#2-envelope', t: '2. Envelope', s: 'Sixteen bytes of fixed header, then the sender, then a two-byte payload length.' },
  { p: 'spec.html', a: '#6-sync', t: '6. Sync and custody', s: 'On any meeting: ANNOUNCE, then INV, peer replies WANT. The envelope is sent.' },
  { p: 'glossary.html', a: '#fountain-code', t: 'Fountain code', s: 'An erasure code: from n pieces it can mint unlimited distinct repair symbols.' },
  { p: 'bridges.html', a: '#lora', t: 'LoRa', s: 'A link with a small frame. Link fragmentation splits an envelope across hops.' },
];

console.log('docs search ranking:');

test('a term in a heading outranks the same term in a body', () => {
  const out = rank(INDEX, 'envelope');
  assert.ok(out.length >= 3);
  assert.strictEqual(out[0].t, 'Envelope', 'the section named for it comes first');
  assert.ok(out.findIndex((e) => e.a === '#6-sync') > 1, 'a passing mention ranks below');
});

test('every term must appear, so a second word narrows', () => {
  const broad = rank(INDEX, 'link');
  const narrow = rank(INDEX, 'link fragmentation');
  assert.ok(narrow.length <= broad.length, 'narrowing cannot widen');
  assert.strictEqual(narrow.length, 1);
  assert.strictEqual(narrow[0].t, 'LoRa');
});

test('a query that matches nothing returns nothing', () => {
  assert.deepStrictEqual(rank(INDEX, 'kubernetes'), []);
  assert.deepStrictEqual(rank(INDEX, ''), [], 'and an empty query is not a match-all');
  assert.deepStrictEqual(rank(INDEX, '   '), []);
});

test('single characters are ignored rather than matching everything', () => {
  assert.deepStrictEqual(terms('a e i'), [], 'one-character terms are dropped');
  assert.deepStrictEqual(rank(INDEX, 'a'), []);
});

test('a long section cannot win by repetition alone', () => {
  // Without a cap on body hits, the longest page wins every query. Real case:
  // SPEC.md says "envelope" dozens of times and would bury the glossary entry
  // that defines it.
  const spammy = { p: 'x.html', a: '#x', t: 'Unrelated', s: 'envelope '.repeat(40) };
  const out = rank([...INDEX, spammy], 'envelope');
  assert.strictEqual(out[0].t, 'Envelope', 'the heading match still wins');
  assert.ok(out.findIndex((e) => e.t === 'Unrelated') > 0);
});

test('an exact word in a heading beats a substring of a longer one', () => {
  const sub = { p: 'y.html', a: '#y', t: 'Envelopes and enveloping', s: 'unrelated text' };
  const exact = { p: 'z.html', a: '#z', t: 'Envelope', s: 'unrelated text' };
  const out = rank([sub, exact], 'envelope');
  assert.strictEqual(out[0].t, 'Envelope');
});

test('search is case-insensitive in both directions', () => {
  assert.strictEqual(rank(INDEX, 'ENVELOPE')[0].t, 'Envelope');
  assert.ok(score({ t: 'ANNOUNCE', s: '' }, terms('announce')) > 0);
});

console.log(failures ? `\n${failures} failed` : '\nSEARCH OK');
process.exit(failures ? 1 : 0);
