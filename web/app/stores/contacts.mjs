// ContactStore (M10-D) — local labels for addresses.
//
// The odd one of the six domain stores: it holds **no Spore data at all**. Every
// field here is something this user decided locally, and none of it is ever
// merged with anything authenticated.
//
// That separation is the whole point, and it mirrors the core's own reasoning
// (`Node::peer_name`): the name in a peer's ANNOUNCE is what they *claim* to be
// called, and anyone may announce any name. So a claimed name is a display hint
// offered as a default, never identity, and never written into this store as
// though the user had chosen it. `labelFor()` returns only what the user typed;
// joining that against a claim is the screen's job, and the screen has to say
// which is which.

export class ContactStore {
  constructor({ storage, key = 'spore.contacts' } = {}) {
    this.storage = storage || null;
    this.key = key;
    /** @type {Map<string, {addr: string, label: string|null, following: boolean, blocked: boolean}>} */
    this.byAddr = new Map();
  }

  // ------------------------------------------------------------- persistence

  async load() {
    if (!this.storage) return;
    const raw = await this.storage.get(this.key);
    if (!raw) return;
    try {
      for (const c of JSON.parse(raw) || []) {
        if (c && typeof c.addr === 'string') this.byAddr.set(c.addr, normalise(c));
      }
    } catch {
      // Corrupt blob: start empty this session, leave it on disk. Wiping a
      // user's own labels because a parse failed would be the worse failure.
    }
  }

  async save() {
    if (!this.storage) return;
    await this.storage.set(this.key, JSON.stringify([...this.byAddr.values()]));
  }

  // ------------------------------------------------------------------ writes

  /** Add or update the local label. An empty label removes it, keeping the row. */
  setLabel(addr, label) {
    const c = this._entry(addr);
    const trimmed = (label || '').trim();
    c.label = trimmed || null;
    return c;
  }

  setFollowing(addr, following) {
    const c = this._entry(addr);
    c.following = Boolean(following);
    return c;
  }

  setBlocked(addr, blocked) {
    const c = this._entry(addr);
    c.blocked = Boolean(blocked);
    return c;
  }

  remove(addr) {
    return this.byAddr.delete(addr);
  }

  _entry(addr) {
    let c = this.byAddr.get(addr);
    if (!c) {
      c = { addr, label: null, following: false, blocked: false };
      this.byAddr.set(addr, c);
    }
    return c;
  }

  // ------------------------------------------------------------------- reads

  get(addr) {
    return this.byAddr.get(addr) || null;
  }

  /**
   * The label this user gave the address, or null. Deliberately does NOT fall
   * back to an announced name: that would let a claim silently become "the
   * contact's name" everywhere, which is the exact confusion this store exists
   * to prevent.
   */
  labelFor(addr) {
    const c = this.byAddr.get(addr);
    return c && c.label ? c.label : null;
  }

  isBlocked(addr) {
    const c = this.byAddr.get(addr);
    return Boolean(c && c.blocked);
  }

  isFollowing(addr) {
    const c = this.byAddr.get(addr);
    return Boolean(c && c.following);
  }

  /** Every address the user has touched, labelled first then by address. */
  all() {
    return [...this.byAddr.values()].sort((a, b) => {
      if (Boolean(a.label) !== Boolean(b.label)) return a.label ? -1 : 1;
      return (a.label || a.addr).localeCompare(b.label || b.addr);
    });
  }

  /** Addresses the user follows — the Blogs screen's subscription list. */
  following() {
    return this.all().filter((c) => c.following);
  }
}

function normalise(c) {
  return {
    addr: c.addr,
    label: typeof c.label === 'string' && c.label.trim() ? c.label.trim() : null,
    following: Boolean(c.following),
    blocked: Boolean(c.blocked),
  };
}

/**
 * Join local labels against what peers claim, for a list the user can read.
 *
 * Returns rows carrying BOTH, plus `nameIsClaim` so the screen can mark an
 * unauthenticated name as such rather than presenting it as established. A row
 * with neither falls back to the address, which is the only thing here that was
 * ever proved.
 *
 * @param {Array} peers      from SporeClient.peers()
 * @param {ContactStore} contacts
 * @param {{ view?: 'contacts'|'seen', query?: string }} opts
 */
export function contactRows(peers, contacts, { view = 'contacts', query = '' } = {}) {
  const seen = new Map(peers.map((p) => [p.addrHex, p]));
  const addrs = new Set([...contacts.byAddr.keys(), ...seen.keys()]);

  let rows = [...addrs].map((addr) => {
    const c = contacts.get(addr);
    const p = seen.get(addr);
    const label = c && c.label ? c.label : null;
    return {
      addr,
      label,
      claimedName: p && p.claimedName ? p.claimedName : null,
      name: label || (p && p.claimedName) || null,
      nameIsClaim: !label && Boolean(p && p.claimedName),
      following: Boolean(c && c.following),
      blocked: Boolean(c && c.blocked),
      isContact: Boolean(c),
      heard: Boolean(p),
      ageSecs: p ? p.ageSecs : null,
      hasPrekey: Boolean(p && p.hasPrekey),
    };
  });

  // "Contacts" is what the user has deliberately kept; "seen" is everyone the
  // node has heard from who is not yet one, which is a genuinely different list
  // rather than a filter of the same one.
  rows = rows.filter((r) => (view === 'contacts' ? r.isContact : r.heard && !r.isContact));

  const q = query.trim().toLowerCase();
  if (q) {
    rows = rows.filter((r) =>
      r.addr.includes(q)
      || (r.label && r.label.toLowerCase().includes(q))
      || (r.claimedName && r.claimedName.toLowerCase().includes(q)));
  }

  rows.sort((a, b) => {
    if (view === 'seen') return (a.ageSecs ?? 0) - (b.ageSecs ?? 0); // freshest first
    if (Boolean(a.label) !== Boolean(b.label)) return a.label ? -1 : 1;
    return (a.name || a.addr).localeCompare(b.name || b.addr);
  });
  return rows;
}
