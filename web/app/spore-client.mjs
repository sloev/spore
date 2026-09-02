// SporeClient — the single seam between the UI and the SPORE kernel (M10).
//
// Everything above this file talks to SporeClient and nothing else. It is the
// entire infrastructure layer's public surface: the sole caller of `spore.mjs`,
// the sole owner of the node pointer, and the sole owner of a timer. Screens
// subscribe to `on()`; they never poll, never hold a node, never see a byte
// pointer.
//
// Why this shape: the communicator logic (threads, contacts, unread, delivery
// state) is written once per platform today — ~6100 lines of Kotlin, ~1300 of
// JS. M10 moves that into Rust so all three surfaces share it. This interface
// is the migration seam: the stores behind it start as JS and move into the
// kernel one at a time WITHOUT this file's callers changing. Keep the interface
// domain-shaped (addresses, envelopes, transfers) and free of wasm vocabulary,
// or that swap stops being invisible.
//
//   import { SporeClient } from './app/spore-client.mjs';
//   const client = new SporeClient({ storage: localStorageAdapter() });
//   const off = client.on((e) => { if (e.type === 'EnvelopeReceived') ... });
//   const identity = await client.init(fetch('./spore.wasm'));

import { loadSpore, Hub, FLAG_ENCRYPTED, FLAG_RATCHET, ZERO_DEST } from '../spore.mjs';
import { TRANSPORTS } from './transports.mjs';

/** Storage keys. Deliberately the same strings the pre-M10 node used, so an
 * existing standalone keeps its identity across the rewrite instead of silently
 * generating a new address and orphaning every prekey a peer holds. */
const K_SEED = 'spore.seed';
const K_RING = 'spore.ring';

const nowSec = () => Math.floor(Date.now() / 1000);
const hex = (u8) => Array.from(u8).map((b) => b.toString(16).padStart(2, '0')).join('');
const unhex = (s) => new Uint8Array((s.match(/.{1,2}/g) || []).map((b) => parseInt(b, 16)));

/**
 * §5.4b Trickle: announce at 5 minutes, double to a 80-minute ceiling, reset to
 * the floor on novelty. Beaconing is a scheduled duty of the runtime, not the
 * core — emitting on a transport is across the boundary the core is defined
 * against, which is why this lives here and not in Rust.
 */
const ANNOUNCE_MIN_MS = 5 * 60 * 1000;
const ANNOUNCE_MAX_MS = 80 * 60 * 1000;

/** How often the single owned loop runs. Ack and feed polling ride on this. */
const TICK_MS = 1000;

/**
 * @typedef {Object} Identity
 * @property {Uint8Array} addr    8-byte node address
 * @property {string}     addrHex 16-hex form — the domain key used in URLs
 * @property {boolean}    restored true if an existing seed was found on disk
 *
 * @typedef {Object} Envelope
 * @property {string}     id      32-hex envelope id, for reconciling acks
 * @property {string}     to      16-hex destination address
 * @property {Uint8Array} body
 * @property {number}     at      unix seconds
 * @property {boolean}    sealed  whether this actually went out encrypted
 * @property {'queued'|'sent'|'acked'|'expired'} status
 *
 * @typedef {Object} ClientEvent
 * @property {string} type
 */

/**
 * The events this client emits. Faults are events, not throws: a dropped bridge
 * or a malformed envelope must not crash a component tree over a transport
 * hiccup.
 */
export const EVENTS = /** @type {const} */ ([
  'Ready',              // { identity }
  'EnvelopeReceived',   // { from, body, sealed, at }
  'EnvelopeAcked',      // { id }
  'EnvelopeExpired',    // { id } — travelled past its TTL, never acked
  'FeedEvent',          // { from, topicHex, body, at }
  'BridgeStateChanged', // { id, kind, up, sent, received, lastFrameAt }
  'TransferProgress',   // { magnet, name, bytes, complete }
  'AnnounceSent',       // { at, nextInMs }
  'ClientFault',        // { scope, message }
]);

