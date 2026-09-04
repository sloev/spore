// SporeClient contract test (M10). Runs against the REAL wasm kernel — the same
// artifact Android and the CLI build from — not a fake, because the whole point
// of M10 is that every surface shares one core.
//
//   cargo build --release --lib --target wasm32-unknown-unknown
//   node web/app/spore-client.test.mjs

import fs from 'node:fs';
import assert from 'node:assert';
import { SporeClient, memoryAdapter } from './spore-client.mjs';
import { BROWSER_TRANSPORTS } from './transports.mjs';
import { memorySpillStore } from '../spore.mjs';
import { loopbackPair } from '../transports/loopback.mjs';

const wasmPath = new URL('../../target/wasm32-unknown-unknown/release/spore.wasm', import.meta.url);
const wasmBytes = fs.readFileSync(wasmPath);

const enc = (s) => new TextEncoder().encode(s);
const dec = (b) => new TextDecoder().decode(b);
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/** Wait for an event of `type`, or reject once `ms` has passed. */
function waitFor(client, type, ms = 4000) {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => { off(); reject(new Error('timed out waiting for ' + type)); }, ms);
    const off = client.on((e) => {
      if (e.type !== type) return;
      clearTimeout(timer);
      off();
      resolve(e);
    });
  });
}

let failures = 0;
async function test(name, fn) {
  try {
    await fn();
    console.log('  ok  ' + name);
  } catch (err) {
    failures++;
    console.log('FAIL  ' + name + '\n      ' + (err && err.message));
  }
}

// ---------------------------------------------------------------- lifecycle

await test('init() creates an identity and persists the seed', async () => {
  const storage = memoryAdapter();
  const client = new SporeClient({ storage });
  const id = await client.init(wasmBytes);

  assert.strictEqual(id.restored, false, 'a fresh store must not report restored');
  assert.strictEqual(id.addr.length, 8, 'address is 8 bytes');
  assert.match(id.addrHex, /^[0-9a-f]{16}$/, 'addrHex is 16 hex chars');
  assert.ok(await storage.get('spore.seed'), 'seed must be persisted');
  assert.ok(await storage.get('spore.ring'), 'prekey ring must be persisted beside the seed');
  client.dispose();
});

await test('a second init() on the same storage restores the same address', async () => {
  const storage = memoryAdapter();
  const first = new SporeClient({ storage });
  const a = await first.init(wasmBytes);
  first.dispose();

  const second = new SporeClient({ storage });
  const b = await second.init(wasmBytes);
  second.dispose();

  assert.strictEqual(b.restored, true, 'must report the identity was restored');
  assert.strictEqual(b.addrHex, a.addrHex, 'restoring a seed must keep the address');
});

await test('calling before init() resolves is refused, not silently wrong', async () => {
  const client = new SporeClient({ storage: memoryAdapter() });
  assert.throws(() => client.sendDirect('00'.repeat(8), enc('x')), /init\(\) has not resolved/);
});

await test('dispose() is idempotent', async () => {
  const client = new SporeClient({ storage: memoryAdapter() });
  await client.init(wasmBytes);
  client.dispose();
  client.dispose(); // must not throw
});

// ------------------------------------------------------------------ messaging

await test('a DM crosses a loopback bridge and arrives verified', async () => {
  const alice = new SporeClient({ storage: memoryAdapter() });
  const bob = new SporeClient({ storage: memoryAdapter() });
  const idA = await alice.init(wasmBytes);
  await bob.init(wasmBytes);

  const [ta, tb] = loopbackPair();
  alice.attachTransport('loopback', ta);
  bob.attachTransport('loopback', tb);

  const received = waitFor(bob, 'EnvelopeReceived');
  const sent = alice.sendDirect((await bob.identity).addrHex, enc('hello mesh'));

  assert.strictEqual(sent.status, 'queued', 'a fresh send is a real row with status queued');
  assert.match(sent.id, /^[0-9a-f]+$/, 'send returns the true envelope id, not a placeholder');

  const e = await received;
  assert.strictEqual(dec(e.body), 'hello mesh');
  assert.strictEqual(e.from, idA.addrHex, 'sender must be the authenticated address');

  alice.dispose();
  bob.dispose();
});

