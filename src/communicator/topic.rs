//! **Followed feeds, their names, and the posts that arrive** (M10-B).
//!
//! The Rust port of `web/app/stores/topics.mjs`. It exists because of one fact
//! about the wire: a topic address is [`crate::topic_of`]`(name)`, a hash, so
//! **a name cannot be recovered from an address**. A feed post arrives carrying
//! eight bytes of topic and nothing else, and the only way to show
//! "ridge-weather" rather than `4f2a…` is to have written the name down when the
//! user typed it.
//!
//! # Membership and naming live in different places, on purpose
//!
//! ```text
//! the kernel  owns which topics are followed — it is what goes out in ANNOUNCE
//! this store  owns what they are called here, and what has been received
//! ```
//!
//! The split is load-bearing rather than tidy. Asking the kernel for membership
//! and this store for names means a drifted local list shows up as *a topic with
//! no name* — visibly odd, and true — rather than as a UI confidently claiming a
//! subscription the node does not actually have. A store that owned both would
//! have no way to show the difference, because it would be the only witness.
//!
//! # Posts are capped
//!
//! A feed is public, it floods, and anyone may publish to it. Unbounded
//! retention is therefore a remote party choosing how much memory this tab uses,
//! which is the resource invariant stated one layer up from the kernel: no
//! remote node can cause another to store an unbounded amount without an
//! explicit bounded local allowance. [`MAX_POSTS`] is that allowance.

use crate::Addr;

/// Posts retained per topic. A feed is public and anyone may publish to it.
pub const MAX_POSTS: usize = 200;

/// One arriving post.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Post {
    /// The authenticated sender, when there was one.
    ///
    /// `None` is ordinary here in a way it is not for a direct message: a feed
    /// post floods and need not be signed at all. It is recorded only when the
    /// core authenticated one, and **never inferred** — the same rule
    /// [`super::ThreadStore::receive`] applies, for the same reason.
    pub from: Option<Addr>,
    pub body: String,
    /// When, as the host reported it. Required — see [`TopicStore::receive`].
    pub at: u32,
}

/// Topic names the user typed, and the posts received on each.
#[derive(Default)]
pub struct TopicStore {
    /// `topic -> the name the user typed`. `Vec` rather than a map so the
    /// encoding is deterministic, as in the other stores.
    names: Vec<(Addr, String)>,
    posts: Vec<(Addr, Vec<Post>)>,
}

impl TopicStore {
    pub fn new() -> TopicStore {
        TopicStore::default()
    }

    // ---------------------------------------------------------------- naming

    /// Remember what a topic is called here.
    ///
    /// An empty name is ignored rather than stored: a topic named `""` renders
    /// as an unnamed topic while claiming to be a named one, which is the single
    /// state this store exists to keep distinguishable.
    pub fn remember(&mut self, topic: Addr, name: &str) {
        let name = name.trim();
        if name.is_empty() {
            return;
        }
        match self.names.iter_mut().find(|(t, _)| *t == topic) {
            Some((_, n)) => *n = name.to_string(),
            None => self.names.push((topic, name.to_string())),
        }
    }

    /// Forget a topic entirely — its name and everything received on it.
    pub fn forget(&mut self, topic: &Addr) {
        self.names.retain(|(t, _)| t != topic);
        self.posts.retain(|(t, _)| t != topic);
    }

    /// The name this user gave a topic, or `None`.
    ///
    /// Deliberately does not invent one. A topic followed on another device, or
    /// one whose name was lost, has no name here, and a screen showing the bare
    /// address is telling the truth about that.
    pub fn name_for(&self, topic: &Addr) -> Option<&str> {
        self.names.iter().find(|(t, _)| t == topic).map(|(_, n)| n.as_str())
    }

    /// Every topic this store has a name for, with the name.
    pub fn named(&self) -> Vec<(Addr, &str)> {
        let mut v: Vec<(Addr, &str)> = self.names.iter().map(|(t, n)| (*t, n.as_str())).collect();
        v.sort_by_key(|(_, n)| n.to_lowercase());
        v
    }

    /// Does a name hash to this topic?
    ///
    /// The one check this store can make about itself: a name is only the right
    /// name if `topic_of(name)` is the address it is filed under. A name that
    /// fails this was typed for a different topic, or the blob was tampered
    /// with, and either way showing it would mislabel a feed.
    pub fn name_matches(&self, topic: &Addr) -> bool {
        self.name_for(topic).is_some_and(|n| crate::topic_of(n) == *topic)
    }