export class SporeClient {
  /**
   * @param {Object}  opts
   * @param {Object}  opts.storage  the storage port — { get, set, remove }, sync
   *   or async. M10-A replaces this with a Rust-side port; keep every call to it
   *   awaited so that swap is invisible here.
   */
  constructor({ storage } = {}) {
    if (!storage) throw new Error('SporeClient needs a storage port');
    this.storage = storage;
    this.spore = null;
    this.node = null;
    this.hub = null;
    this.identity = null;

    this._listeners = new Set();
    this._bridges = new Map(); // id -> { id, kind, transport, up, sent, received, lastFrameAt }
    this._pending = new Map(); // idHex -> { id, at, expiresAt }
    this._timer = null;
    this._announceEveryMs = ANNOUNCE_MIN_MS;
    this._announceAt = 0;
    this._nextBridgeId = 1;
    this._disposed = false;
  }

  // ---------------------------------------------------------------- lifecycle

  /**
   * Load the wasm, restore or create the identity, and start the single loop.
   * Every store and route guard waits on this one promise — there must be no
   * code path that touches the client before it resolves.
   *
   * @param {Response|ArrayBuffer|Uint8Array|Promise<any>} wasmSource
   * @returns {Promise<Identity>}
   */
  async init(wasmSource) {
    if (this.spore) return this.identity;
    this.spore = await loadSpore(wasmSource);

    const savedSeed = await this.storage.get(K_SEED);
    const restored = Boolean(savedSeed);
    this.node = this.spore.newNode(restored ? unhex(savedSeed) : null);

    if (!restored) await this.storage.set(K_SEED, hex(this.node.seed()));

    // The seed restores the identity but NOT the prekey secrets — those are
    // random, and that is exactly what makes deleting them mean something
    // (S-022). Restoring only the seed comes back unable to open mail sealed to
    // any prekey the node had rotated to, so the ring is persisted beside it.
    const savedRing = await this.storage.get(K_RING);
    if (savedRing) {
      const ok = this.node.restorePrekeyRing(unhex(savedRing));
      if (!ok) this._emit('ClientFault', { scope: 'prekey-ring', message: 'stored prekey ring was malformed; it was left unchanged' });
    }
    await this._saveRing();

    this.hub = new Hub(this.node);
    this.hub.onDeliver = (env) => this._onDeliver(env);

    const addr = this.node.addr();
    this.identity = { addr, addrHex: hex(addr), restored };

    this._timer = setInterval(() => this._tick(), TICK_MS);
    this._emit('Ready', { identity: this.identity });
    return this.identity;
  }

  /**
   * The 32-byte identity seed as 64 hex characters, for the one screen allowed
   * to show it (onboarding backup, and the settings row that repeats it).
   *
   * Secret material: this is the whole identity. Anyone who reads it *is* this
   * node. It is deliberately a method rather than a property so that reading it
   * is a visible act at every call site, and it is never included in any event,
   * log line or fault message.
   *
   * Note it does not carry the prekey ring — restoring from the seed alone comes
   * back unable to open mail sealed to a rotated prekey (S-022). That is the
   * documented trade-off of a seed backup, not a defect.
   */
  exportSeed() {
    this._assertReady();
    return hex(this.node.seed());
  }

  /** Stop the loop, close every bridge, free the node. Idempotent. */
  dispose() {
    if (this._disposed) return;
    this._disposed = true;
    if (this._timer) clearInterval(this._timer);
    this._timer = null;
    for (const id of [...this._bridges.keys()]) this.removeBridge(id);
    if (this.node) this.node.free();
    this.node = null;
    this._listeners.clear();
  }

  // ------------------------------------------------------------- subscription

