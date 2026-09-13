//! **Direct-message threads**, keyed by peer address (M10-B, closing G1 and G3).
//!
//! The Rust port of `web/app/stores/threads.mjs`, which was written in JS
//! deliberately and temporarily: M10's sequencing is contract-first, so screens
//! were built against `SporeClient` and each store moves here afterwards *without
//! its callers changing*. The JS was kept free of DOM and client references for
//! exactly this reason, so this is a port rather than a redesign, and its tests
//! mirror `threads.test.mjs` case for case so "unchanged behaviour" is a thing
//! CI checks rather than a thing the commit message claims.
//!
//! # The two rules this store exists to enforce
//!
//! **A thread is keyed on the authenticated sender only.** [`ThreadStore::receive`]
//! takes `Option<Addr>`, and `None` — an unsigned envelope, a failed signature, or
//! an `SRC8` address the envelope cannot prove — is *counted* and never filed.
//! A conversation list keyed on anything weaker is spoofable: anyone in radio
//! range could put words in a contact's thread by claiming their address, which is
//! precisely the attack the signature exists to stop. The count is kept rather
//! than discarded because silently dropping mail is its own failure.
//!
//! **An optimistic send is a real row with the true envelope id.** Not a separate
//! pending list merged at render time. Acks reconcile by id
//! ([`ThreadStore::set_status`]) — never by position and never by guessing, both
//! of which are wrong the moment two sends are in flight at once.
//!
//! # What it does not do
//!
//! It never invents a display name. The only name it could invent is one an
//! envelope *claimed* rather than proved, and resolving an address to a person is
//! the contact store's job. [`Conversation`] therefore carries an address and no
//! name, and a caller that wants one has to go and get it.

use crate::Addr;

/// A tab is not an archive. Oldest messages fall off first.
///
/// The bound belongs here rather than in the host because it is the only place
/// that can enforce it uniformly: a browser, a phone and a CLI all reimplementing
/// "keep the last N" is three chances to keep none.
pub const MAX_PER_THREAD: usize = 500;

/// Where a message is in its life. `Received` is terminal for inbound mail; the
/// rest describe something this node sent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MessageStatus {
    Queued,
    Sent,
    Acked,
    Expired,
    Received,
}

impl MessageStatus {
    /// The wire name, which is also the name the JS store used. Kept identical so
    /// a persisted blob written by either side reads on the other.
    pub fn as_str(&self) -> &'static str {
        match self {
            MessageStatus::Queued => "queued",
            MessageStatus::Sent => "sent",
            MessageStatus::Acked => "acked",
            MessageStatus::Expired => "expired",
            MessageStatus::Received => "received",
        }
    }
}

/// The name an unknown status string was rejected under. A named type rather
/// than `()` so a host ABI can report *what* it was handed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnknownStatus(pub String);

impl std::str::FromStr for MessageStatus {
    type Err = UnknownStatus;

    /// The inverse of [`MessageStatus::as_str`]. A host ABI takes commands as
    /// text, so this is the direction that faces a UI shim.
    fn from_str(s: &str) -> Result<MessageStatus, UnknownStatus> {
        Ok(match s {
            "queued" => MessageStatus::Queued,
            "sent" => MessageStatus::Sent,
            "acked" => MessageStatus::Acked,
            "expired" => MessageStatus::Expired,
            "received" => MessageStatus::Received,
            other => return Err(UnknownStatus(other.to_string())),
        })
    }
}

/// One message in one thread.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    /// The envelope id, for messages this node sent. `None` for received mail:
    /// an inbound envelope's id is not something this store needs to reconcile,
    /// and storing it would invite reconciling *by* it, which is how a peer
    /// would get to move the status of a row it does not own.
    pub id: Option<crate::Id>,
    /// True when this node wrote it.
    pub self_authored: bool,
    pub body: String,
    /// When, as the host reported it. This store never reads a clock (rule 4).
    pub at: u32,
    pub sealed: bool,
    pub status: MessageStatus,
}

/// A row in the conversation list. Carries no name — see the module docs.
#[derive(Clone, Debug)]
pub struct Conversation {
    pub addr: Addr,
    pub last_body: String,
    pub last_at: u32,
    pub last_self: bool,
    pub unread: usize,
}

