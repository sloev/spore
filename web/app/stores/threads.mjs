// ThreadStore (M10-D) — direct messages, keyed by peer address.
//
// This is one of the six domain stores. It is written in JS *deliberately and
// temporarily*: the M10 sequencing is contract-first, so screens are built
// against SporeClient now and each store moves into Rust (M10-B) afterwards
// without its callers changing. Everything here is therefore kept boring and
// free of DOM or client references — it takes plain events in and answers plain
// questions, which is exactly the shape a Rust port needs.
//
// Two rules it exists to enforce:
//
//   * A thread is keyed on the AUTHENTICATED sender only. `SporeClient` emits
//     `from: null` for an unsigned envelope, a bad signature, or SRC8 — an
//     address the envelope cannot prove. Those are counted, never filed into a
//     conversation, because a thread list keyed on anything weaker is spoofable.
//   * An optimistic send is a real row with the true envelope id, not a
//     separate pending list merged at render. Acks reconcile by id.

const MAX_PER_THREAD = 500; // a tab is not an archive; oldest fall off first

/** @typedef {'queued'|'sent'|'acked'|'expired'|'received'} MessageStatus */

export class ThreadStore {
  constructor({ storage, key = 'spore.threads' } = {}) {
    this.storage = storage || null;
    this.key = key;
    /** @type {Map<string, Array>} addrHex -> messages, oldest first */
    this.threads = new Map();
    /** @type {Map<string, number>} addrHex -> unread count */
    this.unread = new Map();
    /** Envelopes that arrived without a provable sender. Surfaced, not filed. */
    this.unauthenticatedCount = 0;
  }

  // ------------------------------------------------------------- persistence

  async load() {
    if (!this.storage) return;
    const raw = await this.storage.get(this.key);
    if (!raw) return;
    try {
      const data = JSON.parse(raw);
      for (const [addr, msgs] of Object.entries(data.threads || {})) this.threads.set(addr, msgs);
      for (const [addr, n] of Object.entries(data.unread || {})) this.unread.set(addr, n);
    } catch {
      // A corrupt blob is not worth crashing a node over, and silently wiping
      // it would be worse. Leave it on disk and start empty this session.
    }
  }

  async save() {
    if (!this.storage) return;
    const data = {
      threads: Object.fromEntries(this.threads),
      unread: Object.fromEntries(this.unread),
    };
    await this.storage.set(this.key, JSON.stringify(data));
  }

  // ------------------------------------------------------------------ writes

  /**
   * A message arrived. Returns the conversation key it was filed under, or null
   * when the sender could not be authenticated.
   */
  receive({ from, body, sealed, at }) {
    if (!from) {
      this.unauthenticatedCount++;
      return null;
    }
    this._append(from, {
      id: null,
      self: false,
      body,
      at,
      sealed: Boolean(sealed),
      status: 'received',
    });
    this.unread.set(from, (this.unread.get(from) || 0) + 1);
    return from;
  }

  /** Record a locally originated send. `envelope` is what sendDirect returned. */
  send(envelope) {
    this._append(envelope.to, {
      id: envelope.id,
      self: true,
      body: envelope.body,
      at: envelope.at,
      sealed: Boolean(envelope.sealed),
      status: 'queued',
    });
    return envelope.to;
  }

  /** Reconcile by envelope id — never by position or by guessing. */
  setStatus(id, status) {
    for (const msgs of this.threads.values()) {
      for (const m of msgs) {
        if (m.id && m.id === id) { m.status = status; return true; }
      }
    }
    return false;
  }

  markRead(addr) {
    this.unread.set(addr, 0);
  }

  _append(addr, msg) {
    const list = this.threads.get(addr) || [];
    list.push(msg);
    if (list.length > MAX_PER_THREAD) list.splice(0, list.length - MAX_PER_THREAD);
    this.threads.set(addr, list);
  }

  // ------------------------------------------------------------------- reads

  messages(addr) {
    return this.threads.get(addr) || [];
  }

  unreadFor(addr) {
    return this.unread.get(addr) || 0;
  }

  /**
   * Conversation rows, most recently active first. `name` is left to the caller
   * to resolve from contacts — this store never invents a display name, because
   * the only name it could invent is one the envelope claimed rather than proved.
   */
  conversations() {
    const rows = [];
    for (const [addr, msgs] of this.threads) {
      const last = msgs[msgs.length - 1];
      rows.push({
        addr,
        lastBody: last ? last.body : '',
        lastAt: last ? last.at : 0,
        lastSelf: last ? last.self : false,
        unread: this.unreadFor(addr),
      });
    }
    rows.sort((a, b) => b.lastAt - a.lastAt);
    return rows;
  }

  totalUnread() {
    let n = 0;
    for (const v of this.unread.values()) n += v;
    return n;
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
