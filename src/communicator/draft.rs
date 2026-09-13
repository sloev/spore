//! **Unsent text, kept where the user left it** (M10-B, closing G4).
//!
//! The one store with no JS predecessor, because the behaviour did not exist:
//! `main.mjs` held `chatDraft` and `postDraft` as transient state and cleared
//! them on every navigation — opening a different conversation discarded what
//! you had typed in the last one, and a reload discarded all of it. That is the
//! gap, and it is the kind a user experiences as the app losing their work.
//!
//! # Scoped, so two kinds of draft cannot collide
//!
//! A conversation is keyed by a peer address and a feed by a topic address, and
//! both are eight bytes from the same space — `addr_of(pubkey)` and
//! `topic_of(name)` are both truncated SHA-256. A collision between a contact
//! and a topic is astronomically unlikely and would be *baffling* rather than
//! merely wrong, so the key carries which kind it is and the question never
//! arises.
//!
//! # Bounds
//!
//! These are the user's own words, not a stranger's, so the resource invariant
//! is not what is at stake — but a caller with a bug is. A draft is capped in
//! length and the store is capped in count, oldest-touched dropped first, so a
//! loop that writes a draft per rendered frame cannot fill the host's storage.

use crate::Addr;

/// Longest draft kept. Past this the text is truncated on a character boundary,
/// never split through one — a draft that comes back as invalid UTF-8 would be
/// worse than one that comes back short.
pub const MAX_DRAFT_BYTES: usize = 16 * 1024;

/// Most drafts kept at once. Reaching this drops the least recently touched.
pub const MAX_DRAFTS: usize = 64;

/// Which kind of composer a draft belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    /// A direct-message composer, keyed by peer address.
    Chat,
    /// A feed composer, keyed by topic address.
    Topic,
}

impl Scope {
    pub fn code(&self) -> u8 {
        match self {
            Scope::Chat => 0,
            Scope::Topic => 1,
        }
    }

    pub fn from_code(c: u8) -> Option<Scope> {
        match c {
            0 => Some(Scope::Chat),
            1 => Some(Scope::Topic),
            _ => None,
        }
    }
}

/// One unsent composer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Draft {
    pub scope: Scope,
    pub addr: Addr,
    pub text: String,
    /// When it was last written, as the host reported it. Used only to decide
    /// what to drop under [`MAX_DRAFTS`]; this module reads no clock.
    pub at: u32,
}

/// Drafts, oldest-touched first.
#[derive(Default)]
pub struct DraftStore {
    drafts: Vec<Draft>,
}

impl DraftStore {
    pub fn new() -> DraftStore {
        DraftStore::default()
    }

    /// Save what the user has typed. Empty text clears the draft instead of
    /// storing one, so an emptied composer leaves nothing behind to restore.
    pub fn set(&mut self, scope: Scope, addr: Addr, text: &str, at: u32) {
        if text.is_empty() {
            self.clear(scope, &addr);
            return;
        }
        let text = truncate_on_char_boundary(text, MAX_DRAFT_BYTES);
        match self.find(scope, &addr) {
            Some(i) => {
                self.drafts[i].text = text;
                self.drafts[i].at = at;
                // Touched, so it moves to the back and is the last to be dropped.
                let d = self.drafts.remove(i);
                self.drafts.push(d);
            }
            None => {
                if self.drafts.len() >= MAX_DRAFTS {
                    self.drafts.remove(0);
                }
                self.drafts.push(Draft { scope, addr, text, at });
            }
        }
    }

    pub fn get(&self, scope: Scope, addr: &Addr) -> Option<&str> {
        self.find(scope, addr).map(|i| self.drafts[i].text.as_str())
    }

    pub fn clear(&mut self, scope: Scope, addr: &Addr) -> bool {
        match self.find(scope, addr) {
            Some(i) => {
                self.drafts.remove(i);
                true
            }
            None => false,
        }
    }