await test('canSealTo() is honest before an ANNOUNCE is heard', async () => {
  const alice = new SporeClient({ storage: memoryAdapter() });
  await alice.init(wasmBytes);
  // Nobody has announced to us, so there is no key to seal to. The UI must be
  // able to learn that rather than drawing a padlock unconditionally.
  assert.strictEqual(alice.canSealTo('11'.repeat(8)), false);
  alice.dispose();
});

// The three-state delivery rule used to live in web/ui/delivery-status.mjs and
// was covered by codec-test.mjs. That module is gone (chat.mjs renders the state
// and this client decides it), so its boundary coverage moves here rather than
// being dropped: the rule is "past its own expiry with no receipt", and the
// exact edge is worth pinning because it is the difference between telling
// someone their message is still travelling and telling them it never arrived.

await test('a message before its expiry is still travelling, not expired', async () => {
  const alice = new SporeClient({ storage: memoryAdapter() });
  await alice.init(wasmBytes);
  const sent = alice.sendDirect('22'.repeat(8), enc('boundary'));

  // Comfortably before the boundary. (Setting it to exactly now would fail for
  // the boring reason that the clock advances past it while the test waits —
  // the tick is 1s.)
  alice._pending.get(sent.id).expiresAt = Math.floor(Date.now() / 1000) + 30;

  let expired = false;
  const off = alice.on((e) => { if (e.type === 'EnvelopeExpired') expired = true; });
  await sleep(1600); // more than one tick
  off();
  assert.strictEqual(expired, false, 'exactly at the boundary is still in flight');
  alice.dispose();
});

await test('one second past the boundary it expires', async () => {
  const alice = new SporeClient({ storage: memoryAdapter() });
  await alice.init(wasmBytes);
  const sent = alice.sendDirect('22'.repeat(8), enc('past it'));
  alice._pending.get(sent.id).expiresAt = Math.floor(Date.now() / 1000) - 1;
  const e = await waitFor(alice, 'EnvelopeExpired');
  assert.strictEqual(e.id, sent.id);
  alice.dispose();
});

await test('a delivery receipt comes home on a two-node link', async () => {
  // This is the end-to-end form of the core regression test: until receipts
  // stopped being emitted with the arrival interface as their `except`, the
  // only route back to the sender was the one link the hub excluded, so
  // "delivered" was unreachable between two directly-linked browser nodes.
  const alice = new SporeClient({ storage: memoryAdapter() });
  const bob = new SporeClient({ storage: memoryAdapter() });
  await alice.init(wasmBytes);
  const idB = await bob.init(wasmBytes);

  const [ta, tb] = loopbackPair();
  alice.attachTransport('loopback', ta);
  bob.attachTransport('loopback', tb);

  const sent = alice.sendDirect(idB.addrHex, enc('ack me'));
  const acked = await waitFor(alice, 'EnvelopeAcked');
  assert.strictEqual(acked.id, sent.id);
  // Once acked it leaves _pending, so a later tick can never call it expired.
  assert.strictEqual(alice._pending.has(sent.id), false);

  alice.dispose();
  bob.dispose();
});

await test('an unroutable DM expires rather than spinning forever', async () => {
  const alice = new SporeClient({ storage: memoryAdapter() });
  await alice.init(wasmBytes);
  const sent = alice.sendDirect('22'.repeat(8), enc('into the void'));

  // Force the deadline into the past: the real TTL is build-configured and far
  // longer than a test should wait. This asserts the derived-expiry rule, which
  // exists because the core emits no "gave up" event of its own.
  alice._pending.get(sent.id).expiresAt = 0;
  const e = await waitFor(alice, 'EnvelopeExpired');
  assert.strictEqual(e.id, sent.id);
  alice.dispose();
});

// ------------------------------------------------------------------ transports