/// Direct-message threads and their unread counts.
#[derive(Default)]
pub struct ThreadStore {
    /// `addr -> messages`, oldest first. A `Vec` of pairs rather than a map so
    /// the encoding below is deterministic: two nodes persisting the same
    /// history must produce the same bytes, and `HashMap` iteration order does
    /// not promise that.
    threads: Vec<(Addr, Vec<Message>)>,
    unread: Vec<(Addr, usize)>,
    /// Envelopes that arrived without a provable sender. Surfaced, not filed.
    unauthenticated: usize,
}

impl ThreadStore {
    pub fn new() -> ThreadStore {
        ThreadStore::default()
    }

    // ---------------------------------------------------------------- writes

    /// A message arrived. Returns the conversation it was filed under, or `None`
    /// when the sender could not be authenticated.
    ///
    /// `from` is `None` for an unsigned envelope, one whose signature did not
    /// verify, or an `SRC8` frame — every case where the envelope names an
    /// address it cannot prove.
    pub fn receive(&mut self, from: Option<Addr>, body: &str, sealed: bool, at: u32) -> Option<Addr> {
        let Some(from) = from else {
            self.unauthenticated += 1;
            return None;
        };
        self.append(
            from,
            Message {
                id: None,
                self_authored: false,
                body: body.to_string(),
                at,
                sealed,
                status: MessageStatus::Received,
            },
        );
        let slot = self.unread_slot(from);
        *slot += 1;
        Some(from)
    }

    /// Record a locally originated send, with the id the kernel gave it.
    pub fn send(&mut self, to: Addr, id: crate::Id, body: &str, sealed: bool, at: u32) -> Addr {
        self.append(
            to,
            Message {
                id: Some(id),
                self_authored: true,
                body: body.to_string(),
                at,
                sealed,
                status: MessageStatus::Queued,
            },
        );
        to
    }

    /// Reconcile by envelope id. Returns whether a row moved.
    ///
    /// By id, never by position: two sends in flight at once make position a
    /// coin flip, and the row that moves would be whichever one rendered first.
    pub fn set_status(&mut self, id: &crate::Id, status: MessageStatus) -> bool {
        for (_, msgs) in self.threads.iter_mut() {
            for m in msgs.iter_mut() {
                if m.id.as_ref() == Some(id) {
                    m.status = status;
                    return true;
                }
            }
        }
        false
    }

    pub fn mark_read(&mut self, addr: Addr) {
        *self.unread_slot(addr) = 0;
    }

    fn append(&mut self, addr: Addr, msg: Message) {
        let list = match self.threads.iter_mut().find(|(a, _)| *a == addr) {
            Some((_, l)) => l,
            None => {
                self.threads.push((addr, Vec::new()));
                &mut self.threads.last_mut().expect("just pushed").1
            }
        };
        list.push(msg);
        if list.len() > MAX_PER_THREAD {
            let excess = list.len() - MAX_PER_THREAD;
            list.drain(..excess);
        }
    }

    fn unread_slot(&mut self, addr: Addr) -> &mut usize {
        if let Some(i) = self.unread.iter().position(|(a, _)| *a == addr) {
            return &mut self.unread[i].1;
        }
        self.unread.push((addr, 0));
        &mut self.unread.last_mut().expect("just pushed").1
    }

    // ----------------------------------------------------------------- reads

    pub fn messages(&self, addr: &Addr) -> &[Message] {
        self.threads.iter().find(|(a, _)| a == addr).map(|(_, m)| m.as_slice()).unwrap_or(&[])
    }

    pub fn unread_for(&self, addr: &Addr) -> usize {
        self.unread.iter().find(|(a, _)| a == addr).map(|(_, n)| *n).unwrap_or(0)
    }

    pub fn total_unread(&self) -> usize {
        self.unread.iter().map(|(_, n)| *n).sum()
    }

    /// How many envelopes arrived that could not be attributed to anyone.
    pub fn unauthenticated_count(&self) -> usize {
        self.unauthenticated
    }

