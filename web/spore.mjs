// SPORE for the browser (and Node): a wasm node + a hub that relays across
// pluggable transports. Works with plain WebAssembly — no wasm-bindgen.
//
//   import { loadSpore, Hub, ZERO_DEST } from './spore.mjs';
//   const spore = await loadSpore(fetch('./spore.wasm'));
//   const hub = new Hub(spore.newNode());
//   hub.onDeliver = (env) => console.log(spore.payload(env));
//   hub.addTransport(new WebSocketTransport('wss://relay.example/spore'));
//   hub.send(ZERO_DEST, new TextEncoder().encode('hello mesh'));

export const ZERO_DEST = new Uint8Array(8); // all-zero = public

const now = () => Math.floor(Date.now() / 1000);

/** Instantiate the wasm and return a small typed API over it. */
export async function loadSpore(source) {
  let mem; // set after instantiation; the RNG import reads it lazily
  const imports = {
    env: {
      spore_fill_random(ptr, len) {
        crypto.getRandomValues(new Uint8Array(mem.buffer, ptr, len));
      },
    },
  };
  // Accept a Response (fetch), a URL string, an ArrayBuffer, or bytes.
  let result;
  if (source && typeof source.then === 'function') source = await source;
  if (typeof Response !== 'undefined' && source instanceof Response) {
    result = await WebAssembly.instantiateStreaming(source, imports);
  } else if (source instanceof ArrayBuffer || ArrayBuffer.isView(source)) {
    result = await WebAssembly.instantiate(source, imports);
  } else {
    result = await WebAssembly.instantiate(await source, imports);
  }
  const ex = result.instance.exports;
  mem = ex.memory;
  return new Spore(ex);
}

class Spore {
  constructor(ex) {
    this.ex = ex;
  }
  _u8(ptr, len) {
    return new Uint8Array(this.ex.memory.buffer, ptr, len);
  }
  _put(bytes) {
    const p = this.ex.spore_alloc(bytes.length);
    this._u8(p, bytes.length).set(bytes);
    return p;
  }
  _unpack(i64) {
    const u = BigInt.asUintN(64, i64);
    const ptr = Number(u >> 32n);
    const len = Number(u & 0xffffffffn);
    const out = this._u8(ptr, len).slice(); // copy out of wasm memory
    this.ex.spore_free(ptr, len);
    return out;
  }
  _parse(blob) {
    const dv = new DataView(blob.buffer, blob.byteOffset, blob.byteLength);
    let o = 0;
    const list = () => {
      const n = dv.getUint32(o, true);
      o += 4;
      const arr = [];
      for (let i = 0; i < n; i++) {
        const l = dv.getUint32(o, true);
        o += 4;
        arr.push(blob.slice(o, o + l));
        o += l;
      }
      return arr;
    };
    const forwards = list();
    const delivered = list();
    return { forwards, delivered };
  }

  /** Create a node handle (call node.free() when done). */
  newNode() {
    return new SporeNode(this, this.ex.spore_node_new());
  }
  /** The payload of a delivered envelope. */
  payload(env) {
    const p = this._put(env);
    const out = this._unpack(this.ex.spore_env_payload(p, env.length));
    this.ex.spore_free(p, env.length);
    return out;
  }
  /** Verify a delivered envelope's signature. */
  verify(env) {
    const p = this._put(env);
    const ok = this.ex.spore_env_verify(p, env.length) !== 0;
    this.ex.spore_free(p, env.length);
    return ok;
  }
}

class SporeNode {
  constructor(s, ptr) {
    this.s = s;
    this.ptr = ptr;
  }
  free() {
    this.s.ex.spore_node_free(this.ptr);
    this.ptr = 0;
  }
  addr() {
    const p = this.s.ex.spore_alloc(8);
    this.s.ex.spore_node_addr(this.ptr, p);
    const a = this.s._u8(p, 8).slice();
    this.s.ex.spore_free(p, 8);
    return a;
  }
  subscribe(topic) {
    const t = new TextEncoder().encode(topic);
    const p = this.s._put(t);
    this.s.ex.spore_node_subscribe(this.ptr, p, t.length);
    this.s.ex.spore_free(p, t.length);
  }
  send(dest, payload) {
    const dp = this.s._put(dest);
    const pp = this.s._put(payload);
    const packed = this.s.ex.spore_node_send(this.ptr, dp, pp, payload.length, now());
    this.s.ex.spore_free(dp, dest.length);
    this.s.ex.spore_free(pp, payload.length);
    return this.s._parse(this.s._unpack(packed));
  }
  recv(bytes) {
    const bp = this.s._put(bytes);
    const packed = this.s.ex.spore_node_recv(this.ptr, bp, bytes.length, now());
    this.s.ex.spore_free(bp, bytes.length);
    return this.s._parse(this.s._unpack(packed));
  }
}

/**
 * A gateway node: one wasm node + N transports. A frame received on any
 * transport is fed to the router and its forwards relayed onto the others —
 * the browser twin of Rust's `bridge::hub`.
 */
export class Hub {
  constructor(node) {
    this.node = node;
    this.transports = [];
    this.onDeliver = null; // (envelopeBytes) => void
  }
  /** Attach a transport; it must call `t.receive(bytes)` on inbound frames. */
  addTransport(t) {
    t.receive = (bytes) => this._rx(t, bytes);
    this.transports.push(t);
    return t;
  }
  _rx(from, bytes) {
    const { forwards, delivered } = this.node.recv(bytes);
    for (const env of delivered) if (this.onDeliver) this.onDeliver(env);
    this._dispatch(forwards, from);
  }
  _dispatch(forwards, except) {
    for (const f of forwards)
      for (const t of this.transports) if (t !== except) t.send(f);
  }
  /** Originate a message from this node onto every transport. */
  send(dest, payload) {
    const { forwards } = this.node.send(dest, payload);
    this._dispatch(forwards, null);
  }
}

/** Base transport: subclasses implement `send(bytes)` and call `receive(bytes)`. */
export class Transport {
  send(_bytes) {
    throw new Error('transport must implement send()');
  }
  receive(_bytes) {} // replaced by Hub.addTransport
}