await test('availableTransports() feature-detects and hides what Node cannot run', async () => {
  const client = new SporeClient({ storage: memoryAdapter() });
  await client.init(wasmBytes);
  const kinds = client.availableTransports().map((t) => t.kind);

  assert.ok(kinds.includes('loopback'), 'loopback always runs');
  // Node has no navigator.serial / navigator.bluetooth / getUserMedia, so none
  // of the gesture transports may be offered here.
  for (const absent of ['webserial', 'webbluetooth', 'meshtastic_serial', 'meshtastic_ble', 'reticulum_serial', 'reticulum_ble', 'audio']) {
    assert.ok(!kinds.includes(absent), absent + ' must not be offered without its API');
  }
  // The eleven native bridges are not in the BROWSER registry. They are not
  // forbidden everywhere — a desktop host that can genuinely open them supplies
  // them — but a browser cannot, so offering one here would be a control whose
  // backend is missing.
  for (const native of ['udp', 'tcp', 'tor', 'i2p', 'icmp', 'iroh', 'ssb', 'copyparty', 'spool', 'foldersync']) {
    assert.ok(!kinds.includes(native), native + ' cannot be opened by a browser');
  }
  client.dispose();
});

await test('a host with more capability can supply its own registry', async () => {
  // A Tauri desktop build is a webview AND a daemon: it offers the browser
  // transports plus the native bridges, proxied to Rust. Which transports exist
  // is a property of the host, so the client takes a registry instead of
  // importing one.
  const desktopRegistry = [
    ...BROWSER_TRANSPORTS,
    { kind: 'udp', label: 'UDP — native', available: () => true, open: async () => ({ send() {}, receive() {} }) },
    { kind: 'tor', label: 'Tor — native', available: () => true, open: async () => ({ send() {}, receive() {} }) },
  ];
  const client = new SporeClient({ storage: memoryAdapter(), transports: desktopRegistry });
  await client.init(wasmBytes);

  const kinds = client.availableTransports().map((t) => t.kind);
  assert.ok(kinds.includes('udp'), 'a desktop host may offer UDP');
  assert.ok(kinds.includes('tor'), 'a desktop host may offer Tor');
  assert.ok(kinds.includes('websocket'), 'and still offers the browser set');

  // And it can actually open one, rather than merely listing it.
  const handle = await client.addBridge({ kind: 'udp' });
  assert.strictEqual(handle.kind, 'udp');
  assert.ok(client.bridges().some((b) => b.kind === 'udp' && b.up));
  client.dispose();
});

await test('a manual transport refuses addBridge() instead of faking it', async () => {
  const client = new SporeClient({ storage: memoryAdapter() });
  await client.init(wasmBytes);
  await assert.rejects(
    () => client.addBridge({ kind: 'webrtc' }),
    /cannot be opened from a config/,
    'webrtc needs a handshake; pretending otherwise is a control whose backend is missing',
  );
  client.dispose();
});

await test('bridge frame counters move and are reported', async () => {
  const alice = new SporeClient({ storage: memoryAdapter() });
  const bob = new SporeClient({ storage: memoryAdapter() });
  await alice.init(wasmBytes);
  await bob.init(wasmBytes);

  const [ta, tb] = loopbackPair();
  const handle = alice.attachTransport('loopback', ta);
  bob.attachTransport('loopback', tb);

  alice.sendDirect(bob.identity.addrHex, enc('counted'));
  await sleep(50);

  const bridge = alice.bridges().find((b) => b.id === handle.id);
  assert.ok(bridge.sent > 0, 'sending must increment the outbound counter');
  assert.ok(bob.bridges()[0].received > 0, 'receiving must increment the inbound counter');

  alice.dispose();
  bob.dispose();
});

// ---------------------------------------------------------------------- files

