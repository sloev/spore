// The JS half of the M10-C command ABI.
//
// One entry point — `spore_comm_call` — takes command bytes and returns response
// bytes, so this file is a codec and nothing else. It holds no application
// state: the stores are in Rust, and everything here is translation.
//
// **Why bytes and not thirty exported functions.** `wasm.rs` already exports 34,
// `ffi.rs` 20 and `android/jni` 64 — three divergent subsets of one kernel, with
// the communicator written again on top of each. That is what M10 exists to
// stop, so the app layer crosses as bytes through a single door. The same
// commands work unchanged over IPC, which is what lets a desktop host run a
// native node behind the same screens (M10-F).
//
// Addresses are hex strings on this side of the line and eight raw bytes on the
// other, because that is what each side already uses everywhere else; converting
// at the boundary is cheaper than making either side pretend.

export const OK = 0;
export const ERR_BAD_COMMAND = 1;

export const CMD = {
  THREAD_RECEIVE: 0x01,
  THREAD_SEND: 0x02,
  THREAD_SET_STATUS: 0x03,
  THREAD_MARK_READ: 0x04,
  THREAD_MESSAGES: 0x05,
  THREAD_CONVERSATIONS: 0x06,
  THREAD_TOTAL_UNREAD: 0x07,
  THREAD_UNAUTHENTICATED: 0x08,
  CONTACT_SET_LABEL: 0x10,
  CONTACT_SET_FOLLOWING: 0x11,
  CONTACT_SET_BLOCKED: 0x12,
  CONTACT_REMOVE: 0x13,
  CONTACT_ROWS: 0x14,
  TOPIC_REMEMBER: 0x20,
  TOPIC_FORGET: 0x21,
  TOPIC_RECEIVE: 0x22,
  TOPIC_POSTS: 0x23,
  TOPIC_NAMED: 0x24,
  SAVE: 0x30,
  LOAD: 0x31,
};

/** Message status codes, matching `MessageStatus::code` in Rust. */
export const STATUS = { queued: 0, sent: 1, acked: 2, expired: 3, received: 4 };
const STATUS_NAME = ['queued', 'sent', 'acked', 'expired', 'received'];

export function hex(bytes) {
  return [...bytes].map((b) => b.toString(16).padStart(2, '0')).join('');
}

export function unhex(s) {
  const out = new Uint8Array(s.length / 2);
  for (let i = 0; i < out.length; i++) out[i] = parseInt(s.slice(i * 2, i * 2 + 2), 16);
  return out;
}

/** Builds a command. Every length prefix is a big-endian u32. */
export class CmdWriter {
  constructor(tag) { this.b = [tag]; }
  u8(v) { this.b.push(v & 0xff); return this; }
  bool(v) { return this.u8(v ? 1 : 0); }
  u32(v) { this.b.push((v >>> 24) & 0xff, (v >>> 16) & 0xff, (v >>> 8) & 0xff, v & 0xff); return this; }
  bytes(a) { for (const x of a) this.b.push(x); return this; }
  str(s) { const e = new TextEncoder().encode(s || ''); return this.u32(e.length).bytes(e); }
  /** An address as a hex string, or null/undefined for "no address". */
  optAddrHex(h) { return h ? this.u8(1).bytes(unhex(h)) : this.u8(0); }
  out() { return Uint8Array.from(this.b); }
}

/** Reads a response. */
export class RespReader {
  constructor(bytes) { this.b = bytes; this.i = 0; }
  u8() { return this.b[this.i++]; }
  bool() { return this.u8() === 1; }
  u32() {
    const v = new DataView(this.b.buffer, this.b.byteOffset).getUint32(this.i);
    this.i += 4;
    return v;
  }
  bytes(n) { const v = this.b.slice(this.i, this.i + n); this.i += n; return v; }
  hex(n) { return hex(this.bytes(n)); }
  str() { return new TextDecoder().decode(this.bytes(this.u32())); }
  /** A presence byte then an address; null when absent. */
  optAddrHex() { return this.u8() === 1 ? this.hex(8) : null; }
  atEnd() { return this.i === this.b.length; }
}

/**
 * The stores, reached through one wasm call.
 *
 * Calls are synchronous, which is not an accident worth losing: wasm is, so the
 * screens keep rendering from a plain read rather than awaiting one. Only
 * persistence is async, because the host's storage is.
 */