    /// Conversation rows, most recently active first.
    pub fn conversations(&self) -> Vec<Conversation> {
        let mut rows: Vec<Conversation> = self
            .threads
            .iter()
            .map(|(addr, msgs)| {
                let last = msgs.last();
                Conversation {
                    addr: *addr,
                    last_body: last.map(|m| m.body.clone()).unwrap_or_default(),
                    last_at: last.map(|m| m.at).unwrap_or(0),
                    last_self: last.map(|m| m.self_authored).unwrap_or(false),
                    unread: self.unread_for(addr),
                }
            })
            .collect();
        // Most recent first. A stable sort, so two threads whose last message
        // shares a timestamp keep insertion order instead of shuffling on every
        // render — the kind of thing a user reads as a bug.
        rows.sort_by_key(|c| std::cmp::Reverse(c.last_at));
        rows
    }

    // ----------------------------------------------------------- persistence

    /// Encode for the host's storage port.
    ///
    /// Hand-rolled and length-prefixed, like every other format in this crate:
    /// no serde, because rule 3 says no new dependencies and the alternative is
    /// pulling a derive macro into a tree that has to build on Espressif's Xtensa
    /// fork. Versioned, because this *will* change — it is application state, not
    /// wire format, and it is under no freeze.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = vec![ENCODING_VERSION];
        put_u32(&mut out, self.threads.len() as u32);
        for (addr, msgs) in &self.threads {
            out.extend_from_slice(addr);
            put_u32(&mut out, msgs.len() as u32);
            for m in msgs {
                out.push(match m.id {
                    Some(_) => 1,
                    None => 0,
                });
                if let Some(id) = &m.id {
                    out.extend_from_slice(id);
                }
                out.push(u8::from(m.self_authored));
                out.push(u8::from(m.sealed));
                out.push(status_code(m.status));
                put_u32(&mut out, m.at);
                let body = m.body.as_bytes();
                put_u32(&mut out, body.len() as u32);
                out.extend_from_slice(body);
            }
        }
        put_u32(&mut out, self.unread.len() as u32);
        for (addr, n) in &self.unread {
            out.extend_from_slice(addr);
            put_u32(&mut out, *n as u32);
        }
        put_u32(&mut out, self.unauthenticated as u32);
        out
    }

    /// Decode what [`ThreadStore::encode`] wrote.
    ///
    /// `None` for anything malformed, truncated, or from a version this build
    /// does not know. The caller's correct response is to **start empty and leave
    /// the stored blob alone** — the JS store's comment is the right one and is
    /// worth repeating: a corrupt blob is not worth crashing a node over, and
    /// silently wiping it would be worse, because the bytes may be the only copy
    /// of a conversation and a future build may know how to read them.
    pub fn decode(bytes: &[u8]) -> Option<ThreadStore> {
        let mut c = Cursor { b: bytes, i: 0 };
        if c.u8()? != ENCODING_VERSION {
            return None;
        }
        let mut s = ThreadStore::new();
        let n_threads = c.u32()? as usize;
        // Every count is checked against what is actually left rather than
        // trusted, so a corrupt length cannot make this allocate for a thread
        // that is not there. Storage is host-owned and a host is not trusted to
        // have kept what it was handed — the same rule `JsSpill::get` applies.
        if n_threads > c.remaining() {
            return None;
        }
        for _ in 0..n_threads {
            let addr = c.addr()?;
            let n_msgs = c.u32()? as usize;
            if n_msgs > c.remaining() {
                return None;
            }
            let mut msgs = Vec::with_capacity(n_msgs.min(MAX_PER_THREAD));
            for _ in 0..n_msgs {
                let id = match c.u8()? {
                    0 => None,
                    1 => Some(c.id()?),
                    _ => return None,
                };
                let self_authored = c.bool()?;
                let sealed = c.bool()?;
                let status = status_of(c.u8()?)?;
                let at = c.u32()?;
                let len = c.u32()? as usize;
                let body = String::from_utf8(c.take(len)?.to_vec()).ok()?;
                msgs.push(Message { id, self_authored, body, at, sealed, status });
            }
            s.threads.push((addr, msgs));
        }
        let n_unread = c.u32()? as usize;
        if n_unread > c.remaining() {
            return None;
        }
        for _ in 0..n_unread {
            let addr = c.addr()?;
            let n = c.u32()? as usize;
            s.unread.push((addr, n));
        }
        s.unauthenticated = c.u32()? as usize;
        // Trailing bytes mean this is not what it claims to be. Refusing is the
        // conservative read: a shorter-than-expected parse that "worked" would
        // silently drop whatever followed.
        if c.i != bytes.len() {
            return None;
        }
        Some(s)
    }
}