    // ----------------------------------------------------------------- posts

    /// File an arriving post.
    ///
    /// `at` is **required**, where the JS store fell back to `Date.now()`. That
    /// fallback cannot come along: `SystemTime::now()` compiles for
    /// `wasm32-unknown-unknown` and panics at runtime (M10 rule 4), so the choice
    /// is not between a fallback and a parameter — it is between a parameter and
    /// a browser node that dies on the first post with no timestamp. The host
    /// knows the time; it passes it in, as it does everywhere else in this crate.
    pub fn receive(&mut self, topic: Addr, from: Option<Addr>, body: &str, at: u32) {
        let post = Post { from, body: body.to_string(), at };
        let list = match self.posts.iter_mut().find(|(t, _)| *t == topic) {
            Some((_, l)) => l,
            None => {
                self.posts.push((topic, Vec::new()));
                &mut self.posts.last_mut().expect("just pushed").1
            }
        };
        list.push(post);
        if list.len() > MAX_POSTS {
            let excess = list.len() - MAX_POSTS;
            list.drain(..excess);
        }
    }

    /// Posts on a topic, oldest first.
    pub fn posts_on(&self, topic: &Addr) -> &[Post] {
        self.posts.iter().find(|(t, _)| t == topic).map(|(_, p)| p.as_slice()).unwrap_or(&[])
    }

    /// The most recent post on a topic.
    pub fn latest_on(&self, topic: &Addr) -> Option<&Post> {
        self.posts_on(topic).last()
    }

    // ----------------------------------------------------------- persistence

    /// Encode for the host's storage port. Same shape as the other stores.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = vec![ENCODING_VERSION];
        out.extend_from_slice(&(self.names.len() as u32).to_be_bytes());
        for (topic, name) in &self.names {
            out.extend_from_slice(topic);
            out.extend_from_slice(&(name.len() as u32).to_be_bytes());
            out.extend_from_slice(name.as_bytes());
        }
        out.extend_from_slice(&(self.posts.len() as u32).to_be_bytes());
        for (topic, list) in &self.posts {
            out.extend_from_slice(topic);
            out.extend_from_slice(&(list.len() as u32).to_be_bytes());
            for p in list {
                match &p.from {
                    Some(a) => {
                        out.push(1);
                        out.extend_from_slice(a);
                    }
                    None => out.push(0),
                }
                out.extend_from_slice(&p.at.to_be_bytes());
                out.extend_from_slice(&(p.body.len() as u32).to_be_bytes());
                out.extend_from_slice(p.body.as_bytes());
            }
        }
        out
    }

    /// Decode what [`TopicStore::encode`] wrote.
    ///
    /// `None` for anything malformed. The caller starts empty and leaves the
    /// blob alone: losing a session of posts beats destroying the names the user
    /// typed because one parse failed, and the names are the half that cannot be
    /// recovered from the mesh.
    pub fn decode(bytes: &[u8]) -> Option<TopicStore> {
        let mut c = super::Cursor::new(bytes);
        if c.u8()? != ENCODING_VERSION {
            return None;
        }
        let mut s = TopicStore::new();

        let n_names = c.u32()? as usize;
        if n_names > c.remaining() {
            return None;
        }
        for _ in 0..n_names {
            let topic = c.addr()?;
            let len = c.u32()? as usize;
            let name = String::from_utf8(c.take(len)?.to_vec()).ok()?;
            if !name.trim().is_empty() {
                s.names.push((topic, name));
            }
        }

        let n_topics = c.u32()? as usize;
        if n_topics > c.remaining() {
            return None;
        }
        for _ in 0..n_topics {
            let topic = c.addr()?;
            let n_posts = c.u32()? as usize;
            if n_posts > c.remaining() {
                return None;
            }
            let mut list = Vec::with_capacity(n_posts.min(MAX_POSTS));
            for _ in 0..n_posts {
                let from = match c.u8()? {
                    0 => None,
                    1 => Some(c.addr()?),
                    _ => return None,
                };
                let at = c.u32()?;
                let len = c.u32()? as usize;
                let body = String::from_utf8(c.take(len)?.to_vec()).ok()?;
                list.push(Post { from, body, at });
            }
            // Re-apply the cap on the way in. A blob written by a build with a
            // larger `MAX_POSTS`, or one a host edited, must not let a stored
            // file raise this node's memory ceiling — the bound belongs to the
            // running node, not to the bytes it was handed.
            if list.len() > MAX_POSTS {
                let excess = list.len() - MAX_POSTS;
                list.drain(..excess);
            }
            s.posts.push((topic, list));
        }

        if !c.at_end() {
            return None;
        }
        Some(s)
    }
}

