// End-to-end test of the browser stack, runnable in Node (which runs wasm):
// two SPORE nodes, each in its own hub, linked by a transport; one publishes a
// signed message and the other receives + verifies it.
//
//   cargo build --release --lib --target wasm32-unknown-unknown
//   node web/test.mjs
import fs from 'node:fs';
import assert from 'node:assert';
import { loadSpore, Hub, ZERO_DEST, FLAG_ENCRYPTED, FLAG_RATCHET } from './spore.mjs';
import { loopbackPair } from './transports/loopback.mjs';

const wasmPath = new URL('../target/wasm32-unknown-unknown/release/spore.wasm', import.meta.url);
const spore = await loadSpore(fs.readFileSync(wasmPath));

const hubA = new Hub(spore.newNode());
const hubB = new Hub(spore.newNode());

let got = null;
hubB.onDeliver = (env) => {
  assert.ok(spore.verify(env), 'signature must verify');
  got = new TextDecoder().decode(spore.payload(env));
};

const [ta, tb] = loopbackPair();
hubA.addTransport(ta);
hubB.addTransport(tb);

const msg = 'hello over wasm + a transport';
hubA.send(ZERO_DEST, new TextEncoder().encode(msg));

await new Promise((r) => setTimeout(r, 50)); // let the loopback microtasks flush

assert.strictEqual(got, msg, 'B should receive exactly what A published');
const [a, b] = [hubA.node.addr(), hubB.node.addr()];
assert.notDeepStrictEqual(a, b, 'the two nodes have distinct addresses');

// §7 prekey ring across the wasm boundary. The browser node persists seed + ring
// to localStorage; if these exports are wrong, a page reload silently loses the
// ability to open mail already sealed to it, which no other test would notice.
{
  const ring = hubA.node.prekeyRing();
  assert.ok(ring.length >= 2 + 68, `ring blob looks sane, got ${ring.length}`);
  assert.strictEqual(ring[0], 1, 'ring format version');

  // A different node restores A's ring and thereby A's advertised prekey.
  const other = spore.newNode();
  assert.ok(other.restorePrekeyRing(ring), 'a well-formed ring restores');
  assert.deepStrictEqual(other.prekeyRing(), ring, 'and round-trips byte for byte');

  // Malformed blobs are refused rather than accepted or thrown on.
  assert.strictEqual(other.restorePrekeyRing(new Uint8Array(0)), false, 'empty');
  assert.strictEqual(other.restorePrekeyRing(new Uint8Array([1, 0])), false, 'zero entries');
  const lying = Uint8Array.from(ring);
  lying[2] ^= 0xff; // public half no longer matches its secret
  assert.strictEqual(other.restorePrekeyRing(lying), false, 'public/secret mismatch');
  assert.deepStrictEqual(other.prekeyRing(), ring, 'a refused restore changes nothing');
}
// W1: the sealed DM path across the wasm boundary. `send()` is the raw unsealed
// call every earlier test used, so none of them would notice if sealing were
// broken, absent, or quietly falling back to cleartext.
{
  // Nobody has heard anybody's ANNOUNCE yet, so there is no key to seal to and
  // the node must say so rather than let a UI promise a padlock.
  assert.strictEqual(hubA.node.canSealTo(b), false, 'no prekey known yet');

  // Meet: each side floods an ANNOUNCE, teaching the other its prekey.
  for (const [from, to] of [[hubA, hubB], [hubB, hubA]]) {
    for (const f of from.node.announce()) to.node.recv(f);
  }
  assert.strictEqual(hubA.node.canSealTo(b), true, 'B\'s prekey is known now');

  const secret = 'north pier at midnight';
  const { forwards } = hubA.node.sendDirect(b, new TextEncoder().encode(secret));
  assert.ok(forwards.length > 0, 'a DM produces something to send');

  // The ciphertext must not contain the plaintext — the check that fails if
  // sealing silently degrades to cleartext, which is the one failure mode a
  // round-trip test alone cannot see.
  for (const f of forwards) {
    assert.ok(
      !Buffer.from(f).includes(Buffer.from(secret)),
      'the wire must not carry the plaintext',
    );
  }

  const { delivered } = hubB.node.recv(forwards[0]);
  assert.strictEqual(delivered.length, 1, 'B is the destination');
  const env = delivered[0];

  // Key the thread on the *authenticated* sender, never a claimed field.
  assert.deepStrictEqual(spore.src(env), a, 'the sender proves itself');
  const flags = spore.flags(env);
  assert.ok(flags & FLAG_ENCRYPTED, 'and it arrived sealed');

  const opened = hubB.node.openDm(a, spore.payload(env), !!(flags & FLAG_RATCHET));
  assert.strictEqual(new TextDecoder().decode(opened), secret, 'B opens it');

  // A stranger cannot: wrong sender => no key => null, not a throw and not junk.
  const stranger = spore.newNode();
  assert.strictEqual(
    stranger.openDm(a, spore.payload(env), !!(flags & FLAG_RATCHET)),
    null,
    'a node it was not sealed to gets null, honestly',
  );
}

console.log('WEB OK — A published, B received + verified over a transport');
console.log('  DM: sealed, sender authenticated, opened by the recipient only');
console.log('  node A addr:', Buffer.from(a).toString('hex'));
console.log('  node B addr:', Buffer.from(b).toString('hex'));
