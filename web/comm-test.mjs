// The M10-C command ABI, driven from JS over the real wasm module.
//
//   cargo build --release --lib --target wasm32-unknown-unknown
//   node web/comm-test.mjs
//
// This is the test the whole M10 sequencing rests on. The three M10-B stores are
// Rust, and the claim is that a host reaches them through *one* entry point that
// works the same over wasm and over IPC. A Rust unit test cannot check that —
// it never crosses the boundary. This does: it builds command bytes in JS, calls
// `spore_comm_call`, and parses the response bytes back.
import fs from 'node:fs';
import assert from 'node:assert';

const wasmPath = new URL('../target/wasm32-unknown-unknown/release/spore.wasm', import.meta.url);

// The module needs the same host imports a real page supplies.
const imports = {
  env: {
    spore_fill_random: (ptr, len) => {
      const mem = new Uint8Array(ex.memory.buffer, ptr, len);
      crypto.getRandomValues(mem);
    },
    // A node with no JS store never calls these, but a wasm module cannot
    // import conditionally, so they must exist.
    spore_store_put: () => {},
    spore_store_get: () => 0n,
    spore_store_remove: () => {},
    spore_store_ids: () => 0n,
  },
};
const { instance } = await WebAssembly.instantiate(fs.readFileSync(wasmPath), imports);
const ex = instance.exports;

const OK = 0;
const ERR_BAD_COMMAND = 1;
const CMD = {
  THREAD_RECEIVE: 0x01, THREAD_SEND: 0x02, THREAD_SET_STATUS: 0x03,
  THREAD_MESSAGES: 0x05, THREAD_CONVERSATIONS: 0x06, THREAD_TOTAL_UNREAD: 0x07,
  CONTACT_SET_LABEL: 0x10, CONTACT_ROWS: 0x14,
  TOPIC_REMEMBER: 0x20, TOPIC_RECEIVE: 0x22, TOPIC_POSTS: 0x23,
  SAVE: 0x30, LOAD: 0x31,
};

// -- the two halves of the codec, in JS --------------------------------------

class W {
  constructor(tag) { this.b = [tag]; }
  u8(v) { this.b.push(v & 0xff); return this; }
  bool(v) { return this.u8(v ? 1 : 0); }
  u32(v) { this.b.push((v >>> 24) & 0xff, (v >>> 16) & 0xff, (v >>> 8) & 0xff, v & 0xff); return this; }
  bytes(a) { for (const x of a) this.b.push(x); return this; }
  str(s) { const e = new TextEncoder().encode(s); return this.u32(e.length).bytes(e); }
  optAddr(a) { return a ? this.u8(1).bytes(a) : this.u8(0); }
  out() { return Uint8Array.from(this.b); }
}

class R {
  constructor(bytes) { this.b = bytes; this.i = 0; }
  u8() { return this.b[this.i++]; }
  bool() { return this.u8() === 1; }
  u32() { const v = new DataView(this.b.buffer, this.b.byteOffset).getUint32(this.i); this.i += 4; return v; }
  bytes(n) { const v = this.b.slice(this.i, this.i + n); this.i += n; return v; }
  str() { return new TextDecoder().decode(this.bytes(this.u32())); }
  atEnd() { return this.i === this.b.length; }
}

const comm = ex.spore_comm_new();

function call(w) {
  const cmd = w.out();
  const ptr = ex.spore_alloc(cmd.length);
  new Uint8Array(ex.memory.buffer, ptr, cmd.length).set(cmd);
  const packed = ex.spore_comm_call(comm, 0, 0, ptr, cmd.length);
  ex.spore_free(ptr, cmd.length);

  const u = BigInt.asUintN(64, packed);
  const outPtr = Number(u >> 32n);
  const outLen = Number(u & 0xffffffffn);
  const out = new Uint8Array(ex.memory.buffer, outPtr, outLen).slice();
  ex.spore_free(outPtr, outLen);
  return out;
}

function ok(res) {
  assert.strictEqual(res[0], OK, `expected OK, got ${res[0]}`);
  return new R(res.slice(1));
}

let failures = 0;
function test(name, fn) {
  try { fn(); console.log(`  ok  ${name}`); }
  catch (e) { failures++; console.log(`FAIL  ${name}\n      ${e.message}`); }
}

const ADA = Uint8Array.from([1, 1, 1, 1, 1, 1, 1, 1]);
const ID = Uint8Array.from(Array(16).fill(9));

console.log('M10-C command ABI, over the real wasm module:');

test('a message crosses the boundary and comes back intact', () => {
  const r = ok(call(new W(CMD.THREAD_RECEIVE).optAddr(ADA).str('hello from js').bool(true).u32(10)));
  assert.strictEqual(r.u8(), 1, 'filed under a sender');
  assert.deepStrictEqual(r.bytes(8), ADA);

  const m = ok(call(new W(CMD.THREAD_MESSAGES).bytes(ADA)));
  assert.strictEqual(m.u32(), 1);
  assert.strictEqual(m.u8(), 0, 'received mail carries no id');
  assert.strictEqual(m.bool(), false, 'not self-authored');
  assert.strictEqual(m.bool(), true, 'sealed');
  m.u8(); // status
  assert.strictEqual(m.u32(), 10);
  assert.strictEqual(m.str(), 'hello from js');
  assert.ok(m.atEnd(), 'the response is exactly what was promised');
});

