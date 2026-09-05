// TopicStore contract.
//
//   node web/app/stores/topics.test.mjs

import assert from 'node:assert';
import { TopicStore } from './topics.mjs';

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
  const s = new TopicStore({});
  assert.strictEqual(s.nameFor(T), null, 'it does not invent a name');
  s.remember(T, 'ridge-weather');
  assert.strictEqual(s.nameFor(T), 'ridge-weather');
});

await test('names and posts survive a reload', async () => {
  const storage = memoryStorage();
  const a = new TopicStore({ storage });
  a.remember(T, 'tides');
  a.receive({ topicHex: T, from: 'bb'.repeat(4), body: 'high at 14:20', at: 100 });
  await a.save();

  const b = new TopicStore({ storage });
  await b.load();
  assert.strictEqual(b.nameFor(T), 'tides');
  assert.strictEqual(b.postsOn(T).length, 1);
  assert.strictEqual(b.latestOn(T).body, 'high at 14:20');
});

await test('an unsigned post is kept with from = null, never with a guess', async () => {
  // A feed post floods and need not be signed. Recording a sender we did not
  // authenticate would be the same mistake ThreadStore refuses to make.
  const s = new TopicStore({});
  s.receive({ topicHex: T, from: null, body: 'anon', at: 1 });
  assert.strictEqual(s.postsOn(T)[0].from, null);
});

await test('retention is bounded, because anyone may publish to a feed', async () => {
  const s = new TopicStore({});
  for (let i = 0; i < 500; i++) s.receive({ topicHex: T, from: null, body: 'p' + i, at: i });
  const kept = s.postsOn(T);
  assert.ok(kept.length <= 200, 'capped, got ' + kept.length);
  assert.strictEqual(kept[kept.length - 1].body, 'p499', 'the newest is what survives');
});

await test('forgetting a topic drops its name and its posts together', async () => {
  const s = new TopicStore({});
  s.remember(T, 'gone');
  s.receive({ topicHex: T, from: null, body: 'x', at: 1 });
  s.forget(T);
  assert.strictEqual(s.nameFor(T), null);
  assert.deepStrictEqual(s.postsOn(T), []);
});

await test('a corrupt blob starts empty rather than throwing', async () => {
  const storage = memoryStorage();
  await storage.set('spore.topics', '{not json');
  const s = new TopicStore({ storage });
  await s.load();
  assert.strictEqual(s.nameFor(T), null);
});

console.log(failures ? '\n' + failures + ' failing' : '\nall passing');
process.exit(failures ? 1 : 0);