const ENCODING_VERSION: u8 = 1;

fn status_code(s: MessageStatus) -> u8 {
    match s {
        MessageStatus::Queued => 0,
        MessageStatus::Sent => 1,
        MessageStatus::Acked => 2,
        MessageStatus::Expired => 3,
        MessageStatus::Received => 4,
    }
}

fn status_of(c: u8) -> Option<MessageStatus> {
    Some(match c {
        0 => MessageStatus::Queued,
        1 => MessageStatus::Sent,
        2 => MessageStatus::Acked,
        3 => MessageStatus::Expired,
        4 => MessageStatus::Received,
        _ => return None,
    })
}

fn put_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_be_bytes());
}

/// A bounds-checked reader. Every read is fallible, so a truncated blob returns
/// `None` rather than panicking on a slice — the host's storage is the one input
/// here that is neither signed nor verified.
struct Cursor<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> Cursor<'a> {
    fn remaining(&self) -> usize {
        self.b.len() - self.i
    }

    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let end = self.i.checked_add(n)?;
        let out = self.b.get(self.i..end)?;
        self.i = end;
        Some(out)
    }

    fn u8(&mut self) -> Option<u8> {
        Some(self.take(1)?[0])
    }

    fn bool(&mut self) -> Option<bool> {
        match self.u8()? {
            0 => Some(false),
            1 => Some(true),
            _ => None,
        }
    }

    fn u32(&mut self) -> Option<u32> {
        let b = self.take(4)?;
        Some(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn addr(&mut self) -> Option<Addr> {
        let b = self.take(8)?;
        let mut a = [0u8; 8];
        a.copy_from_slice(b);
        Some(a)
    }

    fn id(&mut self) -> Option<crate::Id> {
        let b = self.take(16)?;
        let mut id = [0u8; 16];
        id.copy_from_slice(b);
        Some(id)
    }
}

/// How a thread renders: a sticky day divider, and consecutive messages from one
/// author collapsed into a run so only the first carries an avatar.
///
/// Returns a flat list so the renderer stays a simple map. The day label is the
/// caller's to compute — formatting a date needs a locale and a timezone, and
/// this module does not read a clock (rule 4) or know where the user is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ThreadItem<'a> {
    Day(String),
    Message { message: &'a Message, run: Run },
}

/// Where a message sits in a run of consecutive messages by the same author.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Run {
    Only,
    First,
    Mid,
    Last,
}

/// Group a thread into day-separated runs.
///
/// `day_label_of` maps a timestamp to whatever the host wants to show — "Today",
/// a date, a locale-specific string. Two messages are on the same day exactly
/// when it returns the same label for both, which keeps the timezone question
/// entirely on the host's side of the line.
pub fn group_thread<F>(messages: &[Message], mut day_label_of: F) -> Vec<ThreadItem<'_>>
where
    F: FnMut(u32) -> String,
{
    let mut out = Vec::new();
    let mut last_day: Option<String> = None;
    let mut last_author: Option<bool> = None;

    for (i, m) in messages.iter().enumerate() {
        let day = day_label_of(m.at);
        if last_day.as_deref() != Some(day.as_str()) {
            out.push(ThreadItem::Day(day.clone()));
            last_day = Some(day.clone());
            last_author = None;
        }
        let author = m.self_authored;
        let same_as_prev = last_author == Some(author);
        let same_as_next =
            messages.get(i + 1).is_some_and(|n| n.self_authored == author && day_label_of(n.at) == day);

        let run = match (same_as_prev, same_as_next) {
            (true, true) => Run::Mid,
            (true, false) => Run::Last,
            (false, true) => Run::First,
            (false, false) => Run::Only,
        };
        out.push(ThreadItem::Message { message: m, run });
        last_author = Some(author);
    }
    out
}

