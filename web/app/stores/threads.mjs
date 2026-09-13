// ThreadStore — direct messages, keyed by peer address.
//
// **This is now a shim.** The store itself is `src/communicator/thread.rs`; what
// remains here is address translation and the host's storage, which is all a
// browser can contribute. M10's sequencing was contract-first for exactly this
// moment: the screens were built against `SporeClient`, this store's interface
// has not changed, and the ~190 lines of duplicated conversation logic that used
// to live here are gone rather than ported.
//
// The two rules it enforced are unchanged, because they moved with it:
//
//   * A thread is keyed on the AUTHENTICATED sender only. `from: null` — an
//     unsigned envelope, a bad signature, or SRC8 — is counted and never filed.
//   * An optimistic send is a real row with the true envelope id, and acks
//     reconcile by id rather than by position.
//
// Both now hold for every host that speaks the ABI, not just this one, which is
// the whole point of moving them.
//
// `groupThread` stays here. It is a pure function over messages the screen has
// already been handed, it needs a locale-aware day label the host computes, and
// running it in Rust would mean crossing the boundary twice to format a date.

/** Posts retained per thread, mirrored from `thread::MAX_PER_THREAD`. */
export const MAX_PER_THREAD = 500;

export class ThreadStore {
  /**
   * @param {object} opts
   * @param {object} opts.storage  the host's key/value store, or null
   * @param {() => import('../communicator.mjs').Communicator} opts.comm
   *   A thunk, not a value: the communicator does not exist until the wasm
   *   module has loaded, and this store is constructed before `boot` gets that
   *   far. Resolving it lazily keeps the construction order the app already has.
   */
  constructor({ storage, comm, key = 'spore.threads' } = {}) {
    this.storage = storage || null;
    this.key = key;
    this._comm = comm || (() => null);
  }

  get comm() {
    const c = this._comm();
    if (!c) throw new Error('ThreadStore used before the communicator existed');
    return c;
  }

  // ------------------------------------------------------------- persistence

  async load() {
    if (!this.storage) return;
    const raw = await this.storage.get(this.key);
    if (!raw) return;
    // A corrupt blob is left on disk and the session starts empty, exactly as
    // before. Wiping a user's history because one parse failed would be the
    // worse failure, and a later build may know how to read what this one
    // cannot. The Rust side is all-or-nothing, so a refused blob leaves every
    // store untouched rather than half-loaded.
    this.comm.load(fromBase64(raw));
  }

  async save() {
    if (!this.storage) return;
    await this.storage.set(this.key, toBase64(this.comm.save()));
  }

  // ------------------------------------------------------------------ writes

  /** A message arrived. Returns the thread it was filed under, or null. */
  receive({ from, body, sealed, at }) {
    return this.comm.threadReceive({ from, body, sealed, at });
  }

  /** Record a locally originated send. `envelope` is what sendDirect returned. */
  send(envelope) {
    return this.comm.threadSend({
      to: envelope.to,
      id: envelope.id,
      body: envelope.body,
      sealed: Boolean(envelope.sealed),
      at: envelope.at,
    });
  }

  /** Reconcile by envelope id — never by position or by guessing. */
  setStatus(id, status) {
    return this.comm.threadSetStatus(id, status);
  }

  markRead(addr) {
    this.comm.threadMarkRead(addr);
  }

  // ------------------------------------------------------------------- reads

  messages(addr) {
    return this.comm.threadMessages(addr);
  }

  unreadFor(addr) {
    const row = this.conversations().find((c) => c.addr === addr);
    return row ? row.unread : 0;
  }

  /**
   * Conversation rows, most recently active first. `name` is left to the caller
   * to resolve from contacts — this store never invents a display name, because
   * the only name it could invent is one the envelope claimed rather than proved.
   */
  conversations() {
    return this.comm.threadConversations();
  }

  totalUnread() {
    return this.comm.threadTotalUnread();
  }

  /** Envelopes that arrived without a provable sender. Surfaced, not filed. */
  get unauthenticatedCount() {
    return this.comm.threadUnauthenticatedCount();
  }
}

// The blob is bytes and the storage port is strings, so it travels base64. Not
// `JSON.stringify` of an array: that is ~6 characters per byte against base64's
// 1.37, and localStorage is a few megabytes for everything the app owns.
function toBase64(bytes) {
  let s = '';
  for (const b of bytes) s += String.fromCharCode(b);
  return btoa(s);
}

function fromBase64(s) {
  try {
    const bin = atob(s);
    const out = new Uint8Array(bin.length);
    for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
    return out;
  } catch {
    return new Uint8Array(0);
  }
}

/**
 * Group a thread into day-separated runs, the way the design's thread renders:
 * a sticky day divider, and consecutive messages from one author collapsed into
 * a run so only the first carries an avatar.
 *
 * Returns a flat list of {kind} items so the renderer stays a simple map.
 */
export function groupThread(messages, dayLabelOf) {
  const out = [];
  let lastDay = null;
  let lastAuthor = null;

  messages.forEach((m, i) => {
    const day = dayLabelOf(m.at);
    if (day !== lastDay) {
      out.push({ kind: 'day', label: day });
      lastDay = day;
      lastAuthor = null;
    }
    const author = m.self ? '@self' : '@peer';
    const next = messages[i + 1];
    const sameAsPrev = author === lastAuthor;
    const sameAsNext = next && (next.self ? '@self' : '@peer') === author && dayLabelOf(next.at) === day;

    let run = 'only';
    if (sameAsPrev && sameAsNext) run = 'mid';
    else if (sameAsPrev && !sameAsNext) run = 'last';
    else if (!sameAsPrev && sameAsNext) run = 'first';

    out.push({ kind: 'message', message: m, run });
    lastAuthor = author;
  });

  return out;
}
