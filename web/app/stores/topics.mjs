// TopicStore — followed feeds, their names, and the posts that arrive.
//
// **This is now a shim.** The store is `src/communicator/topic.rs`.
//
// It exists because of one fact about the wire: a topic address is
// `topic_of(name)`, a hash, so **a name cannot be recovered from an address**. A
// post arrives carrying eight bytes of topic and nothing else, and the only way
// to show "ridge-weather" rather than `4f2a…` is to have written the name down
// when the user typed it.
//
// Membership and naming stay in different places, and that split is
// load-bearing: the kernel owns which topics are followed — it is what goes out
// in ANNOUNCE — and this store owns what they are called here. Asking each for
// its own half means a drifted local list shows up as a topic with no name,
// which is visibly odd and true, rather than as a UI confidently claiming a
// subscription the node does not have.

export class TopicStore {
  constructor({ storage, comm, key = 'spore.topics' } = {}) {
    this.storage = storage || null;
    this.key = key;
    this._comm = comm || (() => null);
  }

  get comm() {
    const c = this._comm();
    if (!c) throw new Error('TopicStore used before the communicator existed');
    return c;
  }

  // ------------------------------------------------------------- persistence

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

  // ------------------------------------------------------------------ naming

  /** Remember what a topic is called here. */
  remember(topicHex, name) {
    if (topicHex && name) this.comm.topicRemember(topicHex, name);
  }

  /** Forget a topic entirely — its name and everything received on it. */
  forget(topicHex) {
    this.comm.topicForget(topicHex);
  }

  /**
   * The name this user gave a topic, or `null`.
   *
   * Deliberately does not invent one. A topic followed on another device, or one
   * whose name was lost, has no name here, and a screen showing the bare address
   * is telling the truth about that.
   */
  nameFor(topicHex) {
    return this.comm.topicNames().get(topicHex) || null;
  }

  /** `topicHex -> name`, for every topic the user has named. */
  get names() {
    return this.comm.topicNames();
  }

  // ------------------------------------------------------------------- posts

  /**
   * File an arriving post.
   *
   * `from` may be null: a feed post is flooded and need not be signed, so the
   * sender is recorded only when the core authenticated one. It is never
   * inferred, for the same reason ThreadStore refuses to file an unauthenticated
   * message under a claimed sender.
   */
  receive({ topicHex, from, body, at }) {
    if (!topicHex) return false;
    this.comm.topicReceive({ topicHex, from: from || null, body, at });
    return true;
  }

  /** Posts on a topic, oldest first. */
  postsOn(topicHex) {
    return this.comm.topicPosts(topicHex);
  }

  /** The most recent post on a topic, or null. */
  latestOn(topicHex) {
    const list = this.postsOn(topicHex);
    return list.length ? list[list.length - 1] : null;
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