    /// Every draft, oldest-touched first.
    pub fn all(&self) -> &[Draft] {
        &self.drafts
    }

    fn find(&self, scope: Scope, addr: &Addr) -> Option<usize> {
        self.drafts.iter().position(|d| d.scope == scope && d.addr == *addr)
    }

    // ----------------------------------------------------------- persistence

    pub fn encode(&self) -> Vec<u8> {
        let mut out = vec![ENCODING_VERSION];
        out.extend_from_slice(&(self.drafts.len() as u32).to_be_bytes());
        for d in &self.drafts {
            out.push(d.scope.code());
            out.extend_from_slice(&d.addr);
            out.extend_from_slice(&d.at.to_be_bytes());
            out.extend_from_slice(&(d.text.len() as u32).to_be_bytes());
            out.extend_from_slice(d.text.as_bytes());
        }
        out
    }

    pub fn decode(bytes: &[u8]) -> Option<DraftStore> {
        let mut c = super::Cursor::new(bytes);
        if c.u8()? != ENCODING_VERSION {
            return None;
        }
        let n = c.u32()? as usize;
        if n > c.remaining() {
            return None;
        }
        let mut s = DraftStore::new();
        for _ in 0..n {
            let scope = Scope::from_code(c.u8()?)?;
            let addr = c.addr()?;
            let at = c.u32()?;
            let text = c.string()?;
            // Both bounds re-applied on the way in: they belong to the running
            // node, not to a blob it was handed.
            s.drafts.push(Draft { scope, addr, text: truncate_on_char_boundary(&text, MAX_DRAFT_BYTES), at });
        }
        while s.drafts.len() > MAX_DRAFTS {
            s.drafts.remove(0);
        }
        if !c.at_end() {
            return None;
        }
        Some(s)
    }
}

const ENCODING_VERSION: u8 = 1;