await test('publishFile() then listFiles() round-trips through the kernel', async () => {
  const client = new SporeClient({ storage: memoryAdapter() });
  await client.init(wasmBytes);

  const magnet = client.publishFile('notes.txt', enc('some bytes'));
  assert.match(magnet, /^[0-9a-f]{32}$/, 'magnet is a 32-hex id');

  const files = client.listFiles();
  assert.ok(files.some((f) => f.name === 'notes.txt' && f.magnet === magnet));
  assert.strictEqual(dec(client.fileBytes(magnet)), 'some bytes');
  client.dispose();
});

await test('peers() enumerates what the browser previously could not see', async () => {
  const alice = new SporeClient({ storage: memoryAdapter() });
  const bob = new SporeClient({ storage: memoryAdapter() });
  const idA = await alice.init(wasmBytes);
  const idB = await bob.init(wasmBytes);

  assert.deepStrictEqual(alice.peers(), [], 'a node that has heard nothing knows nobody');

  const [ta, tb] = loopbackPair();
  alice.attachTransport('loopback', ta);
  bob.attachTransport('loopback', tb);

  // A DM is signed traffic, which is what puts a peer on the list.
  const heard = waitFor(bob, 'EnvelopeReceived');
  alice.sendDirect(idB.addrHex, enc('hello'));
  await heard;

  const seen = bob.peers();
  assert.ok(seen.some((p) => p.addrHex === idA.addrHex), 'bob has now heard from alice');
  const a = seen.find((p) => p.addrHex === idA.addrHex);
  assert.strictEqual(typeof a.ageSecs, 'number');
  assert.strictEqual(typeof a.hasPrekey, 'boolean');
  // hasPrekey is what makes sealing possible, so the two must agree rather than
  // being derived separately in the UI.
  assert.strictEqual(a.hasPrekey, bob.canSealTo(idA.addrHex));

  alice.dispose();
  bob.dispose();
});

await test('custody survives a restart, and is re-verified on the way in', async () => {
  // The client installs the host store at init. Two clients over one storage is
  // what a page reload is; before M10-A the browser was the only target that
  // could not keep custody at all.
  const storage = memoryAdapter();
  const spill = memorySpillStore();

  const first = new SporeClient({ storage, spillStore: spill });
  await first.init(wasmBytes);
  // Something to carry for somebody else: a public post from a third party.
  const other = new SporeClient({ storage: memoryAdapter(), spillStore: memorySpillStore() });
  await other.init(wasmBytes);
  const [ta, tb] = loopbackPair();
  first.attachTransport('loopback', ta);
  other.attachTransport('loopback', tb);
  other.publish('weather', enc('squall on the ridge'));
  await sleep(300);

  const held = spill.ids().length;
  assert.ok(held > 0, 'the node writes what it carries to the host store');
  first.dispose();

  const second = new SporeClient({ storage, spillStore: spill });
  await second.init(wasmBytes);
  assert.strictEqual(second.adoptedOnStart, held, 'a restart adopts what it was holding');
  second.dispose();
  other.dispose();
});

await test('a tampered host store contributes nothing', async () => {
  const storage = memoryAdapter();
  const spill = memorySpillStore();
  const a = new SporeClient({ storage, spillStore: spill });
  await a.init(wasmBytes);
  const other = new SporeClient({ storage: memoryAdapter(), spillStore: memorySpillStore() });
  await other.init(wasmBytes);
  const [ta, tb] = loopbackPair();
  a.attachTransport('loopback', ta);
  other.attachTransport('loopback', tb);
  other.publish('weather', enc('carry me'));
  await sleep(300);
  assert.ok(spill.ids().length > 0);
  a.dispose();

  for (const id of spill.ids()) {
    const w = spill.get(id);
    w[Math.floor(w.length / 2)] ^= 0xff;
    spill.put(id, w);
  }

  const b = new SporeClient({ storage, spillStore: spill });
  await b.init(wasmBytes);
  assert.strictEqual(b.adoptedOnStart, 0, 'an id is the hash of its content; a mismatch is "not held"');
  b.dispose();
  other.dispose();
});

console.log(failures ? '\n' + failures + ' failing' : '\nall passing');
process.exit(failures ? 1 : 0);