  /**
   * The one subscription surface. Returns an unsubscribe function.
   * @param {(e: ClientEvent) => void} fn
   */
  on(fn) {
    this._listeners.add(fn);
    return () => this._listeners.delete(fn);
  }

  _emit(type, detail) {
    const event = { type, ...detail };
    for (const fn of this._listeners) {
      // A throwing listener is that listener's bug; it must not take down the
      // loop or the other subscribers with it.
      try { fn(event); } catch (err) {
        // eslint-disable-next-line no-console
        console.error('SporeClient listener threw', err);
      }
    }
  }

  // ------------------------------------------------------------------ sending

  /**
   * Send a sealed, signed DM. The returned Envelope is a real row with
   * `status: 'queued'` and its true envelope id — not a UI-only optimistic
   * placeholder merged at render time. A reconnect reconciles by id rather than
   * guessing which optimistic row matches which real one.
   *
   * @param {string} toHex 16-hex peer address
   * @param {Uint8Array} body
   * @returns {Envelope}
   */
  sendDirect(toHex, body) {
    this._assertReady();
    const dest = unhex(toHex);
    const sealed = this.canSealTo(toHex);
    const { forwards, id } = this.node.sendDirect(dest, body);
    this._dispatch(forwards);

    const at = nowSec();
    const idHex = hex(id);
    this._pending.set(idHex, { id, at, expiresAt: at + this.spore.defaultMessageExpirySecs() });
    return { id: idHex, to: toHex, body, at, sealed, status: 'queued' };
  }

  /**
   * Would a DM to this peer actually be sealed right now? Ask before promising
   * it — a node that has never heard the peer's ANNOUNCE has no key to seal to
   * and falls back to cleartext. Never draw a padlock unconditionally.
   */
  canSealTo(toHex) {
    this._assertReady();
    return this.node.canSealTo(unhex(toHex));
  }

  /** Publish to a feed topic (one-to-many, cleartext by design). */
  publish(topicName, body) {
    this._assertReady();
    const { forwards } = this.node.publish(topicName, body);
    this._dispatch(forwards);
    return { topic: topicName, body, at: nowSec() };
  }

  /** Follow a topic so `pollFeed` starts yielding its events. */
  subscribe(topicName) {
    this._assertReady();
    this.node.subscribe(topicName);
  }

  /** The 8-byte topic address for a name, as 16-hex — the domain key for feeds. */
  topicHexOf(topicName) {
    this._assertReady();
    return hex(this.spore.topicOf(topicName));
  }

  // -------------------------------------------------------------------- files

  /** Publish a file; returns its 32-hex magnet id. */
  publishFile(name, bytes, toHex = null) {
    this._assertReady();
    const dest = toHex ? unhex(toHex) : ZERO_DEST;
    return hex(this.node.publishFile(name, bytes, dest));
  }

  /** Ask the mesh for a file. Progress arrives as TransferProgress events. */
  fetchFile(magnetHex) {
    this._assertReady();
    const { forwards } = this.node.fetchFile(unhex(magnetHex));
    this._dispatch(forwards);
    return { magnet: magnetHex, complete: false };
  }

  /** Locally held files: [{ name, magnet }] with magnet as 32-hex. */
  listFiles() {
    this._assertReady();
    return this.node.listFiles().map((f) => ({ name: f.name, magnet: hex(f.magnet) }));
  }

  /** Bytes for a magnet, or null when the transfer has not completed. */
  fileBytes(magnetHex) {
    this._assertReady();
    return this.node.fileBytes(unhex(magnetHex));
  }

  // ------------------------------------------------------------------ bridges

  /**
   * Which transports can actually run here, answered by feature-detecting the
   * registry — never by user-agent sniffing. The bridge-add screen renders only
   * what this returns, which is how the eleven daemon-only bridges never appear
   * in the browser at all.
   *
   * @returns {Array<{kind: string, label: string, needsGesture: boolean}>}
   */
  availableTransports() {
    return TRANSPORTS.filter((t) => {
      try { return t.available(); } catch { return false; }
    }).map(({ kind, label, needsGesture }) => ({ kind, label, needsGesture }));
  }