#[cfg(test)]
mod tests {
    //! Mirrors `web/app/stores/threads.test.mjs` case for case.
    //!
    //! M10's promise is that a store moves into Rust *behind an unchanged
    //! interface*. That is only worth anything if the behaviour is unchanged too,
    //! and the cheapest way to mean it is to port the assertions rather than
    //! write fresh ones — fresh ones would be written against this code, which is
    //! the thing under test.
    use super::*;

    const ADDR: Addr = [1, 2, 3, 4, 5, 6, 7, 8];
    const OTHER: Addr = [9, 9, 9, 9, 9, 9, 9, 9];

    fn id(n: u8) -> crate::Id {
        [n; 16]
    }

    #[test]
    fn an_unauthenticated_sender_creates_no_conversation_but_is_still_counted() {
        let mut s = ThreadStore::new();
        assert_eq!(s.receive(None, "hello", false, 10), None);
        assert_eq!(s.conversations().len(), 0, "no conversation may be created");
        assert_eq!(s.unauthenticated_count(), 1, "but it must not vanish silently either");
    }

    #[test]
    fn a_received_message_is_filed_and_counted_unread() {
        let mut s = ThreadStore::new();
        assert_eq!(s.receive(Some(ADDR), "hello", true, 10), Some(ADDR));
        assert_eq!(s.messages(&ADDR).len(), 1);
        assert_eq!(s.unread_for(&ADDR), 1);
        assert!(s.messages(&ADDR)[0].sealed);
    }

    #[test]
    fn a_send_is_a_real_row_carrying_the_true_envelope_id() {
        let mut s = ThreadStore::new();
        s.send(ADDR, id(0xab), "outbound", false, 20);
        let m = &s.messages(&ADDR)[0];
        assert_eq!(m.status, MessageStatus::Queued);
        assert_eq!(m.id, Some(id(0xab)));
        assert!(m.self_authored);
        assert_eq!(s.unread_for(&ADDR), 0, "a message this node sent is not unread mail");
    }

    #[test]
    fn an_ack_moves_the_row_with_that_id_and_no_other() {
        let mut s = ThreadStore::new();
        s.send(ADDR, id(1), "first", false, 20);
        s.send(ADDR, id(2), "second", false, 21);
        assert!(s.set_status(&id(2), MessageStatus::Acked));
        assert_eq!(s.messages(&ADDR)[0].status, MessageStatus::Queued, "the wrong row must not move");
        assert_eq!(s.messages(&ADDR)[1].status, MessageStatus::Acked);
    }

    #[test]
    fn an_ack_for_an_id_we_never_sent_changes_nothing() {
        let mut s = ThreadStore::new();
        s.send(ADDR, id(1), "first", false, 20);
        assert!(!s.set_status(&id(0xff), MessageStatus::Acked));
        assert_eq!(s.messages(&ADDR)[0].status, MessageStatus::Queued);
    }

    #[test]
    fn received_mail_carries_no_id_so_an_ack_can_never_move_it() {
        // The JS store guarded this by checking `m.id &&` before comparing. Here
        // the type does it: received messages have `None`, and `None == Some(_)`
        // is false, so there is no id a peer could name to move a row it does not
        // own. Asserted anyway, because the guarantee is the point rather than
        // the mechanism.
        let mut s = ThreadStore::new();
        s.receive(Some(ADDR), "inbound", false, 10);
        assert_eq!(s.messages(&ADDR)[0].id, None);
        assert!(!s.set_status(&id(0), MessageStatus::Acked));
        assert_eq!(s.messages(&ADDR)[0].status, MessageStatus::Received);
    }

    #[test]
    fn conversations_are_most_recently_active_first() {
        let mut s = ThreadStore::new();
        s.receive(Some(ADDR), "older", false, 10);
        s.receive(Some(OTHER), "newer", false, 50);
        let order: Vec<Addr> = s.conversations().iter().map(|c| c.addr).collect();
        assert_eq!(order, vec![OTHER, ADDR]);
    }

