//! **One app-level command ABI** (M10-C), over the stores M10-B holds.
//!
//! # The problem this is the answer to
//!
//! Three host layers already export overlapping, divergent subsets of the same
//! kernel — `src/wasm.rs` (34 exports), `src/ffi.rs` (20, and frozen),
//! `android/jni` (64, and stateful). Each exports what the others do not, and
//! the communicator is then written again on top of each. Adding a fourth set of
//! per-method exports for the M10-B stores would be doing the same thing a
//! fourth time, faster.
//!
//! So this is **one entry point**: [`Communicator::call`] takes an encoded
//! command and returns an encoded response. Three wasm exports replace the forty
//! that a method-per-function ABI would need, and — the part that matters for
//! M10-F — a byte-in/byte-out call works unchanged over IPC. That is precisely
//! what "desktop is that ABI over IPC while the browser is the same ABI over
//! wasm" requires: the screens cannot tell the difference, because bytes do not
//! know what carried them.
//!
//! # Why bytes rather than JSON
//!
//! Rule 3 is no new dependencies, and a JSON codec that builds on Espressif's
//! Xtensa fork is not free. The hand-rolled format here is the same
//! length-prefixed shape the stores already persist in, sharing the same
//! bounds-checked [`Cursor`](super::Cursor).
//!
//! # The one rule of this encoding
//!
//! **A malformed command is a response, never a panic.** The caller is a host —
//! a browser tab, an IPC peer, an Android binder — and any of them can send
//! nonsense through a bug or a version skew. `call` therefore always returns
//! bytes, and [`ERR_BAD_COMMAND`] is a legitimate answer. A decoder that
//! `unwrap`s here takes the tab with it.

use super::{contact, draft, thread, topic, Cursor, Writer};
use crate::Addr;

// -- response tags ----------------------------------------------------------

/// The command was understood and did what it said.
pub const OK: u8 = 0;
/// The command did not decode: unknown tag, truncated, or trailing bytes.
///
/// Deliberately one code rather than several. A host that sent a malformed
/// command has a bug, and telling it *which* field was wrong invites parsing the
/// error instead of fixing the caller.
pub const ERR_BAD_COMMAND: u8 = 1;

// -- command tags -----------------------------------------------------------
//
// Explicit numbers, because these cross a version boundary: a page cached from
// last week talks to a wasm module built today. Renumbering silently repoints a
// command at a different operation, so the values are written down rather than
// left to declaration order.

pub const CMD_THREAD_RECEIVE: u8 = 0x01;
pub const CMD_THREAD_SEND: u8 = 0x02;
pub const CMD_THREAD_SET_STATUS: u8 = 0x03;
pub const CMD_THREAD_MARK_READ: u8 = 0x04;
pub const CMD_THREAD_MESSAGES: u8 = 0x05;
pub const CMD_THREAD_CONVERSATIONS: u8 = 0x06;
pub const CMD_THREAD_TOTAL_UNREAD: u8 = 0x07;
/// How many envelopes arrived that could not be attributed to anyone.
///
/// Surfaced rather than merely counted: a node quietly discarding mail it cannot
/// attribute looks identical, from the outside, to a node nobody is writing to.
pub const CMD_THREAD_UNAUTHENTICATED: u8 = 0x08;

pub const CMD_CONTACT_SET_LABEL: u8 = 0x10;
pub const CMD_CONTACT_SET_FOLLOWING: u8 = 0x11;
pub const CMD_CONTACT_SET_BLOCKED: u8 = 0x12;
pub const CMD_CONTACT_REMOVE: u8 = 0x13;
pub const CMD_CONTACT_ROWS: u8 = 0x14;
/// One contact's local state, without building the whole row list.
///
/// `contact_rows` answers "what should the list show"; this answers "what did
/// the user set for this one address", which an edit form needs and a filtered,
/// view-scoped row list cannot give it.
pub const CMD_CONTACT_GET: u8 = 0x15;

pub const CMD_TOPIC_REMEMBER: u8 = 0x20;
pub const CMD_TOPIC_FORGET: u8 = 0x21;
pub const CMD_TOPIC_RECEIVE: u8 = 0x22;
pub const CMD_TOPIC_POSTS: u8 = 0x23;
pub const CMD_TOPIC_NAMED: u8 = 0x24;

