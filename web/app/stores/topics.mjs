// TopicStore (M10-D) — followed feeds, their names, and the posts that arrive.
//
// The fourth of the six domain stores. It exists because of one fact about the
// wire: a topic address is `topic_of(name)`, a hash, so **a name cannot be
// recovered from an address**. A FeedEvent arrives carrying a 16-hex topic and
// nothing else, and the only way to show "ridge-weather" instead of
// `4f2a…` is to have written the name down when the user typed it.
//
// So membership and naming live in different places on purpose, and the split is
// load-bearing rather than tidy:
//
//   the kernel  owns which topics are followed — it is what goes out in ANNOUNCE
//   this store  owns what they are called here, and what has been received
//
// Asking the kernel for membership (`client.subscriptions()`) and this store for
// names means a drifted local list shows up as a topic with no name, which is
// visibly odd, rather than as a UI confidently claiming a subscription the node
// does not have.
//
// Posts are kept per topic and capped. A feed is public, floods, and anyone may
// publish to it, so unbounded retention is a remote party choosing how much
// memory this tab uses.

/** Posts retained per topic. A feed is public and anyone may publish to it. */
const MAX_POSTS = 200;

export class TopicStore {
  constructor({ storage, key = 'spore.topics' } = {}) {
    this.storage = storage || null;
    this.key = key;
    /** @type {Map<string, string>} topicHex -> the name the user typed */
    this.names = new Map();
    /** @type {Map<string, Array<{from: string|null, body: string, at: number}>>} */
    this.posts = new Map();
  }

  // ------------------------------------------------------------- persistence

  async load() {
    if (!this.storage) return;
    const raw = await this.storage.get(this.key);
    if (!raw) return;
    try {
      const blob = JSON.parse(raw) || {};
      for (const [hex, name] of Object.entries(blob.names || {})) {
        if (typeof name === 'string') this.names.set(hex, name);
      }
      for (const [hex, list] of Object.entries(blob.posts || {})) {
        if (Array.isArray(list)) this.posts.set(hex, list.slice(-MAX_POSTS));
      }
    } catch {
      // Corrupt blob: start empty this session and leave it on disk, exactly as
      // ContactStore does. Losing a session of posts beats destroying the names
      // the user typed because one parse failed.
    }
  }

  async save() {
    if (!this.storage) return;
    await this.storage.set(this.key, JSON.stringify({
      names: Object.fromEntries(this.names),
      posts: Object.fromEntries(this.posts),
    }));
  }

  // ------------------------------------------------------------------ naming

  /** Remember what a topic is called here. */
  remember(topicHex, name) {
    if (topicHex && name) this.names.set(topicHex, name);
  }

  /** Forget a topic entirely — its name and everything received on it. */
  forget(topicHex) {
    this.names.delete(topicHex);
    this.posts.delete(topicHex);
  }

  /**
   * The name this user gave a topic, or `null`.
   *
   * Deliberately does not invent one. A topic followed on another device, or
   * one whose name was lost, has no name here, and a screen showing the bare
   * address is telling the truth about that.
   */
  nameFor(topicHex) {
    return this.names.get(topicHex) || null;
  }

  // ------------------------------------------------------------------- posts

  /**
   * File an arriving FeedEvent. Returns true if it was kept.
   *
   * `from` may be null: a feed post is flooded and need not be signed, so the
   * sender is only recorded when the core authenticated one. It is never
   * inferred, for the same reason ThreadStore refuses to file an unauthenticated
   * message under a claimed sender.
   */
  receive({ topicHex, from, body, at }) {
    if (!topicHex) return false;
    const list = this.posts.get(topicHex) || [];
    list.push({ from: from || null, body, at: at || Math.floor(Date.now() / 1000) });
    if (list.length > MAX_POSTS) list.splice(0, list.length - MAX_POSTS);
    this.posts.set(topicHex, list);
    return true;
  }

  /** Posts on a topic, oldest first. */
  postsOn(topicHex) {
    return this.posts.get(topicHex) || [];
  }

  /** The most recent post on a topic, or null. */
  latestOn(topicHex) {
    const list = this.posts.get(topicHex);
    return list && list.length ? list[list.length - 1] : null;
  }
}