const ENCODING_VERSION: u8 = 1;

#[cfg(test)]
mod tests {
    //! Mirrors `web/app/stores/topics.test.mjs`.
    use super::*;

    fn t(name: &str) -> Addr {
        crate::topic_of(name)
    }

    const SENDER: Addr = [7; 8];

    #[test]
    fn a_topic_has_no_name_until_the_user_gives_it_one() {
        let mut s = TopicStore::new();
        let topic = t("ridge-weather");
        assert_eq!(s.name_for(&topic), None, "it does not invent a name");
        s.remember(topic, "ridge-weather");
        assert_eq!(s.name_for(&topic), Some("ridge-weather"));
    }

    #[test]
    fn a_name_is_only_right_if_it_hashes_to_the_topic_it_is_filed_under() {
        // The one self-check available: a topic address is a hash of its name,
        // so a mislabelled feed is detectable without asking anyone.
        let mut s = TopicStore::new();
        let topic = t("tides");
        s.remember(topic, "tides");
        assert!(s.name_matches(&topic));

        s.remember(topic, "not-tides");
        assert!(!s.name_matches(&topic), "a name for a different topic must not pass");
    }

    #[test]
    fn an_empty_name_is_not_stored() {
        let mut s = TopicStore::new();
        let topic = t("x");
        s.remember(topic, "   ");
        assert_eq!(s.name_for(&topic), None, "a blank name would claim to be a name");
    }

    #[test]
    fn a_post_with_no_authenticated_sender_is_kept_with_no_sender() {
        // Unlike a direct message, this is ordinary: a feed post floods and need
        // not be signed. What must not happen is inventing one.
        let mut s = TopicStore::new();
        let topic = t("open");
        s.receive(topic, None, "anyone can say this", 10);
        assert_eq!(s.posts_on(&topic).len(), 1);
        assert_eq!(s.posts_on(&topic)[0].from, None);
    }

    #[test]
    fn posts_are_capped_and_the_newest_survive() {
        let mut s = TopicStore::new();
        let topic = t("loud");
        for i in 0..500 {
            s.receive(topic, Some(SENDER), &format!("p{i}"), i);
        }
        let kept = s.posts_on(&topic);
        assert_eq!(kept.len(), MAX_POSTS, "a public feed does not get to choose our memory");
        assert_eq!(kept[kept.len() - 1].body, "p499", "the newest is what survives");
        assert_eq!(kept[0].body, format!("p{}", 500 - MAX_POSTS));
    }

    #[test]
    fn forgetting_a_topic_drops_its_name_and_its_posts() {
        let mut s = TopicStore::new();
        let topic = t("gone");
        s.remember(topic, "gone");
        s.receive(topic, None, "hello", 10);
        s.forget(&topic);
        assert_eq!(s.name_for(&topic), None);
        assert!(s.posts_on(&topic).is_empty());
    }

    #[test]
    fn latest_is_the_most_recent_post_or_nothing() {
        let mut s = TopicStore::new();
        let topic = t("tides");
        assert_eq!(s.latest_on(&topic), None);
        s.receive(topic, None, "low at 08:10", 10);
        s.receive(topic, None, "high at 14:20", 20);
        assert_eq!(s.latest_on(&topic).map(|p| p.body.as_str()), Some("high at 14:20"));
    }

