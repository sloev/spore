// ThreadStore rules (M10-D). Pure logic, no DOM, no client.
//
//   node web/app/stores/threads.test.mjs

import assert from 'node:assert';
import { ThreadStore, groupThread } from './threads.mjs';
import { memoryAdapter } from '../spore-client.mjs';
import { dayLabel } from '../ui/format.mjs';

let failures = 0;
async function test(name, fn) {
  try { await fn(); console.log('  ok  ' + name); }
  catch (err) { failures++; console.log('FAIL  ' + name + '\n      ' + (err && err.message)); }
}

const ADDR = '3f2a9c1088e4001b';
const OTHER = '9c10e4883f2a001b';

// ------------------------------------------------------------ authentication

await test('an unauthenticated envelope is counted, never filed into a thread', async () => {
  const s = new ThreadStore();
  // from:null covers an unsigned envelope, a bad signature, and SRC8 alike.
  // Filing any of them under a conversation would make the list spoofable.
  const key = s.receive({ from: null, body: 'trust me', sealed: false, at: 1 });
  assert.strictEqual(key, null);
  assert.strictEqual(s.conversations().length, 0, 'no conversation may be created');
  assert.strictEqual(s.unauthenticatedCount, 1, 'but it must not vanish silently either');
});

await test('an authenticated message opens a conversation', async () => {
  const s = new ThreadStore();
  assert.strictEqual(s.receive({ from: ADDR, body: 'hello', sealed: true, at: 10 }), ADDR);
  assert.strictEqual(s.messages(ADDR).length, 1);
  assert.strictEqual(s.unreadFor(ADDR), 1);
});

// -------------------------------------------------------------- optimistic

await test('a send is a real row carrying the true envelope id', async () => {
  const s = new ThreadStore();
  s.send({ id: 'abc123', to: ADDR, body: 'hi', at: 5, sealed: true });
  const [m] = s.messages(ADDR);
  assert.strictEqual(m.status, 'queued');
  assert.strictEqual(m.id, 'abc123');
  assert.strictEqual(m.self, true);
});

await test('an ack reconciles by id, not by position', async () => {
  const s = new ThreadStore();
  s.send({ id: 'first', to: ADDR, body: 'one', at: 1, sealed: true });
  s.send({ id: 'second', to: ADDR, body: 'two', at: 2, sealed: true });
  assert.ok(s.setStatus('second', 'acked'));
  assert.strictEqual(s.messages(ADDR)[0].status, 'queued', 'the wrong row must not move');
  assert.strictEqual(s.messages(ADDR)[1].status, 'acked');
});

await test('an unknown id reports that it matched nothing', async () => {
  const s = new ThreadStore();
  s.send({ id: 'real', to: ADDR, body: 'x', at: 1, sealed: true });
  assert.strictEqual(s.setStatus('ghost', 'acked'), false);
});

await test('a received message has no id and cannot be mistaken for a send', async () => {
  const s = new ThreadStore();
  s.receive({ from: ADDR, body: 'in', sealed: true, at: 1 });
  // setStatus walks every message; a null id must never match a null lookup.
  assert.strictEqual(s.setStatus(null, 'acked'), false);
});

// ------------------------------------------------------------- conversations

await test('conversations sort most recently active first', async () => {
  const s = new ThreadStore();
  s.receive({ from: ADDR, body: 'older', sealed: true, at: 100 });
  s.receive({ from: OTHER, body: 'newer', sealed: true, at: 200 });
  assert.deepStrictEqual(s.conversations().map((c) => c.addr), [OTHER, ADDR]);
});

await test('the store never invents a display name', async () => {
  const s = new ThreadStore();
  s.receive({ from: ADDR, body: 'hi', sealed: true, at: 1 });
  const [row] = s.conversations();
  assert.ok(!('name' in row), 'naming is the contact store\'s job — a claimed name is not a proved one');
  assert.strictEqual(row.addr, ADDR);
});

await test('marking read clears only that conversation', async () => {
  const s = new ThreadStore();
  s.receive({ from: ADDR, body: 'a', sealed: true, at: 1 });
  s.receive({ from: OTHER, body: 'b', sealed: true, at: 2 });
  s.markRead(ADDR);
  assert.strictEqual(s.unreadFor(ADDR), 0);
  assert.strictEqual(s.unreadFor(OTHER), 1);
  assert.strictEqual(s.totalUnread(), 1);
});

// -------------------------------------------------------------- persistence

await test('threads survive a reload through the storage port', async () => {
  const storage = memoryAdapter();
  const a = new ThreadStore({ storage });
  a.receive({ from: ADDR, body: 'persist me', sealed: true, at: 7 });
  await a.save();

  const b = new ThreadStore({ storage });
  await b.load();
  assert.strictEqual(b.messages(ADDR).length, 1);
  assert.strictEqual(b.messages(ADDR)[0].body, 'persist me');
  assert.strictEqual(b.unreadFor(ADDR), 1);
});

await test('a corrupt blob starts empty rather than throwing or wiping', async () => {
  const storage = memoryAdapter({ 'spore.threads': '{not json' });
  const s = new ThreadStore({ storage });
  await s.load();
  assert.strictEqual(s.conversations().length, 0);
  assert.strictEqual(await storage.get('spore.threads'), '{not json', 'the bad blob is left alone, not destroyed');
});

// ------------------------------------------------------------------ grouping

await test('a thread is split by day and collapsed into author runs', async () => {
  const day = (ms) => dayLabel(ms, new Date(2026, 2, 15, 12, 0).getTime());
  const t = (d, h) => new Date(2026, 2, d, h, 0).getTime();
  const msgs = [
    { self: false, at: t(14, 9) },
    { self: false, at: t(14, 10) },
    { self: true, at: t(15, 9) },
  ];
  const out = groupThread(msgs, day);
  assert.deepStrictEqual(out.map((x) => x.kind), ['day', 'message', 'message', 'day', 'message']);
  assert.strictEqual(out[0].label, 'Yesterday');
  assert.strictEqual(out[1].run, 'first');
  assert.strictEqual(out[2].run, 'last');
  assert.strictEqual(out[3].label, 'Today');
  assert.strictEqual(out[4].run, 'only', 'a day break restarts the run');
});

await test('a lone message is a run of one, not a first', async () => {
  const out = groupThread([{ self: true, at: 1 }], () => 'Today');
  assert.strictEqual(out[1].run, 'only');
});

console.log(failures ? '\n' + failures + ' failing' : '\nall passing');
process.exit(failures ? 1 : 0);
