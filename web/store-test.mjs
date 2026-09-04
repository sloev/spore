// M10-A: the browser's custody store.
//
//   cargo build --release --lib --target wasm32-unknown-unknown
//   node web/store-test.mjs
//
// Before this, a browser node's store was memory-only: every envelope it was
// carrying for someone else vanished on reload. ESP32 mounts SPIFFS through
// ESP-IDF's VFS and runs `FsSpill` unmodified, and every other target has a real
// filesystem, so the browser was the only target that could not keep custody.
//
// Two properties matter, and the second is the one worth guarding: a store that
// persists, and a store that is *never trusted*. An id is the hash of its own
// content, so adoption re-verifies — a host that lost, truncated or altered
// bytes contributes fewer envelopes rather than injecting one.

import fs from 'node:fs';
import assert from 'node:assert';
import { loadSpore, memorySpillStore } from './spore.mjs';

const wasm = fs.readFileSync(new URL('../target/wasm32-unknown-unknown/release/spore.wasm', import.meta.url));
// Real wall clock, deliberately: `recv()` stamps arrivals with the host's own
// clock, so an envelope built at a fixed past timestamp arrives already expired
// and is dropped before it can ever be stored.
const NOW = Math.floor(Date.now() / 1000);

let failures = 0;
async function test(name, fn) {
  try { await fn(); console.log('  ok  ' + name); }
  catch (err) { failures++; console.log('FAIL  ' + name + '\n      ' + (err && err.message)); }
}

/** One envelope addressed to nobody in particular, so any node will carry it. */
async function anEnvelope() {
  const spore = await loadSpore(wasm);
  const author = spore.newNode();
  const { forwards } = author.publish('weather', new TextEncoder().encode('squall on the ridge'), NOW);
  assert.ok(forwards.length > 0, 'publishing must produce something to carry');
  return forwards[0];
}

// ---------------------------------------------------------------- persistence

await test('a store handed to loadSpore receives what the node carries', async () => {
  const store = memorySpillStore();
  const spore = await loadSpore(wasm, { store });
  const node = spore.newNode();
  node.useHostStore(NOW);

  assert.strictEqual(store.ids().length, 0, 'nothing carried yet');
  node.recv(await anEnvelope(), NOW);
  assert.ok(store.ids().length > 0, 'an envelope it is carrying reaches the host store');
});

await test('custody survives a reload', async () => {
  // The same store across two node lifetimes is exactly what a page refresh is.
  const store = memorySpillStore();

  const first = await loadSpore(wasm, { store });
  const a = first.newNode();
  a.useHostStore(NOW);
  a.recv(await anEnvelope(), NOW);
  const carried = store.ids().length;
  assert.ok(carried > 0);

  const second = await loadSpore(wasm, { store });
  const b = second.newNode();
  const adopted = b.useHostStore(NOW);
  assert.strictEqual(adopted, carried, 'the new node adopts what the old one was holding');
});

await test('a node with no host store keeps the old memory-only behaviour', async () => {
  // Never calling useHostStore must change nothing, since the imports are wired
  // unconditionally (a wasm module cannot import conditionally).
  const store = memorySpillStore();
  const spore = await loadSpore(wasm, { store });
  const node = spore.newNode();
  node.recv(await anEnvelope(), NOW);
  assert.strictEqual(store.ids().length, 0, 'an unasked-for store is never written');
});

// ------------------------------------------------------- the store is untrusted

await test('a tampered entry is not adopted', async () => {
  const store = memorySpillStore();
  const first = await loadSpore(wasm, { store });
  const a = first.newNode();
  a.useHostStore(NOW);
  a.recv(await anEnvelope(), NOW);

  // Flip a byte in the middle of the stored wire, as a hostile or failing store
  // would. The id is the hash of the content, so this no longer hashes to the
  // id it is filed under.
  const [id] = store.ids();
  const wire = store.get(id);
  wire[Math.floor(wire.length / 2)] ^= 0xff;
  store.put(id, wire);

  const second = await loadSpore(wasm, { store });
  const adopted = second.newNode().useHostStore(NOW);
  assert.strictEqual(adopted, 0, 'a mismatched entry reads as "not held", never as an envelope');
});

await test('a store returning garbage cannot inject an envelope', async () => {
  const hostile = {
    put() {},
    get: () => new Uint8Array(200).fill(0x41),      // not an envelope at all
    remove() {},
    ids: () => ['a'.repeat(32), 'b'.repeat(32)],    // ids it never held
  };
  const spore = await loadSpore(wasm, { store: hostile });
  assert.strictEqual(spore.newNode().useHostStore(NOW), 0, 'nothing is adopted from nonsense');
});

await test('malformed ids from the host are ignored, not crashed on', async () => {
  const junk = {
    put() {}, get: () => null, remove() {},
    ids: () => ['not-hex', '', 'ab', 'c'.repeat(32)],
  };
  const spore = await loadSpore(wasm, { store: junk });
  assert.strictEqual(spore.newNode().useHostStore(NOW), 0);
});

console.log(failures ? '\n' + failures + ' failing' : '\nSTORE OK — browser custody persists, and is never trusted');
process.exit(failures ? 1 : 0);