pub const CMD_DRAFT_SET: u8 = 0x40;
pub const CMD_DRAFT_GET: u8 = 0x41;
pub const CMD_DRAFT_CLEAR: u8 = 0x42;
pub const CMD_DRAFT_ALL: u8 = 0x43;

/// Hand back everything, for the host to persist.
pub const CMD_SAVE: u8 = 0x30;
/// Replace everything from a blob the host kept.
pub const CMD_LOAD: u8 = 0x31;

/// The three M10-B stores behind one call.
#[derive(Default)]
pub struct Communicator {
    pub threads: thread::ThreadStore,
    pub contacts: contact::ContactStore,
    pub topics: topic::TopicStore,
    pub drafts: draft::DraftStore,
}

impl Communicator {
    pub fn new() -> Communicator {
        Communicator::default()
    }

    /// Run one command. Always returns bytes: `[OK]` then the payload, or
    /// `[ERR_BAD_COMMAND]` alone.
    ///
    /// Peers are **carried by the command that needs them** rather than held on
    /// this struct. Two reasons, and the second is the one that decided it:
    /// a cached peer table goes stale the moment anything arrives, and not every
    /// host keeps its peer list where this layer could reach it — Android has
    /// its own, and a browser has the node. Putting them in the command lets
    /// each host answer from whatever it actually has, and lets a test answer
    /// from a list it wrote by hand.
    pub fn call(&mut self, cmd: &[u8]) -> Vec<u8> {
        match self.dispatch(cmd) {
            Some(payload) => {
                let mut w = Writer::new();
                w.u8(OK).bytes(&payload);
                w.into_vec()
            }
            None => vec![ERR_BAD_COMMAND],
        }
    }

