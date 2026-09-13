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
  CONTACT_GET: 0x15,
  TOPIC_REMEMBER: 0x20,
  TOPIC_FORGET: 0x21,
  TOPIC_RECEIVE: 0x22,
  TOPIC_POSTS: 0x23,
  TOPIC_NAMED: 0x24,
  DRAFT_SET: 0x40,
  DRAFT_GET: 0x41,
  DRAFT_CLEAR: 0x42,
  DRAFT_ALL: 0x43,
  SAVE: 0x30,
  LOAD: 0x31,
};

/** Draft scopes, matching `Scope::code` in Rust. */
export const SCOPE = { chat: 0, topic: 1 };

/** Message status codes, matching `MessageStatus::code` in Rust. */
export const STATUS = { queued: 0, sent: 1, acked: 2, expired: 3, received: 4 };
const STATUS_NAME = ['queued', 'sent', 'acked', 'expired', 'received'];

// Named `commHex`/`commUnhex` rather than `hex`/`unhex`, which `spore-client.mjs`
// already owns. The standalone build flattens every module into one classic
// script, so two top-level `hex` declarations are a duplicate-identifier error at
// build time — module scoping hides the clash right up until the single-file node
// is built, which is the artefact most people actually run.
export function commHex(bytes) {
  return [...bytes].map((b) => b.toString(16).padStart(2, '0')).join('');
}