  /**
   * Open a transport and attach it. `open()` may need a user gesture (serial and
   * bluetooth both prompt), so this must be called from an event handler for
   * those kinds.
   *
   * @param {{kind: string} & Record<string, any>} config
   */
  async addBridge(config) {
    this._assertReady();
    const spec = TRANSPORTS.find((t) => t.kind === config.kind);
    if (!spec) throw new Error('unknown transport kind: ' + config.kind);

    if (!spec.open) throw new Error(config.kind + ' cannot be opened from a config; build it and call attachTransport()');

    try {
      return this.attachTransport(config.kind, await spec.open(config));
    } catch (err) {
      this._emit('ClientFault', { scope: 'bridge:' + config.kind, message: String(err && err.message || err) });
      throw err;
    }
  }

  /**
   * Attach an already-constructed transport. `addBridge` is the path for kinds
   * that can be opened from a config; this is the path for the ones that cannot
   * — WebRTC needs an offer/answer exchanged out of band, so its handshake
   * screen builds the channel itself and hands the finished transport here.
   *
   * @param {string} kind
   * @param {{send: Function, receive: Function}} transport
   */
  attachTransport(kind, transport) {
    this._assertReady();
    const id = 'b' + this._nextBridgeId++;
    const record = { id, kind, transport, up: true, sent: 0, received: 0, lastFrameAt: 0 };
    this._bridges.set(id, record);

    // Count frames without the transports needing to know they are counted.
    const send = transport.send.bind(transport);
    transport.send = (bytes) => {
      record.sent++;
      record.lastFrameAt = Date.now();
      this._emitBridge(record);
      return send(bytes);
    };
    this.hub.addTransport(transport); // sets transport.receive to the hub's router
    const inbound = transport.receive;
    transport.receive = (bytes) => {
      record.received++;
      record.lastFrameAt = Date.now();
      this._emitBridge(record);
      return inbound(bytes);
    };

    this._emitBridge(record);
    // A new bridge is novelty: reset the trickle so peers hear us promptly.
    this._resetAnnounceBackoff();
    return { id, kind };
  }

  /** Detach and close a bridge. */
  removeBridge(id) {
    const record = this._bridges.get(id);
    if (!record) return;
    if (record.transport) {
      this.hub.removeTransport(record.transport);
      try { if (typeof record.transport.close === 'function') record.transport.close(); } catch { /* closing a dead transport is not an error */ }
    }
    this._bridges.delete(id);
    record.up = false;
    this._emitBridge(record);
  }

  /** Current bridge health, for a screen that mounts after the events fired. */
  bridges() {
    return [...this._bridges.values()].map(({ id, kind, up, sent, received, lastFrameAt }) =>
      ({ id, kind, up, sent, received, lastFrameAt }));
  }

  _emitBridge(r) {
    this._emit('BridgeStateChanged', {
      id: r.id, kind: r.kind, up: r.up, sent: r.sent, received: r.received, lastFrameAt: r.lastFrameAt,
    });
  }

  // --------------------------------------------------------------- the loop

  _tick() {
    if (this._disposed) return;
    try {
      this._pollFeed();
      this._pollAcks();
      this._maybeAnnounce();
    } catch (err) {
      this._emit('ClientFault', { scope: 'tick', message: String(err && err.message || err) });
    }
  }

  _pollFeed() {
    for (const ev of this.node.pollFeed()) {
      this._emit('FeedEvent', {
        from: ev.from ? hex(ev.from) : null,
        topicHex: hex(ev.topic),
        body: ev.data,
        at: nowSec(),
      });
    }
  }