    fn dispatch(&mut self, cmd: &[u8]) -> Option<Vec<u8>> {
        let mut c = Cursor::new(cmd);
        let tag = c.u8()?;
        let mut w = Writer::new();

        match tag {
            // -- threads ----------------------------------------------------
            CMD_THREAD_RECEIVE => {
                let from = opt_addr(&mut c)?;
                let body = c.string()?;
                let sealed = c.bool()?;
                let at = c.u32()?;
                end(&c)?;
                let filed = self.threads.receive(from, &body, sealed, at);
                w.opt_addr(&filed);
            }
            CMD_THREAD_SEND => {
                let to = c.addr()?;
                let id = c.id()?;
                let body = c.string()?;
                let sealed = c.bool()?;
                let at = c.u32()?;
                end(&c)?;
                self.threads.send(to, id, &body, sealed, at);
                w.bytes(&to);
            }
            CMD_THREAD_SET_STATUS => {
                let id = c.id()?;
                let status = thread::MessageStatus::from_str_code(c.u8()?)?;
                end(&c)?;
                w.bool(self.threads.set_status(&id, status));
            }
            CMD_THREAD_MARK_READ => {
                let addr = c.addr()?;
                end(&c)?;
                self.threads.mark_read(addr);
            }
            CMD_THREAD_MESSAGES => {
                let addr = c.addr()?;
                end(&c)?;
                let msgs = self.threads.messages(&addr);
                w.u32(msgs.len() as u32);
                for m in msgs {
                    match &m.id {
                        Some(id) => {
                            w.u8(1).bytes(id);
                        }
                        None => {
                            w.u8(0);
                        }
                    }
                    w.bool(m.self_authored).bool(m.sealed).u8(m.status.code()).u32(m.at).string(&m.body);
                }
            }
            CMD_THREAD_CONVERSATIONS => {
                end(&c)?;
                let rows = self.threads.conversations();
                w.u32(rows.len() as u32);
                for r in rows {
                    w.bytes(&r.addr)
                        .u32(r.last_at)
                        .bool(r.last_self)
                        .u32(r.unread as u32)
                        .string(&r.last_body);
                }
            }
            CMD_THREAD_TOTAL_UNREAD => {
                end(&c)?;
                w.u32(self.threads.total_unread() as u32);
            }
            CMD_THREAD_UNAUTHENTICATED => {
                end(&c)?;
                w.u32(self.threads.unauthenticated_count() as u32);
            }

            // -- contacts ---------------------------------------------------
            CMD_CONTACT_SET_LABEL => {
                let addr = c.addr()?;
                let label = c.string()?;
                end(&c)?;
                self.contacts.set_label(addr, &label);
            }
            CMD_CONTACT_SET_FOLLOWING => {
                let addr = c.addr()?;
                let v = c.bool()?;
                end(&c)?;
                self.contacts.set_following(addr, v);
            }
            CMD_CONTACT_SET_BLOCKED => {
                let addr = c.addr()?;
                let v = c.bool()?;
                end(&c)?;
                self.contacts.set_blocked(addr, v);
            }
            CMD_CONTACT_REMOVE => {
                let addr = c.addr()?;
                end(&c)?;
                w.bool(self.contacts.remove(&addr));
            }
            CMD_CONTACT_GET => {
                let addr = c.addr()?;
                end(&c)?;
                match self.contacts.get(&addr) {
                    Some(ct) => {
                        w.bool(true)
                            .bool(ct.following)
                            .bool(ct.blocked)
                            .string(ct.label.as_deref().unwrap_or(""));
                    }
                    // Present as "no row" rather than as a row of defaults: an
                    // address the user has never touched and one they touched and
                    // cleared are different states, and an edit form opened on
                    // the second must not silently discard what is still there.
                    None => {
                        w.bool(false);
                    }
                }
            }
            CMD_CONTACT_ROWS => {
                let view = match c.u8()? {
                    0 => contact::View::Contacts,
                    1 => contact::View::Seen,
                    _ => return None,
                };
                let query = c.string()?;
                // The peers the caller has heard from, as it knows them. Bounded
                // against what is actually left, like every other count here.
                let n = c.u32()? as usize;
                if n > c.remaining() {
                    return None;
                }
                let mut peers: Vec<contact::PeerSeen> = Vec::with_capacity(n.min(1024));
                for _ in 0..n {
                    let addr = c.addr()?;
                    let age_secs = c.u32()?;
                    let has_prekey = c.bool()?;
                    let name = c.string()?;
                    peers.push(contact::PeerSeen {
                        addr,
                        claimed_name: if name.is_empty() { None } else { Some(name) },
                        age_secs,
                        has_prekey,
                    });
                }
                end(&c)?;
                let rows = contact::contact_rows(&peers, &self.contacts, view, &query);
                w.u32(rows.len() as u32);
                for r in rows {
                    w.bytes(&r.addr)
                        .bool(r.name_is_claim)
                        .bool(r.following)
                        .bool(r.blocked)
                        .bool(r.is_contact)
                        .bool(r.heard)
                        .bool(r.has_prekey)
                        .u32(r.age_secs.unwrap_or(0))
                        .bool(r.age_secs.is_some())
                        .string(r.label.as_deref().unwrap_or(""))
                        .string(r.claimed_name.as_deref().unwrap_or(""))
                        .string(r.name.as_deref().unwrap_or(""));
                }
            }

            // -- topics -----------------------------------------------------
            CMD_TOPIC_REMEMBER => {
                let t = c.addr()?;
                let name = c.string()?;
                end(&c)?;
                self.topics.remember(t, &name);
            }
            CMD_TOPIC_FORGET => {
                let t = c.addr()?;
                end(&c)?;
                self.topics.forget(&t);
            }
            CMD_TOPIC_RECEIVE => {
                let t = c.addr()?;
                let from = opt_addr(&mut c)?;
                let body = c.string()?;
                let at = c.u32()?;
                end(&c)?;
                self.topics.receive(t, from, &body, at);
            }
            CMD_TOPIC_POSTS => {
                let t = c.addr()?;
                end(&c)?;
                let posts = self.topics.posts_on(&t);
                w.u32(posts.len() as u32);
                for p in posts {
                    w.opt_addr(&p.from).u32(p.at).string(&p.body);
                }
            }
            CMD_TOPIC_NAMED => {
                end(&c)?;
                let named = self.topics.named();
                w.u32(named.len() as u32);
                for (t, name) in named {
                    w.bytes(&t).string(name);
                }
            }

            // -- drafts -----------------------------------------------------
            CMD_DRAFT_SET => {
                let scope = draft::Scope::from_code(c.u8()?)?;
                let addr = c.addr()?;
                let at = c.u32()?;
                let text = c.string()?;
                end(&c)?;
                self.drafts.set(scope, addr, &text, at);
            }
            CMD_DRAFT_GET => {
                let scope = draft::Scope::from_code(c.u8()?)?;
                let addr = c.addr()?;
                end(&c)?;
                w.string(self.drafts.get(scope, &addr).unwrap_or(""));
            }
            CMD_DRAFT_CLEAR => {
                let scope = draft::Scope::from_code(c.u8()?)?;
                let addr = c.addr()?;
                end(&c)?;
                w.bool(self.drafts.clear(scope, &addr));
            }
            CMD_DRAFT_ALL => {
                end(&c)?;
                let all = self.drafts.all();
                w.u32(all.len() as u32);
                for d in all {
                    w.u8(d.scope.code()).bytes(&d.addr).u32(d.at).string(&d.text);
                }
            }

            // -- persistence ------------------------------------------------
            CMD_SAVE => {
                end(&c)?;
                for part in [
                    self.threads.encode(),
                    self.contacts.encode(),
                    self.topics.encode(),
                    self.drafts.encode(),
                ] {
                    w.u32(part.len() as u32).bytes(&part);
                }
            }
            CMD_LOAD => {
                let threads = c.string_bytes()?;
                let contacts = c.string_bytes()?;
                let topics = c.string_bytes()?;
                // **Trailing sections are optional**, so a blob written before a
                // store existed still loads. Drafts were added fourth, and
                // without this every user with a saved blob would have had it
                // refused — and because the load is all-or-nothing, refused
                // means they lose their threads and contacts too, to gain a
                // feature they did not ask for. A store that is absent is simply
                // empty.
                let drafts = if c.at_end() { None } else { Some(c.string_bytes()?) };
                end(&c)?;
                // **All or nothing.** A partial load would leave one store from
                // this session and the rest from the last, and nothing
                // downstream could tell: a conversation list would render
                // against a contact book that had not caught up. Decode
                // everything, then commit.
                let t = thread::ThreadStore::decode(threads)?;
                let ct = contact::ContactStore::decode(contacts)?;
                let tp = topic::TopicStore::decode(topics)?;
                let dr = match drafts {
                    Some(b) => draft::DraftStore::decode(b)?,
                    None => draft::DraftStore::new(),
                };
                self.threads = t;
                self.contacts = ct;
                self.topics = tp;
                self.drafts = dr;
            }

            _ => return None,
        }
        Some(w.into_vec())
    }
}