export function commUnhex(s) {
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
  optAddrHex(h) { return h ? this.u8(1).bytes(commUnhex(h)) : this.u8(0); }
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
  hex(n) { return commHex(this.bytes(n)); }
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
   */
  constructor(ex) {
    this.ex = ex;
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
  call(w) {
    const cmd = w.out();
    const { ex } = this;
    const ptr = ex.spore_alloc(cmd.length);
    new Uint8Array(ex.memory.buffer, ptr, cmd.length).set(cmd);
    const packed = ex.spore_comm_call(this.ptr, ptr, cmd.length);
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
    this.call(new CmdWriter(CMD.THREAD_SEND).bytes(commUnhex(to)).bytes(commUnhex(id)).str(body).bool(sealed).u32(at || 0));
    return to;
  }

  threadSetStatus(idHex, status) {
    if (!idHex || !(status in STATUS)) return false;
    return this.call(new CmdWriter(CMD.THREAD_SET_STATUS).bytes(commUnhex(idHex)).u8(STATUS[status])).u8() === 1;
  }

  threadMarkRead(addrHex) {
    this.call(new CmdWriter(CMD.THREAD_MARK_READ).bytes(commUnhex(addrHex)));
  }

  threadMessages(addrHex) {
    const r = this.call(new CmdWriter(CMD.THREAD_MESSAGES).bytes(commUnhex(addrHex)));
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

  // ----------------------------------------------------------------- contacts

  contactSetLabel(addrHex, label) {
    this.call(new CmdWriter(CMD.CONTACT_SET_LABEL).bytes(commUnhex(addrHex)).str(label || ''));
  }

  contactSetFollowing(addrHex, v) {
    this.call(new CmdWriter(CMD.CONTACT_SET_FOLLOWING).bytes(commUnhex(addrHex)).bool(v));
  }

  contactSetBlocked(addrHex, v) {
    this.call(new CmdWriter(CMD.CONTACT_SET_BLOCKED).bytes(commUnhex(addrHex)).bool(v));
  }

  contactRemove(addrHex) {
    return this.call(new CmdWriter(CMD.CONTACT_REMOVE).bytes(commUnhex(addrHex))).u8() === 1;
  }

  /** One contact's local state, or null when the user has never touched it. */
  contactGet(addrHex) {
    const r = this.call(new CmdWriter(CMD.CONTACT_GET).bytes(commUnhex(addrHex)));
    if (!r.bool()) return null;
    const following = r.bool();
    const blocked = r.bool();
    const label = r.str();
    return { addr: addrHex, label: label || null, following, blocked };
  }

  /**
   * Rows for the contacts or seen list.
   *
   * `peers` is what the caller has heard from — `client.peers()` in the browser.
   * It travels with the command rather than being cached here, because a cached
   * peer table is stale the moment anything arrives, and because not every host
   * keeps its peers where this layer could reach them.
   */
  contactRows(peers = [], { view = 'contacts', query = '' } = {}) {
    const w = new CmdWriter(CMD.CONTACT_ROWS).u8(view === 'seen' ? 1 : 0).str(query).u32(peers.length);
    for (const p of peers) {
      w.bytes(commUnhex(p.addrHex || p.addr)).u32(p.ageSecs || 0).bool(p.hasPrekey).str(p.claimedName || '');
    }
    const r = this.call(w);
    const out = [];
    const n = r.u32();
    for (let i = 0; i < n; i++) {
      const addr = r.hex(8);
      const nameIsClaim = r.bool();
      const following = r.bool();
      const blocked = r.bool();
      const isContact = r.bool();
      const heard = r.bool();
      const hasPrekey = r.bool();
      const age = r.u32();
      const hasAge = r.bool();
      const label = r.str();
      const claimedName = r.str();
      const name = r.str();
      out.push({
        addr,
        label: label || null,
        claimedName: claimedName || null,
        name: name || null,
        nameIsClaim,
        following,
        blocked,
        isContact,
        heard,
        ageSecs: hasAge ? age : null,
        hasPrekey,
      });
    }
    return out;
  }

  // ------------------------------------------------------------------- topics

  topicRemember(topicHex, name) {
    this.call(new CmdWriter(CMD.TOPIC_REMEMBER).bytes(commUnhex(topicHex)).str(name));
  }

  topicForget(topicHex) {
    this.call(new CmdWriter(CMD.TOPIC_FORGET).bytes(commUnhex(topicHex)));
  }

  topicReceive({ topicHex, from, body, at }) {
    this.call(new CmdWriter(CMD.TOPIC_RECEIVE).bytes(commUnhex(topicHex)).optAddrHex(from).str(body).u32(at || 0));
  }

  topicPosts(topicHex) {
    const r = this.call(new CmdWriter(CMD.TOPIC_POSTS).bytes(commUnhex(topicHex)));
    const out = [];
    const n = r.u32();
    for (let i = 0; i < n; i++) {
      const from = r.optAddrHex();
      const at = r.u32();
      out.push({ from, body: r.str(), at });
    }
    return out;
  }

  /** `topicHex -> name`, for every topic the user has named. */
  topicNames() {
    const r = this.call(new CmdWriter(CMD.TOPIC_NAMED));
    const out = new Map();
    const n = r.u32();
    for (let i = 0; i < n; i++) {
      const topic = r.hex(8);
      out.set(topic, r.str());
    }
    return out;
  }

  // ------------------------------------------------------------------- drafts

  /**
   * Save what the user has typed. Empty text clears it, so an emptied composer
   * leaves nothing to restore.
   *
   * `scope` keeps a conversation draft and a feed draft apart even when their
   * addresses collide — both key spaces are truncated SHA-256.
   */
  draftSet(scope, addrHex, text, at) {
    this.call(new CmdWriter(CMD.DRAFT_SET).u8(SCOPE[scope]).bytes(commUnhex(addrHex)).u32(at || 0).str(text || ''));
  }

  /** What the user had typed here, or '' if nothing. */
  draftGet(scope, addrHex) {
    return this.call(new CmdWriter(CMD.DRAFT_GET).u8(SCOPE[scope]).bytes(commUnhex(addrHex))).str();
  }

  draftClear(scope, addrHex) {
    return this.call(new CmdWriter(CMD.DRAFT_CLEAR).u8(SCOPE[scope]).bytes(commUnhex(addrHex))).u8() === 1;
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