    #[test]
    fn a_conversation_row_carries_an_address_and_never_a_name() {
        // Naming is the contact store's job: the only name this store could
        // invent is one an envelope claimed rather than proved. In JS that was a
        // `!('name' in row)` assertion; here it is enforced by `Conversation`
        // having no such field, which this test exists to keep true.
        let mut s = ThreadStore::new();
        s.receive(Some(ADDR), "hello", false, 10);
        let row = &s.conversations()[0];
        assert_eq!(row.addr, ADDR);
        assert_eq!(row.last_body, "hello");
        assert!(!row.last_self);
    }

    #[test]
    fn marking_read_clears_one_thread_and_leaves_the_others() {
        let mut s = ThreadStore::new();
        s.receive(Some(ADDR), "a", false, 10);
        s.receive(Some(OTHER), "b", false, 11);
        s.mark_read(ADDR);
        assert_eq!(s.unread_for(&ADDR), 0);
        assert_eq!(s.unread_for(&OTHER), 1);
        assert_eq!(s.total_unread(), 1);
    }

    #[test]
    fn a_thread_is_capped_and_the_oldest_fall_off_first() {
        let mut s = ThreadStore::new();
        for i in 0..(MAX_PER_THREAD + 10) {
            s.receive(Some(ADDR), &format!("m{i}"), false, i as u32);
        }
        let msgs = s.messages(&ADDR);
        assert_eq!(msgs.len(), MAX_PER_THREAD, "a tab is not an archive");
        assert_eq!(msgs[0].body, "m10", "the ten oldest are the ones that went");
        assert_eq!(msgs[MAX_PER_THREAD - 1].body, format!("m{}", MAX_PER_THREAD + 9));
    }

    #[test]
    fn a_round_trip_through_storage_preserves_everything() {
        let mut s = ThreadStore::new();
        s.receive(Some(ADDR), "persist me", true, 10);
        s.send(ADDR, id(7), "and me", false, 11);
        s.set_status(&id(7), MessageStatus::Acked);
        s.receive(Some(OTHER), "me too", false, 12);
        s.receive(None, "unattributable", false, 13);
        s.mark_read(OTHER);

        let b = ThreadStore::decode(&s.encode()).expect("its own output must decode");
        assert_eq!(b.messages(&ADDR).len(), 2);
        assert_eq!(b.messages(&ADDR)[0].body, "persist me");
        assert!(b.messages(&ADDR)[0].sealed);
        assert_eq!(b.messages(&ADDR)[1].id, Some(id(7)));
        assert_eq!(b.messages(&ADDR)[1].status, MessageStatus::Acked);
        assert_eq!(b.unread_for(&ADDR), 1);
        assert_eq!(b.unread_for(&OTHER), 0);
        assert_eq!(b.unauthenticated_count(), 1);
        assert_eq!(
            b.conversations().iter().map(|c| c.addr).collect::<Vec<_>>(),
            s.conversations().iter().map(|c| c.addr).collect::<Vec<_>>()
        );
    }

    #[test]
    fn every_status_survives_a_round_trip_through_its_name() {
        // The host ABI moves these as text, so the pair has to be total in both
        // directions — a status that renders as a name it cannot be parsed back
        // from is a row the UI can display and never update.
        use std::str::FromStr;
        for s in [
            MessageStatus::Queued,
            MessageStatus::Sent,
            MessageStatus::Acked,
            MessageStatus::Expired,
            MessageStatus::Received,
        ] {
            assert_eq!(MessageStatus::from_str(s.as_str()), Ok(s));
        }
        assert_eq!(MessageStatus::from_str("nonsense"), Err(UnknownStatus("nonsense".into())));
    }

    #[test]
    fn the_encoding_is_deterministic() {
        // Two nodes persisting the same history must produce the same bytes, or
        // every sync, diff and content-addressed backup above this layer becomes
        // noise. This is why the store is a Vec of pairs and not a HashMap.
        let build = || {
            let mut s = ThreadStore::new();
            s.receive(Some(ADDR), "one", false, 10);
            s.receive(Some(OTHER), "two", false, 11);
            s.send(ADDR, id(3), "three", true, 12);
            s
        };
        assert_eq!(build().encode(), build().encode());
    }