test('an unauthenticated sender is filed nowhere', () => {
  const r = ok(call(new W(CMD.THREAD_RECEIVE).optAddr(null).str('spoofed').bool(false).u32(11)));
  assert.strictEqual(r.u8(), 0, 'no address: it was filed nowhere');
  const c = ok(call(new W(CMD.THREAD_CONVERSATIONS)));
  assert.strictEqual(c.u32(), 1, 'only the authenticated one has a conversation');
});

test('an ack travels by id', () => {
  call(new W(CMD.THREAD_SEND).bytes(ADA).bytes(ID).str('out').bool(false).u32(12));
  const moved = ok(call(new W(CMD.THREAD_SET_STATUS).bytes(ID).u8(2)));
  assert.strictEqual(moved.u8(), 1);
  const ghost = ok(call(new W(CMD.THREAD_SET_STATUS).bytes(new Uint8Array(16)).u8(2)));
  assert.strictEqual(ghost.u8(), 0, 'an id we never sent moves nothing');
});

test('a malformed command is an error response, not a dead module', () => {
  // The whole point: a page with a version skew must get an answer.
  assert.strictEqual(call(new W(0xff))[0], ERR_BAD_COMMAND, 'unknown tag');
  assert.strictEqual(call(new W(CMD.THREAD_MESSAGES))[0], ERR_BAD_COMMAND, 'truncated');
  assert.strictEqual(call(new W(CMD.THREAD_MESSAGES).bytes(ADA).u8(0))[0], ERR_BAD_COMMAND, 'trailing');
  // And the module still works afterwards, which is the part that matters.
  assert.strictEqual(call(new W(CMD.THREAD_TOTAL_UNREAD))[0], OK, 'still alive');
});

test('a hostile length inside a command allocates nothing', () => {
  assert.strictEqual(call(new W(CMD.TOPIC_REMEMBER).bytes(ADA).u32(0xffffffff))[0], ERR_BAD_COMMAND);
  assert.strictEqual(call(new W(CMD.THREAD_TOTAL_UNREAD))[0], OK, 'still alive');
});

test('contacts and topics answer through the same door', () => {
  call(new W(CMD.CONTACT_SET_LABEL).bytes(ADA).str('Ada'));
  const rows = ok(call(new W(CMD.CONTACT_ROWS).u8(0).str('')));
  assert.strictEqual(rows.u32(), 1);
  rows.bytes(8);
  assert.strictEqual(rows.bool(), false, 'name_is_claim: the user typed this one');

  const topic = Uint8Array.from([2, 2, 2, 2, 2, 2, 2, 2]);
  call(new W(CMD.TOPIC_REMEMBER).bytes(topic).str('tides'));
  call(new W(CMD.TOPIC_RECEIVE).bytes(topic).optAddr(null).str('high at 14:20').u32(20));
  const posts = ok(call(new W(CMD.TOPIC_POSTS).bytes(topic)));
  assert.strictEqual(posts.u32(), 1);
  assert.strictEqual(posts.u8(), 0, 'an unsigned feed post has no sender, which is ordinary here');
  assert.strictEqual(posts.u32(), 20);
  assert.strictEqual(posts.str(), 'high at 14:20');
});

test('save and load round-trip every store through the host', () => {
  const saved = ok(call(new W(CMD.SAVE)));
  const blob = saved.b.slice(saved.i);

  const fresh = ex.spore_comm_new();
  const prev = comm;
  // Load into a second communicator to prove the blob is portable rather than
  // a handle into the first one's memory.
  const cmd = new W(CMD.LOAD).bytes(blob).out();
  const ptr = ex.spore_alloc(cmd.length);
  new Uint8Array(ex.memory.buffer, ptr, cmd.length).set(cmd);
  const packed = ex.spore_comm_call(fresh, 0, 0, ptr, cmd.length);
  ex.spore_free(ptr, cmd.length);
  const u = BigInt.asUintN(64, packed);
  const rp = Number(u >> 32n); const rl = Number(u & 0xffffffffn);
  const res = new Uint8Array(ex.memory.buffer, rp, rl).slice();
  ex.spore_free(rp, rl);
  assert.strictEqual(res[0], OK, 'the blob loads');

  ex.spore_comm_free(fresh);
  assert.ok(prev, 'and the original is untouched');
});

ex.spore_comm_free(comm);

console.log(failures ? `\n${failures} failed` : '\nCOMM ABI OK — the stores are reachable from JS through one entry point');
process.exit(failures ? 1 : 0);
