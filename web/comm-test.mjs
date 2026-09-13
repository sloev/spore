// The M10-C command ABI, driven from JS over the real wasm module.
//
//   cargo build --release --lib --target wasm32-unknown-unknown
//   node web/comm-test.mjs
//
// This is the test the M10 sequencing rests on. The stores are Rust, and the
// claim is that a host reaches them through *one* entry point that works the
// same over wasm and over IPC. A Rust unit test cannot check that — it never
// crosses the boundary. This does.
//
// It drives the app's own codec (`web/app/communicator.mjs`) rather than a copy,
// so a codec change that breaks the app breaks this too. The raw-byte cases
// below build their commands by hand on purpose: malformed input is the one
// thing the real codec will not produce.
import fs from 'node:fs';
import assert from 'node:assert';
import { Communicator, CmdWriter, CMD, OK, ERR_BAD_COMMAND } from './app/communicator.mjs';

const wasmPath = new URL('../target/wasm32-unknown-unknown/release/spore.wasm', import.meta.url);
let ex;
const { instance } = await WebAssembly.instantiate(fs.readFileSync(wasmPath), {
  env: {
    spore_fill_random: (ptr, len) => crypto.getRandomValues(new Uint8Array(ex.memory.buffer, ptr, len)),
    // A wasm module cannot import conditionally, so these must exist even for a
    // node that never uses the JS spill store.
    spore_store_put: () => {}, spore_store_get: () => 0n,
    spore_store_remove: () => {}, spore_store_ids: () => 0n,
  },
});
ex = instance.exports;

const comm = new Communicator(ex);

/** Send raw bytes and return the response tag, bypassing the codec. */
function rawTag(bytes) {
  const ptr = ex.spore_alloc(bytes.length);
  new Uint8Array(ex.memory.buffer, ptr, bytes.length).set(bytes);
  const packed = ex.spore_comm_call(comm.ptr, ptr, bytes.length);
  ex.spore_free(ptr, bytes.length);
  const u = BigInt.asUintN(64, BigInt(packed));
  const outPtr = Number(u >> 32n);
  const outLen = Number(u & 0xffffffffn);
  const tag = new Uint8Array(ex.memory.buffer, outPtr, outLen)[0];
  ex.spore_free(outPtr, outLen);
  return tag;
}

let failures = 0;
function test(name, fn) {
  try { fn(); console.log(`  ok  ${name}`); }
  catch (e) { failures++; console.log(`FAIL  ${name}\n      ${e.message}`); }
}

const ADA = '0101010101010101';
const ID = '09090909090909090909090909090909';

console.log('M10-C command ABI, over the real wasm module:');

test('a message crosses the boundary and comes back intact', () => {
  assert.strictEqual(comm.threadReceive({ from: ADA, body: 'hello from js', sealed: true, at: 10 }), ADA);
  const [m] = comm.threadMessages(ADA);
  assert.strictEqual(m.id, null, 'received mail carries no id');
  assert.strictEqual(m.self, false);
  assert.strictEqual(m.sealed, true);
  assert.strictEqual(m.status, 'received');
  assert.strictEqual(m.at, 10);
  assert.strictEqual(m.body, 'hello from js');
});

test('an unauthenticated sender is filed nowhere but counted', () => {
  assert.strictEqual(comm.threadReceive({ from: null, body: 'spoofed', sealed: false, at: 11 }), null);
  assert.strictEqual(comm.threadConversations().length, 1, 'only the authenticated one has a conversation');
  assert.strictEqual(comm.threadUnauthenticatedCount(), 1, 'and it did not vanish silently');
});

test('an ack travels by id', () => {
  comm.threadSend({ to: ADA, id: ID, body: 'out', sealed: false, at: 12 });
  assert.strictEqual(comm.threadSetStatus(ID, 'acked'), true);
  assert.strictEqual(comm.threadSetStatus('00'.repeat(16), 'acked'), false, 'an id we never sent moves nothing');
});

test('a malformed command is an error response, not a dead module', () => {
  // The whole point: a page with a version skew must get an answer.
  assert.strictEqual(rawTag(Uint8Array.from([0xff])), ERR_BAD_COMMAND, 'unknown tag');
  assert.strictEqual(rawTag(Uint8Array.from([CMD.THREAD_MESSAGES])), ERR_BAD_COMMAND, 'truncated');
  assert.strictEqual(
    rawTag(new CmdWriter(CMD.THREAD_MESSAGES).bytes(Array(9).fill(1)).out()),
    ERR_BAD_COMMAND, 'trailing',
  );
  // And the module still works afterwards, which is the part that matters.
  assert.strictEqual(typeof comm.threadTotalUnread(), 'number', 'still alive');
});

test('a hostile length inside a command allocates nothing', () => {
  const w = new CmdWriter(CMD.TOPIC_REMEMBER).bytes(Array(8).fill(2)).u32(0xffffffff);
  assert.strictEqual(rawTag(w.out()), ERR_BAD_COMMAND);
  assert.strictEqual(typeof comm.threadTotalUnread(), 'number', 'still alive');
});

test('contacts join local labels against claims, and say which is which', () => {
  comm.contactSetLabel(ADA, 'Ada');
  const peers = [{ addrHex: ADA, claimedName: 'Ada Lovelace', ageSecs: 5, hasPrekey: true }];
  const [row] = comm.contactRows(peers, { view: 'contacts' });
  assert.strictEqual(row.name, 'Ada', 'the label the user typed wins');
  assert.strictEqual(row.claimedName, 'Ada Lovelace', 'and the claim is still there for context');
  assert.strictEqual(row.nameIsClaim, false);
  assert.strictEqual(row.hasPrekey, true);

  // The same store, a caller that has heard from nobody: the claim is gone.
  const [alone] = comm.contactRows([], { view: 'contacts' });
  assert.strictEqual(alone.claimedName, null);
  assert.strictEqual(alone.ageSecs, null);
});

test('topics keep names the mesh cannot supply, and posts it can', () => {
  const topic = '0202020202020202';
  comm.topicRemember(topic, 'tides');
  comm.topicReceive({ topicHex: topic, from: null, body: 'high at 14:20', at: 20 });
  assert.strictEqual(comm.topicNames().get(topic), 'tides');
  const [post] = comm.topicPosts(topic);
  assert.strictEqual(post.from, null, 'an unsigned feed post has no sender, which is ordinary here');
  assert.strictEqual(post.body, 'high at 14:20');
});

test('save and load round-trip every store through the host', () => {
  const blob = comm.save();
  const fresh = new Communicator(ex);
  assert.strictEqual(fresh.load(blob), true, 'the blob is portable, not a handle into the first one');
  assert.strictEqual(fresh.threadMessages(ADA).length, 2);
  assert.strictEqual(fresh.contactRows([], { view: 'contacts' })[0].label, 'Ada');
  assert.strictEqual(fresh.topicNames().get('0202020202020202'), 'tides');
  fresh.free();
});

test('a load that fails leaves every store untouched', () => {
  const before = comm.threadMessages(ADA).length;
  assert.strictEqual(comm.load(Uint8Array.from([0, 0, 0, 1, 99, 0, 0, 0, 0, 0, 0, 0, 0])), false);
  assert.strictEqual(comm.threadMessages(ADA).length, before, 'a refused blob changes nothing');
});

comm.free();

console.log(failures ? `\n${failures} failed` : '\nCOMM ABI OK — the stores are reachable from JS through one entry point');
process.exit(failures ? 1 : 0);
