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

  /** Create a node handle (call node.free() when done). Pass a 32-byte `seed`
   * (its whole identity) to restore a persisted node; omit it for a fresh one. */
  newNode(seed = null) {
    if (seed) {
      const p = this._put(seed);
      const node = new SporeNode(this, this.ex.spore_node_new_seeded(p));
      this.ex.spore_free(p, seed.length);
      return node;
    }
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
  /** A delivered envelope's flags byte. `ENCRYPTED` (0x01) says the payload is
   * sealed; `RATCHET` (0x40) says which scheme opens it — pass that as
   * `ratcheted` to `openDm`. */
  flags(env) {
    const p = this._put(env);
    const f = this.ex.spore_env_flags(p, env.length);
    this.ex.spore_free(p, env.length);
    return f;
  }
  /** The **authenticated** sender address (8 bytes), or `null`.
   *
   * `null` for an unsigned envelope, a signature that does not verify, and
   * `SRC8` — which carries an address the envelope cannot prove. A thread list
   * keyed on anything weaker than this is spoofable, so this is the only sender
   * the UI should ever key on. */
  src(env) {
    const p = this._put(env);
    const out = this._unpack(this.ex.spore_env_src(p, env.length));
    this.ex.spore_free(p, env.length);
    return out.length === 8 ? out : null;
  }

  /** Seal a message under a 32-byte topic pre-shared key (W3). */
  topicSeal(msg, psk) {
    const mp = this._put(msg);
    const pp = this._put(psk);
    const out = this._unpack(this.ex.spore_topic_seal(mp, msg.length, pp));
    this.ex.spore_free(mp, msg.length);
    this.ex.spore_free(pp, psk.length);
    return out;
  }

  /** Open a topic-sealed payload with the 32-byte key. null on failure. */
  topicOpen(ct, psk) {
    const cp = this._put(ct);
    const pp = this._put(psk);
    const out = this._unpack(this.ex.spore_topic_open(cp, ct.length, pp));
    this.ex.spore_free(cp, ct.length);
    this.ex.spore_free(pp, psk.length);
    return out.length ? out : null;
  }
}

/** Envelope flag bits the DM path needs (§2). */
export const FLAG_ENCRYPTED = 0x01;
export const FLAG_RATCHET = 0x40;

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
  /** The node's 32-byte signing seed — persist it and pass to `newNode(seed)`
   * to keep the same identity (address) across restarts. */
  seed() {
    const p = this.s.ex.spore_alloc(32);
    this.s.ex.spore_node_seed(this.ptr, p);
    const s = this.s._u8(p, 32).slice();
    this.s.ex.spore_free(p, 32);
    return s;
  }
  /** The prekey ring (§7) — persist it *alongside* `seed()`.
   *
   * The seed restores the identity; it does not restore prekey secrets, because
   * those are random and that is exactly what makes deleting them mean something
   * (S-022). A page that saves only the seed comes back unable to open mail
   * sealed to any prekey the node had rotated to.
   *
   * Secret material: every byte opens mail. Guard it like the seed. */
  prekeyRing() {
    const n = this.s.ex.spore_node_prekey_ring_len(this.ptr);
    const p = this.s.ex.spore_alloc(n);
    const wrote = this.s.ex.spore_node_prekey_ring(this.ptr, p);
    const r = this.s._u8(p, wrote).slice();
    this.s.ex.spore_free(p, n);
    return r;
  }
  /** Restore a ring from `prekeyRing()`. Returns false for a malformed blob, in
   * which case the node is left as it was. */
  restorePrekeyRing(blob) {
    const p = this.s._put(blob);
    const ok = this.s.ex.spore_node_restore_prekey_ring(this.ptr, p, blob.length);
    this.s.ex.spore_free(p, blob.length);
    return ok === 1;
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

  /** Build this node's ANNOUNCE (§4) — prekey, topics, petname — to flood.
   *
   * Until a peer has heard one of these it has no key to seal to us, so it will
   * fall back to cleartext. Send one on connect and then on the §5.4b Trickle
   * schedule (5→80 min, reset on novelty): beaconing is the scheduled duty that
   * belongs to the runtime rather than the core, because emitting on a transport
   * is across the boundary the core is defined against. */
  announce() {
    const packed = this.s.ex.spore_node_announce(this.ptr, now());
    return this.s._parse(this.s._unpack(packed)).forwards;
  }

  /** Send a sealed, signed DM (W1). `send()` above is the raw unsealed path —
   * the one a protocol implementer uses to prove a transport carries bytes, not
   * the one a person sends another person.
   *
   * Sealing is layered by what this node knows about `dest`: a live §7 ratchet
   * session, else a one-shot seal to their prekey, else **cleartext**, because a
   * node that has never heard their ANNOUNCE has no key to seal to. Ask
   * `canSealTo(dest)` first and say so — never draw a padlock unconditionally. */
  sendDirect(dest, payload) {
    const dp = this.s._put(dest);
    const pp = this.s._put(payload);
    const packed = this.s.ex.spore_node_send_direct(this.ptr, dp, pp, payload.length, now());
    this.s.ex.spore_free(dp, dest.length);
    this.s.ex.spore_free(pp, payload.length);
    return this.s._parse(this.s._unpack(packed));
  }

  /** Would a DM to `dest` right now actually be sealed? Ask before promising it. */
  canSealTo(dest) {
    const dp = this.s._put(dest);
    const ok = this.s.ex.spore_node_send_direct_sealed(this.ptr, dp);
    this.s.ex.spore_free(dp, dest.length);
    return ok === 1;
  }

  /** Open a delivered DM. `null` when it does not open — which is a real state,
   * not a bug: a prekey may have expired past the offline window. Say "couldn't
   * decrypt this — the key may have expired" rather than dropping it silently. */
  openDm(sender, sealed, ratcheted) {
    const sp = this.s._put(sender);
    const bp = this.s._put(sealed);
    const packed = this.s.ex.spore_node_open_dm(this.ptr, sp, bp, sealed.length, ratcheted ? 1 : 0, now());
    this.s.ex.spore_free(sp, sender.length);
    this.s.ex.spore_free(bp, sealed.length);
    const out = this.s._unpack(packed);
    return out.length ? out : null;
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
  /** Detach a transport (does not close it — the caller owns its lifecycle). */
  removeTransport(t) {
    this.transports = this.transports.filter((x) => x !== t);
    t.receive = () => {};
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