export class Communicator {
  /**
   * @param {object} ex  the wasm instance's exports
   * @param {number} nodePtr  the node whose peer table answers contact rows, or 0
   */
  constructor(ex, nodePtr = 0) {
    this.ex = ex;
    this.nodePtr = nodePtr;
    this.ptr = ex.spore_comm_new();
  }

  free() {
    if (this.ptr) { this.ex.spore_comm_free(this.ptr); this.ptr = 0; }
  }

  /**
   * Run one command. Throws on a malformed command rather than returning a
   * sentinel: the Rust side answers `ERR_BAD_COMMAND` instead of trapping so the
   * module survives, and on this side a bad command is a bug in *this file* —
   * something a caller could act on would imply it was expected.
   */
  call(w, now = 0) {
    const cmd = w.out();
    const { ex } = this;
    const ptr = ex.spore_alloc(cmd.length);
    new Uint8Array(ex.memory.buffer, ptr, cmd.length).set(cmd);
    const packed = ex.spore_comm_call(this.ptr, this.nodePtr, now, ptr, cmd.length);
    ex.spore_free(ptr, cmd.length);

    const u = BigInt.asUintN(64, BigInt(packed));
    const outPtr = Number(u >> 32n);
    const outLen = Number(u & 0xffffffffn);
    // Copied off the wasm heap before freeing: `memory.buffer` is detached and
    // replaced whenever the heap grows, so a view held across another call is a
    // window onto bytes that have moved.
    const out = new Uint8Array(ex.memory.buffer, outPtr, outLen).slice();
    ex.spore_free(outPtr, outLen);

    if (out[0] !== OK) throw new Error(`communicator rejected command 0x${cmd[0].toString(16)}`);
    return new RespReader(out.slice(1));
  }

  // ------------------------------------------------------------------ threads

  threadReceive({ from, body, sealed, at }) {
    return this.call(new CmdWriter(CMD.THREAD_RECEIVE).optAddrHex(from).str(body).bool(sealed).u32(at || 0))
      .optAddrHex();
  }

  threadSend({ to, id, body, sealed, at }) {
    this.call(new CmdWriter(CMD.THREAD_SEND).bytes(unhex(to)).bytes(unhex(id)).str(body).bool(sealed).u32(at || 0));
    return to;
  }

  threadSetStatus(idHex, status) {
    if (!idHex || !(status in STATUS)) return false;
    return this.call(new CmdWriter(CMD.THREAD_SET_STATUS).bytes(unhex(idHex)).u8(STATUS[status])).u8() === 1;
  }

  threadMarkRead(addrHex) {
    this.call(new CmdWriter(CMD.THREAD_MARK_READ).bytes(unhex(addrHex)));
  }

  threadMessages(addrHex) {
    const r = this.call(new CmdWriter(CMD.THREAD_MESSAGES).bytes(unhex(addrHex)));
    const out = [];
    const n = r.u32();
    for (let i = 0; i < n; i++) {
      const id = r.u8() === 1 ? r.hex(16) : null;
      const self = r.bool();
      const sealed = r.bool();
      const status = STATUS_NAME[r.u8()];
      const at = r.u32();
      out.push({ id, self, body: r.str(), at, sealed, status });
    }
    return out;
  }

  threadConversations() {
    const r = this.call(new CmdWriter(CMD.THREAD_CONVERSATIONS));
    const out = [];
    const n = r.u32();
    for (let i = 0; i < n; i++) {
      const addr = r.hex(8);
      const lastAt = r.u32();
      const lastSelf = r.bool();
      const unread = r.u32();
      out.push({ addr, lastBody: r.str(), lastAt, lastSelf, unread });
    }
    return out;
  }

  threadTotalUnread() {
    return this.call(new CmdWriter(CMD.THREAD_TOTAL_UNREAD)).u32();
  }

  threadUnauthenticatedCount() {
    return this.call(new CmdWriter(CMD.THREAD_UNAUTHENTICATED)).u32();
  }

  // -------------------------------------------------------------- persistence

  /** Everything, as one blob for the host to keep. */
  save() {
    const r = this.call(new CmdWriter(CMD.SAVE));
    return r.b.slice(r.i);
  }

  /**
   * Replace everything from a blob. Returns whether it was accepted.
   *
   * A rejected blob leaves every store exactly as it was — the Rust side decodes
   * all three before committing any — so a caller's correct response is to carry
   * on with what it has and leave the stored bytes alone.
   */
  load(blob) {
    if (!blob || !blob.length) return false;
    try {
      this.call(new CmdWriter(CMD.LOAD).bytes(blob));
      return true;
    } catch {
      return false;
    }
  }
}