/// Cut to at most `max` bytes without splitting a character.
///
/// `String::truncate` panics on a non-boundary index, and a draft is arbitrary
/// user text — the first person to paste an emoji at the wrong offset would take
/// the tab down.
fn truncate_on_char_boundary(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut end = max;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const PEER: Addr = [1; 8];
    const OTHER: Addr = [2; 8];

    #[test]
    fn a_draft_survives_switching_away_and_back() {
        // The whole gap: `main.mjs` cleared `chatDraft` on every navigation, so
        // opening another conversation threw away what you had typed.
        let mut s = DraftStore::new();
        s.set(Scope::Chat, PEER, "half a thought", 10);
        s.set(Scope::Chat, OTHER, "a different one", 11);
        assert_eq!(s.get(Scope::Chat, &PEER), Some("half a thought"));
        assert_eq!(s.get(Scope::Chat, &OTHER), Some("a different one"));
    }

    #[test]
    fn a_chat_and_a_topic_at_the_same_address_are_different_drafts() {
        // Both key spaces are truncated SHA-256, so a collision is possible and
        // would be baffling rather than merely wrong.
        let mut s = DraftStore::new();
        s.set(Scope::Chat, PEER, "to a person", 10);
        s.set(Scope::Topic, PEER, "to a feed", 10);
        assert_eq!(s.get(Scope::Chat, &PEER), Some("to a person"));
        assert_eq!(s.get(Scope::Topic, &PEER), Some("to a feed"));
    }

    #[test]
    fn emptying_a_composer_leaves_nothing_to_restore() {
        let mut s = DraftStore::new();
        s.set(Scope::Chat, PEER, "typed", 10);
        s.set(Scope::Chat, PEER, "", 11);
        assert_eq!(s.get(Scope::Chat, &PEER), None, "an emptied composer is not a draft");
        assert!(s.all().is_empty());
    }

    #[test]
    fn the_store_is_capped_and_drops_the_least_recently_touched() {
        let mut s = DraftStore::new();
        for i in 0..(MAX_DRAFTS + 10) {
            let mut a = [0u8; 8];
            a[0] = (i % 256) as u8;
            a[1] = (i / 256) as u8;
            s.set(Scope::Chat, a, &format!("d{i}"), i as u32);
        }
        assert_eq!(s.all().len(), MAX_DRAFTS);
        assert_eq!(s.all().last().unwrap().text, format!("d{}", MAX_DRAFTS + 9));
    }

    #[test]
    fn touching_a_draft_moves_it_out_of_the_firing_line() {
        let mut s = DraftStore::new();
        s.set(Scope::Chat, PEER, "old but loved", 1);
        for i in 0..MAX_DRAFTS {
            let mut a = [0u8; 8];
            a[0] = (i + 10) as u8;
            a[1] = 9;
            s.set(Scope::Chat, a, "filler", i as u32);
            // Keep re-touching the first one: it must not be the one dropped.
            s.set(Scope::Chat, PEER, "old but loved", 100 + i as u32);
        }
        assert_eq!(s.get(Scope::Chat, &PEER), Some("old but loved"));
    }

    #[test]
    fn an_over_long_draft_is_cut_without_splitting_a_character() {
        // `String::truncate` panics on a non-boundary index, and a draft is
        // arbitrary user text — the first pasted emoji at the wrong offset would
        // take the tab down.
        let mut s = DraftStore::new();
        let long = "é".repeat(MAX_DRAFT_BYTES); // two bytes each, so well over
        s.set(Scope::Chat, PEER, &long, 10);
        let kept = s.get(Scope::Chat, &PEER).unwrap();
        assert!(kept.len() <= MAX_DRAFT_BYTES);
        assert!(kept.chars().all(|c| c == 'é'), "cut on a boundary, not through one");
        assert!(!kept.is_empty());
    }

    #[test]
    fn a_round_trip_through_storage_preserves_everything() {
        let mut s = DraftStore::new();
        s.set(Scope::Chat, PEER, "to a person", 10);
        s.set(Scope::Topic, OTHER, "to a feed", 11);
        let b = DraftStore::decode(&s.encode()).expect("its own output decodes");
        assert_eq!(b.get(Scope::Chat, &PEER), Some("to a person"));
        assert_eq!(b.get(Scope::Topic, &OTHER), Some("to a feed"));
        assert_eq!(b.all().len(), 2);
    }

    #[test]
    fn the_encoding_is_deterministic() {
        let build = || {
            let mut s = DraftStore::new();
            s.set(Scope::Chat, PEER, "one", 10);
            s.set(Scope::Topic, OTHER, "two", 11);
            s
        };
        assert_eq!(build().encode(), build().encode());
    }

    #[test]
    fn a_corrupt_blob_decodes_to_nothing_rather_than_panicking() {
        assert!(DraftStore::decode(b"").is_none());
        assert!(DraftStore::decode(&[99, 0, 0, 0, 0]).is_none(), "unknown version");
        let mut s = DraftStore::new();
        s.set(Scope::Chat, PEER, "text", 10);
        let good = s.encode();
        for cut in 0..good.len() {
            assert!(DraftStore::decode(&good[..cut]).is_none(), "truncated to {cut}");
        }
        let mut extra = good.clone();
        extra.push(0);
        assert!(DraftStore::decode(&extra).is_none(), "trailing bytes");
        assert!(DraftStore::decode(&good).is_some());

        // An unknown scope byte is refused rather than guessed: filing a feed
        // draft into a conversation would put words in a composer aimed at a
        // person.
        let mut bad = good.clone();
        bad[5] = 9;
        assert!(DraftStore::decode(&bad).is_none(), "unknown scope");
    }

    #[test]
    fn a_hostile_count_cannot_make_the_decoder_allocate() {
        let mut b = vec![ENCODING_VERSION];
        b.extend_from_slice(&u32::MAX.to_be_bytes());
        assert!(DraftStore::decode(&b).is_none());
    }
}
