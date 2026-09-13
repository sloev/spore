// ContactStore — local labels for addresses.
//
// **This is now a shim.** The store is `src/communicator/contact.rs`; what
// remains here is the host's storage and the shape `main.mjs` already calls.
//
// The rule it existed to enforce moved with it, and is worth restating because
// it is the whole reason this store is separate from anything the mesh says:
// **it holds no SPORE data at all.** Every field is something this user decided
// locally, and `labelFor` returns only what they typed. It deliberately does not
// fall back to an announced name — anyone may announce any name, and a fallback
// there is how a claim silently becomes "the contact's name" everywhere.
//
// `contactRows` joins the two and marks which is which. Its signature is
// unchanged — it still takes the caller's peer list — because peers travel with
// the command rather than being cached in the communicator: a cached table is
// stale the moment anything arrives, and Android keeps its peers somewhere this
// layer could not reach anyway.

export class ContactStore {
  /**
   * @param {object} opts
   * @param {object} opts.storage  the host's key/value store, or null
   * @param {() => import('../communicator.mjs').Communicator} opts.comm
   *   A thunk: the communicator does not exist until wasm has loaded, and this
   *   store is constructed before `boot` gets that far.
   */
  constructor({ storage, comm, key = 'spore.contacts' } = {}) {
    this.storage = storage || null;
    this.key = key;
    this._comm = comm || (() => null);
  }

  get comm() {
    const c = this._comm();
    if (!c) throw new Error('ContactStore used before the communicator existed');
    return c;
  }

  // ------------------------------------------------------------- persistence
  //
  // One blob holds all three stores, so whichever of them loads first restores
  // every one. Saving from any of them is likewise a full save. That is a
  // consequence of the Rust side being all-or-nothing on load, which is the
  // property worth having: a contact book from this session beside a thread list
  // from the last is a state nothing downstream can detect.

  async load() {
    if (!this.storage) return;
    const raw = await this.storage.get(this.key);
    if (!raw) return;
    this.comm.load(fromBase64(raw));
  }

  async save() {
    if (!this.storage) return;
    await this.storage.set(this.key, toBase64(this.comm.save()));
  }

  // ------------------------------------------------------------------ writes

  /** Add or update the local label. An empty label removes it, keeping the row. */
  setLabel(addr, label) {
    this.comm.contactSetLabel(addr, label);
    return this.get(addr);
  }

  setFollowing(addr, following) {
    this.comm.contactSetFollowing(addr, Boolean(following));
    return this.get(addr);
  }

  setBlocked(addr, blocked) {
    this.comm.contactSetBlocked(addr, Boolean(blocked));
    return this.get(addr);
  }

  remove(addr) {
    return this.comm.contactRemove(addr);
  }

  // ------------------------------------------------------------------- reads

  get(addr) {
    return this.comm.contactGet(addr);
  }

  /**
   * The label this user gave the address, or null. Deliberately does NOT fall
   * back to an announced name.
   */
  labelFor(addr) {
    const c = this.get(addr);
    return c && c.label ? c.label : null;
  }

  isBlocked(addr) {
    const c = this.get(addr);
    return Boolean(c && c.blocked);
  }

  isFollowing(addr) {
    const c = this.get(addr);
    return Boolean(c && c.following);
  }

  /**
   * Every address the user has touched, labelled first then by sort key.
   *
   * `peers` is optional here because this list is about what the *user* kept,
   * not about what has been heard from; passing it in fills the claimed names
   * and ages, and leaving it out simply means those fields are null.
   */
  all(peers = []) {
    return this.comm.contactRows(peers, { view: 'contacts', query: '' });
  }

  /** Addresses the user follows — the Blogs screen's subscription list. */
  following(peers = []) {
    return this.all(peers).filter((c) => c.following);
  }
}

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
 * Join local labels against what peers claim, for a list the user can read.
 *
 * Rows carry BOTH, plus `nameIsClaim` so the screen can mark an unauthenticated
 * name as such rather than presenting it as established. A row with neither
 * falls back to the address, which is the only thing here that was ever proved.
 *
 * @param {ContactStore} contacts
 * @param {{ view?: 'contacts'|'seen', query?: string }} opts
 * @param {number} now  seconds, for the peer ages the kernel computes
 */
export function contactRows(peers, contacts, { view = 'contacts', query = '' } = {}) {
  return contacts.comm.contactRows(peers, { view, query });
}
