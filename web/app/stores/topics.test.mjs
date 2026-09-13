// TopicStore contract.
//
//   node web/app/stores/topics.test.mjs

import fs from 'node:fs';
import assert from 'node:assert';
import { Communicator } from '../communicator.mjs';
import { TopicStore } from './topics.mjs';

const wasmPath = new URL('../../../target/wasm32-unknown-unknown/release/spore.wasm', import.meta.url);
let ex;
const { instance } = await WebAssembly.instantiate(fs.readFileSync(wasmPath), {
  env: {
    spore_fill_random: (ptr, len) => crypto.getRandomValues(new Uint8Array(ex.memory.buffer, ptr, len)),
    spore_store_put: () => {}, spore_store_get: () => 0n,
    spore_store_remove: () => {}, spore_store_ids: () => 0n,
  },
});
ex = instance.exports;

/** A store with its own communicator, so no test can see another's state. */
function newStore(opts = {}) {
  const c = new Communicator(ex);
  return new TopicStore({ ...opts, comm: () => c });
}

let failures = 0;
async function test(name, fn) {
  try { await fn(); console.log('  ok  ' + name); }
  catch (err) { failures++; console.log('FAIL  ' + name + '\n      ' + (err && err.message)); }
}

function memoryStorage() {
  const m = new Map();
  return { get: async (k) => m.get(k) ?? null, set: async (k, v) => { m.set(k, v); }, remove: async (k) => { m.delete(k); } };
}

const T = 'a'.repeat(16);

await test('a topic with no remembered name reads as nameless, not as its address', async () => {
  const s = newStore({});
  assert.strictEqual(s.nameFor(T), null, 'it does not invent a name');
  s.remember(T, 'ridge-weather');
  assert.strictEqual(s.nameFor(T), 'ridge-weather');
});

await test('names and posts survive a reload', async () => {
  const storage = memoryStorage();
  const a = newStore({ storage });
  a.remember(T, 'tides');
  a.receive({ topicHex: T, from: 'bb'.repeat(8), body: 'high at 14:20', at: 100 });
  await a.save();

  const b = newStore({ storage });
  await b.load();
  assert.strictEqual(b.nameFor(T), 'tides');
  assert.strictEqual(b.postsOn(T).length, 1);
  assert.strictEqual(b.latestOn(T).body, 'high at 14:20');
});

await test('an unsigned post is kept with from = null, never with a guess', async () => {
  // A feed post floods and need not be signed. Recording a sender we did not
  // authenticate would be the same mistake ThreadStore refuses to make.
  const s = newStore({});
  s.receive({ topicHex: T, from: null, body: 'anon', at: 1 });
  assert.strictEqual(s.postsOn(T)[0].from, null);
});

await test('retention is bounded, because anyone may publish to a feed', async () => {
  const s = newStore({});
  for (let i = 0; i < 500; i++) s.receive({ topicHex: T, from: null, body: 'p' + i, at: i });
  const kept = s.postsOn(T);
  assert.ok(kept.length <= 200, 'capped, got ' + kept.length);
  assert.strictEqual(kept[kept.length - 1].body, 'p499', 'the newest is what survives');
});

await test('forgetting a topic drops its name and its posts together', async () => {
  const s = newStore({});
  s.remember(T, 'gone');
  s.receive({ topicHex: T, from: null, body: 'x', at: 1 });
  s.forget(T);
  assert.strictEqual(s.nameFor(T), null);
  assert.deepStrictEqual(s.postsOn(T), []);
});

await test('a corrupt blob starts empty rather than throwing', async () => {
  const storage = memoryStorage();
  await storage.set('spore.topics', '{not json');
  const s = newStore({ storage });
  await s.load();
  assert.strictEqual(s.nameFor(T), null);
});

console.log(failures ? '\n' + failures + ' failing' : '\nall passing');
process.exit(failures ? 1 : 0);