    #[test]
    fn a_corrupt_blob_decodes_to_nothing_rather_than_panicking() {
        assert!(ThreadStore::decode(b"").is_none(), "empty");
        assert!(ThreadStore::decode(b"{not json").is_none(), "the JS store's format");
        assert!(ThreadStore::decode(&[99, 0, 0, 0, 0]).is_none(), "unknown version");

        // Every truncation of a valid blob. This is the shape a half-written
        // storage write actually takes, and the one a hand-rolled parser gets
        // wrong: it is not enough that *some* corrupt input is refused.
        let mut s = ThreadStore::new();
        s.receive(Some(ADDR), "hello", true, 10);
        s.send(ADDR, id(2), "world", false, 11);
        let good = s.encode();
        for cut in 0..good.len() {
            assert!(
                ThreadStore::decode(&good[..cut]).is_none(),
                "a blob truncated to {cut} of {} bytes decoded anyway",
                good.len()
            );
        }
        // And trailing junk, which is the other half of the same mistake.
        let mut extra = good.clone();
        extra.push(0);
        assert!(ThreadStore::decode(&extra).is_none(), "trailing bytes must be refused");
        assert!(ThreadStore::decode(&good).is_some(), "the unmangled blob still reads");
    }

    #[test]
    fn a_hostile_length_field_cannot_make_the_decoder_allocate() {
        // `n_threads` and the per-message body length come from host storage,
        // which is neither signed nor verified. A count of 4 billion must be
        // refused against what is actually left rather than believed.
        let mut b = vec![ENCODING_VERSION];
        b.extend_from_slice(&u32::MAX.to_be_bytes());
        assert!(ThreadStore::decode(&b).is_none(), "a huge thread count must be refused");

        let mut b = vec![ENCODING_VERSION];
        b.extend_from_slice(&1u32.to_be_bytes());
        b.extend_from_slice(&ADDR);
        b.extend_from_slice(&u32::MAX.to_be_bytes());
        assert!(ThreadStore::decode(&b).is_none(), "a huge message count must be refused");
    }

    #[test]
    fn a_thread_groups_into_days_and_runs() {
        let day = |at: u32| if at < 100 { "Yesterday".to_string() } else { "Today".to_string() };
        let mut s = ThreadStore::new();
        s.receive(Some(ADDR), "a", false, 10);
        s.receive(Some(ADDR), "b", false, 11);
        s.send(ADDR, id(1), "c", false, 200);

        let out = group_thread(s.messages(&ADDR), day);
        let kinds: Vec<&str> = out
            .iter()
            .map(|i| match i {
                ThreadItem::Day(_) => "day",
                ThreadItem::Message { .. } => "message",
            })
            .collect();
        assert_eq!(kinds, vec!["day", "message", "message", "day", "message"]);
        assert_eq!(out[0], ThreadItem::Day("Yesterday".into()));
        match (&out[1], &out[2]) {
            (ThreadItem::Message { run: a, .. }, ThreadItem::Message { run: b, .. }) => {
                assert_eq!(*a, Run::First);
                assert_eq!(*b, Run::Last);
            }
            _ => panic!("expected two messages"),
        }
    }

    #[test]
    fn a_day_boundary_breaks_a_run_even_between_messages_by_one_author() {
        // The subtle one: same author either side, so a grouper that only looks
        // at authorship calls the second `Mid` and the renderer drops its avatar
        // directly under a day divider.
        let day = |at: u32| if at < 100 { "Mon".to_string() } else { "Tue".to_string() };
        let mut s = ThreadStore::new();
        s.receive(Some(ADDR), "a", false, 10);
        s.receive(Some(ADDR), "b", false, 200);
        let out = group_thread(s.messages(&ADDR), day);
        match (&out[1], &out[3]) {
            (ThreadItem::Message { run: a, .. }, ThreadItem::Message { run: b, .. }) => {
                assert_eq!(*a, Run::Only, "alone on its day");
                assert_eq!(*b, Run::Only, "and so is the next, despite the same author");
            }
            _ => panic!("expected a day divider before each message"),
        }
    }
}