  _pollAcks() {
    const at = nowSec();
    for (const [idHex, rec] of [...this._pending]) {
      if (this.node.acked(rec.id)) {
        this._pending.delete(idHex);
        this._emit('EnvelopeAcked', { id: idHex });
      } else if (at > rec.expiresAt) {
        // The core has no "gave up" event for an unacknowledged send, so the
        // runtime derives it: past its own TTL it stopped travelling. Saying
        // "expired, never delivered" is honest; leaving it spinning is not.
        this._pending.delete(idHex);
        this._emit('EnvelopeExpired', { id: idHex });
      }
    }
  }

  _maybeAnnounce() {
    const t = Date.now();
    if (t < this._announceAt) return;
    this._dispatch(this.node.announce());
    this._announceAt = t + this._announceEveryMs;
    this._emit('AnnounceSent', { at: nowSec(), nextInMs: this._announceEveryMs });
    this._announceEveryMs = Math.min(this._announceEveryMs * 2, ANNOUNCE_MAX_MS);
  }

  _resetAnnounceBackoff() {
    this._announceEveryMs = ANNOUNCE_MIN_MS;
    this._announceAt = 0; // fire on the next tick
  }

  // ------------------------------------------------------------------ inbound

  _onDeliver(env) {
    try {
      const from = this.spore.src(env);
      // `src` is null for unsigned envelopes, signatures that do not verify, and
      // SRC8 — which carries an address the envelope cannot prove. A thread list
      // keyed on anything weaker than this is spoofable.
      const flags = this.spore.flags(env);
      const encrypted = (flags & FLAG_ENCRYPTED) !== 0;
      let body = this.spore.payload(env);

      if (encrypted && from) {
        const opened = this.node.openDm(from, body, (flags & FLAG_RATCHET) !== 0);
        if (!opened) {
          // A real state, not a bug: a prekey may have expired past the offline
          // window. Surface it rather than dropping the message silently.
          this._emit('ClientFault', {
            scope: 'open-dm',
            message: "couldn't decrypt this message — the key may have expired",
          });
          return;
        }
        body = opened;
      }

      this._emit('EnvelopeReceived', {
        from: from ? hex(from) : null,
        body,
        sealed: encrypted,
        at: nowSec(),
      });
      // Hearing from someone is novelty under §5.4b.
      this._resetAnnounceBackoff();
      this._saveRing();
    } catch (err) {
      this._emit('ClientFault', { scope: 'deliver', message: String(err && err.message || err) });
    }
  }

  _dispatch(forwards) {
    for (const f of forwards) {
      for (const r of this._bridges.values()) {
        if (!r.transport) continue;
        try { r.transport.send(f); } catch (err) {
          r.up = false;
          this._emitBridge(r);
          this._emit('ClientFault', { scope: 'bridge:' + r.kind, message: String(err && err.message || err) });
        }
      }
    }
  }

  async _saveRing() {
    try { await this.storage.set(K_RING, hex(this.node.prekeyRing())); } catch (err) {
      this._emit('ClientFault', { scope: 'storage', message: 'could not persist the prekey ring: ' + String(err && err.message || err) });
    }
  }

  _assertReady() {
    if (!this.node) throw new Error('SporeClient.init() has not resolved yet');
  }
}

/**
 * The default storage port: browser localStorage. Async-shaped on purpose even
 * though localStorage is synchronous, so M10-A can drop in a Rust-backed port
 * without a single caller changing.
 */
export function localStorageAdapter(ls = globalThis.localStorage) {
  return {
    async get(k) { return ls.getItem(k); },
    async set(k, v) { ls.setItem(k, v); },
    async remove(k) { ls.removeItem(k); },
  };
}

/** An in-memory port, for tests and for a node that must not persist. */
export function memoryAdapter(seed = {}) {
  const m = new Map(Object.entries(seed));
  return {
    async get(k) { return m.has(k) ? m.get(k) : null; },
    async set(k, v) { m.set(k, v); },
    async remove(k) { m.delete(k); },
  };
}