    #[test]
    fn a_round_trip_through_storage_preserves_everything() {
        let mut s = TopicStore::new();
        let topic = t("tides");
        s.remember(topic, "tides");
        s.receive(topic, Some(SENDER), "high at 14:20", 20);
        s.receive(topic, None, "unsigned too", 21);

        let b = TopicStore::decode(&s.encode()).expect("its own output must decode");
        assert_eq!(b.name_for(&topic), Some("tides"));
        assert_eq!(b.posts_on(&topic).len(), 2);
        assert_eq!(b.latest_on(&topic).map(|p| p.body.as_str()), Some("unsigned too"));
        assert_eq!(b.posts_on(&topic)[0].from, Some(SENDER));
        assert_eq!(b.posts_on(&topic)[1].from, None);
    }

    #[test]
    fn a_stored_blob_cannot_raise_this_node_s_memory_ceiling() {
        // The bound belongs to the running node, not to the bytes it was handed.
        // A blob written by a build with a larger cap — or one a host edited —
        // must be trimmed on the way in rather than believed.
        let topic = t("loud");
        let mut out = vec![ENCODING_VERSION];
        out.extend_from_slice(&0u32.to_be_bytes()); // no names
        out.extend_from_slice(&1u32.to_be_bytes()); // one topic
        out.extend_from_slice(&topic);
        let n = MAX_POSTS + 50;
        out.extend_from_slice(&(n as u32).to_be_bytes());
        for i in 0..n {
            out.push(0); // no sender
            out.extend_from_slice(&(i as u32).to_be_bytes());
            let body = format!("p{i}");
            out.extend_from_slice(&(body.len() as u32).to_be_bytes());
            out.extend_from_slice(body.as_bytes());
        }
        let s = TopicStore::decode(&out).expect("a legal, oversized blob still decodes");
        assert_eq!(s.posts_on(&topic).len(), MAX_POSTS, "trimmed on the way in");
        assert_eq!(s.latest_on(&topic).unwrap().body, format!("p{}", n - 1), "newest kept");
    }

    #[test]
    fn the_encoding_is_deterministic() {
        let build = || {
            let mut s = TopicStore::new();
            s.remember(t("a"), "a");
            s.remember(t("b"), "b");
            s.receive(t("a"), Some(SENDER), "one", 10);
            s
        };
        assert_eq!(build().encode(), build().encode());
    }

    #[test]
    fn a_corrupt_blob_decodes_to_nothing_rather_than_panicking() {
        assert!(TopicStore::decode(b"").is_none());
        assert!(TopicStore::decode(b"{\"names\":{}}").is_none(), "the JS store's format");
        assert!(TopicStore::decode(&[99, 0, 0, 0, 0]).is_none(), "unknown version");

        let mut s = TopicStore::new();
        s.remember(t("tides"), "tides");
        s.receive(t("tides"), Some(SENDER), "high", 20);
        let good = s.encode();
        for cut in 0..good.len() {
            assert!(TopicStore::decode(&good[..cut]).is_none(), "truncated to {cut} decoded anyway");
        }
        let mut extra = good.clone();
        extra.push(0);
        assert!(TopicStore::decode(&extra).is_none(), "trailing bytes must be refused");
        assert!(TopicStore::decode(&good).is_some());
    }

    #[test]
    fn a_hostile_count_cannot_make_the_decoder_allocate() {
        let mut b = vec![ENCODING_VERSION];
        b.extend_from_slice(&u32::MAX.to_be_bytes());
        assert!(TopicStore::decode(&b).is_none(), "a huge name count");

        let mut b = vec![ENCODING_VERSION];
        b.extend_from_slice(&0u32.to_be_bytes());
        b.extend_from_slice(&u32::MAX.to_be_bytes());
        assert!(TopicStore::decode(&b).is_none(), "a huge topic count");

        let mut b = vec![ENCODING_VERSION];
        b.extend_from_slice(&0u32.to_be_bytes());
        b.extend_from_slice(&1u32.to_be_bytes());
        b.extend_from_slice(&t("x"));
        b.extend_from_slice(&u32::MAX.to_be_bytes());
        assert!(TopicStore::decode(&b).is_none(), "a huge post count");
    }

    #[test]
    fn named_lists_every_named_topic_in_order() {
        let mut s = TopicStore::new();
        s.remember(t("zebra"), "zebra");
        s.remember(t("apple"), "apple");
        s.receive(t("unnamed"), None, "no name here", 10);
        let names: Vec<&str> = s.named().iter().map(|(_, n)| *n).collect();
        assert_eq!(names, vec!["apple", "zebra"], "a topic with only posts has no name to list");
    }
}