/// Every byte consumed, or this is not the command it claims to be.
fn end(c: &Cursor) -> Option<()> {
    c.at_end().then_some(())
}

fn opt_addr(c: &mut Cursor) -> Option<Option<Addr>> {
    match c.u8()? {
        0 => Some(None),
        1 => Some(Some(c.addr()?)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: Addr = [1; 8];
    const B: Addr = [2; 8];

    fn cmd(tag: u8) -> Writer {
        let mut w = Writer::new();
        w.u8(tag);
        w
    }

    fn ok(bytes: &[u8]) -> &[u8] {
        assert_eq!(bytes[0], OK, "expected OK, got tag {}", bytes[0]);
        &bytes[1..]
    }

    #[test]
    fn a_message_can_be_filed_and_read_back_entirely_through_the_abi() {
        // The point of the whole exercise: a host that can send bytes and read
        // bytes needs nothing else.
        let mut c = Communicator::new();

        let mut w = cmd(CMD_THREAD_RECEIVE);
        w.u8(1).bytes(&A).string("hello").bool(true).u32(10);
        let r = c.call(&w.into_vec());
        assert_eq!(ok(&r), &[&[1u8][..], &A[..]].concat()[..], "filed under the sender");

        let mut w = cmd(CMD_THREAD_MESSAGES);
        w.bytes(&A);
        let r = c.call(&w.into_vec());
        let body = ok(&r);
        let mut rd = Cursor::new(body);
        assert_eq!(rd.u32().unwrap(), 1, "one message");
        assert_eq!(rd.u8().unwrap(), 0, "received mail carries no id");
        assert!(!rd.bool().unwrap(), "not self-authored");
        assert!(rd.bool().unwrap(), "sealed");
        assert_eq!(rd.u8().unwrap(), thread::MessageStatus::Received.code());
        assert_eq!(rd.u32().unwrap(), 10);
        assert_eq!(rd.string().unwrap(), "hello");
        assert!(rd.at_end());
    }

    #[test]
    fn an_unauthenticated_sender_files_nothing_and_says_so() {
        let mut c = Communicator::new();
        let mut w = cmd(CMD_THREAD_RECEIVE);
        w.u8(0).string("spoofed").bool(false).u32(10); // no sender
        let r = c.call(&w.into_vec());
        assert_eq!(ok(&r), &[0u8], "no address, because it was filed nowhere");

        let r = c.call(&cmd(CMD_THREAD_CONVERSATIONS).into_vec());
        assert_eq!(Cursor::new(ok(&r)).u32().unwrap(), 0, "and no conversation exists");
    }

    #[test]
    fn every_malformed_command_is_an_error_and_never_a_panic() {
        // The host is a browser tab, an IPC peer or a binder call, and any of
        // them can send nonsense through a bug or a version skew.
        let mut c = Communicator::new();
        assert_eq!(c.call(&[]), vec![ERR_BAD_COMMAND], "empty");
        assert_eq!(c.call(&[0xff]), vec![ERR_BAD_COMMAND], "unknown tag");

        // Every truncation of a valid command.
        let mut w = cmd(CMD_THREAD_SEND);
        w.bytes(&A).bytes(&[9u8; 16]).string("hi").bool(false).u32(10);
        let good = w.into_vec();
        for cut in 0..good.len() {
            assert_eq!(c.call(&good[..cut]), vec![ERR_BAD_COMMAND], "truncated to {cut}");
        }
        // And trailing bytes, which is the other half of the same mistake: a
        // command that decodes and leaves data behind is a different command.
        let mut extra = good.clone();
        extra.push(0);
        assert_eq!(c.call(&extra), vec![ERR_BAD_COMMAND], "trailing");
        assert_eq!(c.call(&good)[0], OK, "and the unmangled one still works");
    }

    #[test]
    fn a_hostile_length_inside_a_command_cannot_allocate() {
        let mut c = Communicator::new();
        let mut w = cmd(CMD_TOPIC_REMEMBER);
        w.bytes(&A).u32(u32::MAX); // a name claiming to be 4 GB
        assert_eq!(c.call(&w.into_vec()), vec![ERR_BAD_COMMAND]);
    }

    #[test]
    fn save_and_load_round_trip_all_three_stores() {
        let mut c = Communicator::new();
        let mut w = cmd(CMD_THREAD_RECEIVE);
        w.u8(1).bytes(&A).string("hi").bool(false).u32(10);
        c.call(&w.into_vec());
        let mut w = cmd(CMD_CONTACT_SET_LABEL);
        w.bytes(&A).string("Ada");
        c.call(&w.into_vec());
        let mut w = cmd(CMD_TOPIC_REMEMBER);
        w.bytes(&crate::topic_of("tides")).string("tides");
        c.call(&w.into_vec());

        let saved = c.call(&cmd(CMD_SAVE).into_vec());
        let blob = ok(&saved).to_vec();

        let mut fresh = Communicator::new();
        let mut w = cmd(CMD_LOAD);
        w.bytes(&blob);
        assert_eq!(fresh.call(&w.into_vec())[0], OK);
        assert_eq!(fresh.threads.messages(&A).len(), 1);
        assert_eq!(fresh.contacts.label_for(&A), Some("Ada"));
        assert_eq!(fresh.topics.name_for(&crate::topic_of("tides")), Some("tides"));
    }

    #[test]
    fn a_blob_written_before_drafts_existed_still_loads() {
        // The migration that would otherwise have cost real users their data.
        // Drafts were added as a fourth section; a blob with three would have
        // been refused, and because the load is all-or-nothing, refused means
        // losing threads and contacts as well — to gain a feature nobody asked
        // for. Trailing sections are therefore optional.
        let mut old = Communicator::new();
        let mut w = cmd(CMD_THREAD_RECEIVE);
        w.u8(1).bytes(&A).string("from last week").bool(false).u32(10);
        old.call(&w.into_vec());
        let mut w = cmd(CMD_CONTACT_SET_LABEL);
        w.bytes(&A).string("Ada");
        old.call(&w.into_vec());

        // A three-section blob, exactly as the previous build wrote them.
        let mut three = Writer::new();
        for part in [old.threads.encode(), old.contacts.encode(), old.topics.encode()] {
            three.u32(part.len() as u32).bytes(&part);
        }

        let mut fresh = Communicator::new();
        let mut w = cmd(CMD_LOAD);
        w.bytes(&three.into_vec());
        assert_eq!(fresh.call(&w.into_vec())[0], OK, "an older blob must still load");
        assert_eq!(fresh.threads.messages(&A).len(), 1, "and bring its threads");
        assert_eq!(fresh.contacts.label_for(&A), Some("Ada"));
        assert!(fresh.drafts.all().is_empty(), "with no drafts, which is simply empty");
    }

    #[test]
    fn drafts_round_trip_through_save_and_load() {
        let mut c = Communicator::new();
        let mut w = cmd(CMD_DRAFT_SET);
        w.u8(draft::Scope::Chat.code()).bytes(&A).u32(10).string("half a thought");
        c.call(&w.into_vec());

        let saved = c.call(&cmd(CMD_SAVE).into_vec());
        let blob = ok(&saved).to_vec();
        let mut fresh = Communicator::new();
        let mut w = cmd(CMD_LOAD);
        w.bytes(&blob);
        assert_eq!(fresh.call(&w.into_vec())[0], OK);

        let mut w = cmd(CMD_DRAFT_GET);
        w.u8(draft::Scope::Chat.code()).bytes(&A);
        let r = fresh.call(&w.into_vec());
        assert_eq!(Cursor::new(ok(&r)).string().unwrap(), "half a thought");
    }

    #[test]
    fn a_draft_for_a_feed_is_not_a_draft_for_a_person() {
        let mut c = Communicator::new();
        let mut w = cmd(CMD_DRAFT_SET);
        w.u8(draft::Scope::Topic.code()).bytes(&A).u32(10).string("to a feed");
        c.call(&w.into_vec());

        let mut w = cmd(CMD_DRAFT_GET);
        w.u8(draft::Scope::Chat.code()).bytes(&A);
        let r = c.call(&w.into_vec());
        assert_eq!(Cursor::new(ok(&r)).string().unwrap(), "", "the same address, the other scope");
    }

    #[test]
    fn a_load_that_fails_leaves_every_store_untouched() {
        // All or nothing: one store from this session and two from the last is a
        // state nothing downstream can detect or recover from.
        let mut c = Communicator::new();
        let mut w = cmd(CMD_CONTACT_SET_LABEL);
        w.bytes(&A).string("Ada");
        c.call(&w.into_vec());

        let mut w = cmd(CMD_LOAD);
        w.u32(1)
            .bytes(&[99]) // a thread blob with an unknown version
            .u32(0)
            .bytes(&[])
            .u32(0)
            .bytes(&[]);
        assert_eq!(c.call(&w.into_vec()), vec![ERR_BAD_COMMAND]);
        assert_eq!(c.contacts.label_for(&A), Some("Ada"), "the good store survived the bad load");
    }

    #[test]
    fn contact_rows_reflect_the_peers_the_command_carried() {
        // Peers travel with the command, so the same store answers differently
        // as the caller's view of the mesh changes — and a test can write that
        // view by hand instead of needing live traffic.
        let mut c = Communicator::new();
        let mut w = cmd(CMD_CONTACT_SET_LABEL);
        w.bytes(&A).string("Ada");
        c.call(&w.into_vec());

        let mut w = cmd(CMD_CONTACT_ROWS);
        w.u8(0).string("").u32(1).bytes(&A).u32(5).bool(true).string("Ada Lovelace");
        let r = c.call(&w.into_vec());
        let mut rd = Cursor::new(ok(&r));
        assert_eq!(rd.u32().unwrap(), 1);
        let _addr = rd.addr().unwrap();
        assert!(!rd.bool().unwrap(), "name_is_claim is false: the user typed a label");
        for _ in 0..5 {
            rd.bool().unwrap();
        }
        rd.u32().unwrap();
        rd.bool().unwrap();
        assert_eq!(rd.string().unwrap(), "Ada");
        assert_eq!(rd.string().unwrap(), "Ada Lovelace", "the claim travels too");
        assert_eq!(rd.string().unwrap(), "Ada");

        // Same command, no peers: the claim is gone because nothing has been
        // heard from them, and a cached snapshot would still be showing it.
        let mut w = cmd(CMD_CONTACT_ROWS);
        w.u8(0).string("").u32(0); // no peers heard from
        let r = c.call(&w.into_vec());
        let mut rd = Cursor::new(ok(&r));
        assert_eq!(rd.u32().unwrap(), 1);
        rd.addr().unwrap();
        for _ in 0..6 {
            rd.bool().unwrap();
        }
        rd.u32().unwrap();
        assert!(!rd.bool().unwrap(), "no age, because nothing was heard");
        assert_eq!(rd.string().unwrap(), "Ada");
        assert_eq!(rd.string().unwrap(), "", "no claim");
    }

    #[test]
    fn a_status_ack_travels_by_id_through_the_abi() {
        let mut c = Communicator::new();
        let id = [7u8; 16];
        let mut w = cmd(CMD_THREAD_SEND);
        w.bytes(&B).bytes(&id).string("out").bool(false).u32(10);
        c.call(&w.into_vec());

        let mut w = cmd(CMD_THREAD_SET_STATUS);
        w.bytes(&id).u8(thread::MessageStatus::Acked.code());
        let r = c.call(&w.into_vec());
        assert_eq!(ok(&r), &[1u8], "it moved");

        let mut w = cmd(CMD_THREAD_SET_STATUS);
        w.bytes(&[0u8; 16]).u8(thread::MessageStatus::Acked.code());
        let r = c.call(&w.into_vec());
        assert_eq!(ok(&r), &[0u8], "an id we never sent moves nothing");
    }

    #[test]
    fn unauthenticated_mail_is_counted_and_reportable() {
        let mut c = Communicator::new();
        let mut w = cmd(CMD_THREAD_RECEIVE);
        w.u8(0).string("spoofed").bool(false).u32(10);
        c.call(&w.into_vec());
        let r = c.call(&cmd(CMD_THREAD_UNAUTHENTICATED).into_vec());
        assert_eq!(Cursor::new(ok(&r)).u32().unwrap(), 1);
    }

    #[test]
    fn getting_one_contact_distinguishes_untouched_from_cleared() {
        let mut c = Communicator::new();
        let r = c.call(&{
            let mut w = cmd(CMD_CONTACT_GET);
            w.bytes(&A);
            w.into_vec()
        });
        assert_eq!(ok(&r), &[0u8], "never touched: no row");

        let mut w = cmd(CMD_CONTACT_SET_BLOCKED);
        w.bytes(&A).bool(true);
        c.call(&w.into_vec());
        let mut w = cmd(CMD_CONTACT_SET_LABEL);
        w.bytes(&A).string("");
        c.call(&w.into_vec());

        let r = c.call(&{
            let mut w = cmd(CMD_CONTACT_GET);
            w.bytes(&A);
            w.into_vec()
        });
        let mut rd = Cursor::new(ok(&r));
        assert!(rd.bool().unwrap(), "cleared label, but the row is still there");
        assert!(!rd.bool().unwrap(), "not following");
        assert!(rd.bool().unwrap(), "still blocked");
        assert_eq!(rd.string().unwrap(), "", "and the label really is empty");
    }

    #[test]
    fn command_tags_are_unique() {
        // These cross a version boundary: a page cached last week talks to a
        // module built today. Two commands sharing a tag would mean one silently
        // performing the other.
        let tags = [
            CMD_THREAD_RECEIVE,
            CMD_THREAD_SEND,
            CMD_THREAD_SET_STATUS,
            CMD_THREAD_MARK_READ,
            CMD_THREAD_MESSAGES,
            CMD_THREAD_CONVERSATIONS,
            CMD_THREAD_TOTAL_UNREAD,
            CMD_THREAD_UNAUTHENTICATED,
            CMD_CONTACT_SET_LABEL,
            CMD_CONTACT_SET_FOLLOWING,
            CMD_CONTACT_SET_BLOCKED,
            CMD_CONTACT_REMOVE,
            CMD_CONTACT_ROWS,
            CMD_CONTACT_GET,
            CMD_TOPIC_REMEMBER,
            CMD_TOPIC_FORGET,
            CMD_TOPIC_RECEIVE,
            CMD_TOPIC_POSTS,
            CMD_TOPIC_NAMED,
            CMD_DRAFT_SET,
            CMD_DRAFT_GET,
            CMD_DRAFT_CLEAR,
            CMD_DRAFT_ALL,
            CMD_SAVE,
            CMD_LOAD,
        ];
        let mut seen = tags.to_vec();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), tags.len(), "two commands share a tag");
    }
}
