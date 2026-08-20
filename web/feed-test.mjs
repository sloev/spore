// Feed pub/sub round-trip across the wasm boundary, including the authenticated
// author. Kept out of web/test.mjs because that file is a frozen v1 contract
// test; this exercises the W10 addition to `spore_node_poll_feed` — the
// per-event `from` field — without touching the frozen surface.
//
//   cargo build --release --lib --target wasm32-unknown-unknown
//   node web/feed-test.mjs
import fs from 'node:fs';
import assert from 'node:assert';
import { loadSpore, Hub } from './spore.mjs';
import { loopbackPair } from './transports/loopback.mjs';

const wasmPath = new URL('../target/wasm32-unknown-unknown/release/spore.wasm', import.meta.url);
const spore = await loadSpore(fs.readFileSync(wasmPath));

const hubA = new Hub(spore.newNode());
const hubB = new Hub(spore.newNode());

const [ta, tb] = loopbackPair();
hubA.addTransport(ta);
hubB.addTransport(tb);

// B subscribes to a topic; A publishes to it; the event must reach B with its
// authenticated author intact.
hubB.node.subscribe('alerts');
const { forwards } = hubA.node.publish('alerts', new TextEncoder().encode('second wave'));
for (const f of forwards) hubB.node.recv(f);

const events = hubB.node.pollFeed();
assert.strictEqual(events.length, 1, 'B drains exactly one event');
assert.strictEqual(new TextDecoder().decode(events[0].data), 'second wave');

// `topic` is the 8-byte SHA-256 prefix (topic_of), not the string.
assert.deepStrictEqual(events[0].topic, spore.topicOf('alerts'), 'topic is the hashed address');

// The author is the authenticated sender, surfaced so the UI can show who said
// it — and never invented when absent.
assert.strictEqual(
  Buffer.from(events[0].from).toString('hex'),
  Buffer.from(hubA.node.addr()).toString('hex'),
  'the feed event names its authenticated author',
);

console.log('FEED OK — publish/subscribe round-trip surfaces the authenticated author');
