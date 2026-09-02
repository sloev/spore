// SporeClient contract test (M10). Runs against the REAL wasm kernel — the same
// artifact Android and the CLI build from — not a fake, because the whole point
// of M10 is that every surface shares one core.
//
//   cargo build --release --lib --target wasm32-unknown-unknown
//   node web/app/spore-client.test.mjs

import fs from 'node:fs';
import assert from 'node:assert';
import { SporeClient, memoryAdapter } from './spore-client.mjs';
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
  // The eleven daemon-only bridges are not in the registry at all.
  for (const daemon of ['udp', 'tcp', 'tor', 'i2p', 'icmp', 'iroh', 'ssb', 'copyparty', 'spool', 'foldersync']) {
    assert.ok(!kinds.includes(daemon), daemon + ' is daemon-only and must never reach a browser');
  }
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

console.log(failures ? '\n' + failures + ' failing' : '\nall passing');
process.exit(failures ? 1 : 0);
