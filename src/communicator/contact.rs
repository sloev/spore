//! **Local labels for addresses** (M10-B, closing G2).
//!
//! The Rust port of `web/app/stores/contacts.mjs`, and the odd one of the six
//! stores: it holds **no SPORE data at all**. Every field here is something this
//! user decided locally, and none of it is ever merged with anything
//! authenticated.
//!
//! # Why the separation is the whole point
//!
//! It mirrors the kernel's own reasoning at [`crate::Node::peer_name`]: the name
//! in a peer's ANNOUNCE is what they *claim* to be called, and anyone may
//! announce any name. Ada's address announcing "Grace Hopper" is not a bug, it is
//! Tuesday.
//!
//! So a claimed name is a display hint offered as a default, never identity, and
//! never written into this store as though the user had chosen it.
//! [`ContactStore::label_for`] returns **only what the user typed** and
//! deliberately does not fall back to a claim — a fallback there would let a
//! claim silently become "the contact's name" everywhere, which is the exact
//! confusion this store exists to prevent.
//!
//! Joining the two is [`contact_rows`]'s job, and it returns both halves plus
//! [`ContactRow::name_is_claim`] so a screen can mark an unauthenticated name as
//! such rather than presenting it as established. A row with neither falls back
//! to the address, which is the only thing here that was ever proved.

use crate::Addr;

/// One address the user has touched. Local state only.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Contact {
    pub addr: Addr,
    /// What this user typed. `None` when they never typed one, or cleared it.
    pub label: Option<String>,
    pub following: bool,
    pub blocked: bool,
}

impl Contact {
    fn new(addr: Addr) -> Contact {
        Contact { addr, label: None, following: false, blocked: false }
    }
}

/// What the node has heard, as the host reports it.
///
/// Built from [`crate::Node::peers`] and [`crate::Node::peer_name`] — see
/// [`ContactStore::seen_from_node`]. A plain struct rather than a borrow of the
/// node so the row-building below stays testable without one, and so a host that
/// keeps its own peer table (Android does) can supply that instead.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PeerSeen {
    pub addr: Addr,
    /// What the peer claims to be called. Never identity.
    pub claimed_name: Option<String>,
    pub age_secs: u32,
    /// Whether we hold their prekey — what makes a sealed message possible.
    pub has_prekey: bool,
}

/// Which list a screen is showing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    /// What the user deliberately kept.
    Contacts,
    /// Everyone the node has heard from who is not yet a contact. A genuinely
    /// different list rather than a filter of the same one.
    Seen,
}

/// A row a screen can render, carrying the local label and the claim separately.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContactRow {
    pub addr: Addr,
    pub label: Option<String>,
    pub claimed_name: Option<String>,
    /// What to show: the label if there is one, else the claim, else nothing.
    /// `None` means the screen formats the address — the row never invents a name.
    pub name: Option<String>,
    /// True when `name` came from a claim rather than from the user. A screen
    /// that ignores this field is presenting an unauthenticated string as fact.
    pub name_is_claim: bool,
    pub following: bool,
    pub blocked: bool,
    pub is_contact: bool,
    pub heard: bool,
    pub age_secs: Option<u32>,
    pub has_prekey: bool,
}

/// Local labels, follows and blocks.
#[derive(Default)]
pub struct ContactStore {
    /// A `Vec` rather than a map, for the same reason the thread store uses one:
    /// the encoding has to be deterministic, and `HashMap` iteration order does
    /// not promise that.
    by_addr: Vec<Contact>,
}

impl ContactStore {
    pub fn new() -> ContactStore {
        ContactStore::default()
    }

    // ---------------------------------------------------------------- writes

    /// Set the local label. An empty or whitespace-only label **removes the
    /// label and keeps the row**, because the row also carries follow and block
    /// state that clearing a name must not throw away.
    pub fn set_label(&mut self, addr: Addr, label: &str) -> &Contact {
        let trimmed = label.trim();
        let c = self.entry(addr);
        c.label = if trimmed.is_empty() { None } else { Some(trimmed.to_string()) };
        c
    }

    pub fn set_following(&mut self, addr: Addr, following: bool) -> &Contact {
        let c = self.entry(addr);
        c.following = following;
        c
    }

    pub fn set_blocked(&mut self, addr: Addr, blocked: bool) -> &Contact {
        let c = self.entry(addr);
        c.blocked = blocked;
        c
    }

    /// Forget an address entirely. Returns whether there was anything to forget.
    pub fn remove(&mut self, addr: &Addr) -> bool {
        match self.by_addr.iter().position(|c| c.addr == *addr) {
            Some(i) => {
                self.by_addr.remove(i);
                true
            }
            None => false,
        }
    }

    fn entry(&mut self, addr: Addr) -> &mut Contact {
        match self.by_addr.iter().position(|c| c.addr == addr) {
            Some(i) => &mut self.by_addr[i],
            None => {
                self.by_addr.push(Contact::new(addr));
                self.by_addr.last_mut().expect("just pushed")
            }
        }
    }

    // ----------------------------------------------------------------- reads

    pub fn get(&self, addr: &Addr) -> Option<&Contact> {
        self.by_addr.iter().find(|c| c.addr == *addr)
    }

    /// The label this user gave the address, or `None`.
    ///
    /// Deliberately does **not** fall back to an announced name. See the module
    /// docs: that fallback is the confusion this store exists to prevent.
    pub fn label_for(&self, addr: &Addr) -> Option<&str> {
        self.get(addr).and_then(|c| c.label.as_deref())
    }

    pub fn is_blocked(&self, addr: &Addr) -> bool {
        self.get(addr).is_some_and(|c| c.blocked)
    }

    pub fn is_following(&self, addr: &Addr) -> bool {
        self.get(addr).is_some_and(|c| c.following)
    }

    /// Every address the user has touched, labelled first then by sort key.
    pub fn all(&self) -> Vec<&Contact> {
        let mut v: Vec<&Contact> = self.by_addr.iter().collect();
        v.sort_by(|a, b| {
            labelled_first(a.label.is_some(), b.label.is_some()).then_with(|| {
                sort_key(a.label.as_deref(), &a.addr).cmp(&sort_key(b.label.as_deref(), &b.addr))
            })
        });
        v
    }

    /// Addresses the user follows — the Blogs screen's subscription list.
    pub fn following(&self) -> Vec<&Contact> {
        self.all().into_iter().filter(|c| c.following).collect()
    }

    /// Build [`PeerSeen`] rows from a node's own peer table.
    ///
    /// The convenience path: a host with a `Node` should not have to join
    /// `peers()` and `peer_name()` by hand and risk doing it differently on each
    /// platform, which is the duplication M10 exists to end.
    pub fn seen_from_node(node: &crate::Node, now: u32) -> Vec<PeerSeen> {
        node.peers(now)
            .into_iter()
            .map(|(addr, age_secs, has_prekey)| PeerSeen {
                claimed_name: node.peer_name(&addr).map(str::to_string),
                addr,
                age_secs,
                has_prekey,
            })
            .collect()
    }

    // ----------------------------------------------------------- persistence

    /// Encode for the host's storage port. Same shape and reasoning as the
    /// thread store's: hand-rolled, length-prefixed, versioned, deterministic.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = vec![ENCODING_VERSION];
        out.extend_from_slice(&(self.by_addr.len() as u32).to_be_bytes());
        for c in &self.by_addr {
            out.extend_from_slice(&c.addr);
            out.push(u8::from(c.following));
            out.push(u8::from(c.blocked));
            let label = c.label.as_deref().unwrap_or("");
            out.extend_from_slice(&(label.len() as u32).to_be_bytes());
            out.extend_from_slice(label.as_bytes());
        }
        out
    }

    /// Decode what [`ContactStore::encode`] wrote. `None` for anything
    /// malformed, truncated, or from an unknown version.
    ///
    /// The caller starts empty and **leaves the stored blob alone**. These are
    /// labels the user typed and nothing else holds a copy: wiping them because
    /// a parse failed would destroy the only data in this module a person
    /// actually authored.
    pub fn decode(bytes: &[u8]) -> Option<ContactStore> {
        let mut c = super::Cursor::new(bytes);
        if c.u8()? != ENCODING_VERSION {
            return None;
        }
        let n = c.u32()? as usize;
        // Checked against what is left rather than believed: host storage is
        // neither signed nor verified.
        if n > c.remaining() {
            return None;
        }
        let mut s = ContactStore::new();
        for _ in 0..n {
            let addr = c.addr()?;
            let following = c.bool()?;
            let blocked = c.bool()?;
            let len = c.u32()? as usize;
            let label = String::from_utf8(c.take(len)?.to_vec()).ok()?;
            // An empty stored label is `None`, so a round trip cannot turn "no
            // label" into `Some("")` — which would sort as labelled and render
            // as a blank name.
            let label = if label.trim().is_empty() { None } else { Some(label) };
            s.by_addr.push(Contact { addr, label, following, blocked });
        }
        if !c.at_end() {
            return None;
        }
        Some(s)
    }
}

const ENCODING_VERSION: u8 = 1;

/// Labelled contacts sort before unlabelled ones.
fn labelled_first(a: bool, b: bool) -> std::cmp::Ordering {
    b.cmp(&a)
}

/// What a row sorts under: the label if there is one, else the address in hex.
///
/// **A known divergence from the JS**, written down rather than smoothed over:
/// `contacts.mjs` sorts with `localeCompare`, which is locale-aware collation —
/// in a Danish locale "Ærø" sorts after "Zeta", and in an English one it does
/// not. Rust's `str` ordering is by Unicode scalar value, so the two agree on
/// ASCII and can disagree on anything else.
///
/// Matching `localeCompare` would need an ICU-grade collation table, and rule 3
/// is no new dependencies — a crate that has to build on Espressif's Xtensa fork
/// is not where a megabyte of locale data belongs. The alternative, sorting
/// names by codepoint, is what almost every embedded address book does.
///
/// It is a real difference and a user with non-ASCII labels may notice their
/// list reorder once. Sorting is presentational and nothing is lost, which is
/// why this is a documented divergence rather than a blocker — but if a host
/// wants locale order it should sort [`ContactStore::all`]'s output itself,
/// where it already knows the user's locale and this module never can.
fn sort_key(label: Option<&str>, addr: &Addr) -> String {
    match label {
        Some(l) => l.to_lowercase(),
        None => hex(addr),
    }
}

fn hex(a: &Addr) -> String {
    a.iter().map(|b| format!("{b:02x}")).collect()
}

/// Join local labels against what peers claim, for a list the user can read.
///
/// `query` matches the address hex, the label, or the claimed name, all
/// case-insensitively — a user searching for someone will type whichever of the
/// three they remember.
pub fn contact_rows(peers: &[PeerSeen], contacts: &ContactStore, view: View, query: &str) -> Vec<ContactRow> {
    let mut addrs: Vec<Addr> = contacts.by_addr.iter().map(|c| c.addr).collect();
    for p in peers {
        if !addrs.contains(&p.addr) {
            addrs.push(p.addr);
        }
    }

    let mut rows: Vec<ContactRow> = addrs
        .into_iter()
        .map(|addr| {
            let c = contacts.get(&addr);
            let p = peers.iter().find(|p| p.addr == addr);
            let label = c.and_then(|c| c.label.clone());
            let claimed_name = p.and_then(|p| p.claimed_name.clone()).filter(|s| !s.is_empty());
            let name_is_claim = label.is_none() && claimed_name.is_some();
            ContactRow {
                addr,
                name: label.clone().or_else(|| claimed_name.clone()),
                label,
                claimed_name,
                name_is_claim,
                following: c.is_some_and(|c| c.following),
                blocked: c.is_some_and(|c| c.blocked),
                is_contact: c.is_some(),
                heard: p.is_some(),
                age_secs: p.map(|p| p.age_secs),
                has_prekey: p.is_some_and(|p| p.has_prekey),
            }
        })
        .filter(|r| match view {
            View::Contacts => r.is_contact,
            View::Seen => r.heard && !r.is_contact,
        })
        .collect();

    let q = query.trim().to_lowercase();
    if !q.is_empty() {
        rows.retain(|r| {
            hex(&r.addr).contains(&q)
                || r.label.as_deref().is_some_and(|l| l.to_lowercase().contains(&q))
                || r.claimed_name.as_deref().is_some_and(|n| n.to_lowercase().contains(&q))
        });
    }

    match view {
        // Freshest first: in the "seen" list, recency is the only ordering that
        // means anything, since none of these have a name the user chose.
        View::Seen => rows.sort_by_key(|r| r.age_secs.unwrap_or(0)),
        View::Contacts => rows.sort_by(|a, b| {
            labelled_first(a.label.is_some(), b.label.is_some())
                .then_with(|| sort_key(a.name.as_deref(), &a.addr).cmp(&sort_key(b.name.as_deref(), &b.addr)))
        }),
    }
    rows
}

#[cfg(test)]
mod tests {
    //! Mirrors `web/app/stores/contacts.test.mjs` case for case, for the same
    //! reason the thread store's tests do: "unchanged behaviour behind an
    //! unchanged interface" is only worth something if CI checks it.
    use super::*;

    const ADA: Addr = [0xa0; 8];
    const RAE: Addr = [0xb0; 8];
    const JO: Addr = [0x10; 8];

    fn seen(addr: Addr, name: Option<&str>, age: u32, prekey: bool) -> PeerSeen {
        PeerSeen { addr, claimed_name: name.map(str::to_string), age_secs: age, has_prekey: prekey }
    }

    #[test]
    fn a_claimed_name_is_never_a_label() {
        let mut s = ContactStore::new();
        s.set_following(ADA, true); // the address is known to us, with no label
        assert_eq!(s.label_for(&ADA), None, "no label was typed, so there is none");

        let peers = vec![seen(ADA, Some("Ada Lovelace"), 5, true)];
        let rows = contact_rows(&peers, &s, View::Contacts, "");
        assert_eq!(s.label_for(&ADA), None);
        assert_eq!(rows[0].name.as_deref(), Some("Ada Lovelace"));
        assert!(rows[0].name_is_claim, "and it must be flagged as a claim");
    }

    #[test]
    fn a_label_wins_over_a_claim_but_the_claim_stays_visible() {
        let mut s = ContactStore::new();
        s.set_label(ADA, "Ada (work)");
        let peers = vec![seen(ADA, Some("Ada Lovelace"), 5, true)];
        let row = &contact_rows(&peers, &s, View::Contacts, "")[0];
        assert_eq!(row.name.as_deref(), Some("Ada (work)"));
        assert_eq!(row.claimed_name.as_deref(), Some("Ada Lovelace"), "the claim is still there for context");
        assert!(!row.name_is_claim);
    }

    #[test]
    fn a_row_with_neither_invents_nothing() {
        let mut s = ContactStore::new();
        s.set_following(JO, true);
        let row = &contact_rows(&[], &s, View::Contacts, "")[0];
        assert_eq!(row.name, None, "the screen formats the address; the row does not invent a name");
        assert_eq!(row.addr, JO);
        assert!(!row.heard);
        assert!(!row.has_prekey, "so nothing to them can be sealed yet");
    }

    #[test]
    fn clearing_a_label_keeps_the_row_and_everything_else_on_it() {
        let mut s = ContactStore::new();
        s.set_label(ADA, "Ada");
        s.set_blocked(ADA, true);
        s.set_label(ADA, "   "); // whitespace only: the same as clearing it
        assert_eq!(s.label_for(&ADA), None);
        assert!(s.is_blocked(&ADA), "blocking must survive clearing the label");
        assert!(s.get(&ADA).is_some(), "and the row itself must survive");
    }

    #[test]
    fn follow_and_block_are_independent() {
        let mut s = ContactStore::new();
        s.set_following(ADA, true);
        s.set_blocked(ADA, true);
        assert!(s.is_following(&ADA));
        assert!(s.is_blocked(&ADA));
        s.set_blocked(ADA, false);
        assert!(s.is_following(&ADA), "unblocking is not unfollowing");
    }

    #[test]
    fn following_lists_only_the_followed() {
        let mut s = ContactStore::new();
        s.set_following(ADA, true);
        s.set_label(RAE, "Rae");
        assert_eq!(s.following().iter().map(|c| c.addr).collect::<Vec<_>>(), vec![ADA]);
    }

    #[test]
    fn contacts_and_seen_are_different_lists_not_a_filter_of_one() {
        let mut s = ContactStore::new();
        s.set_label(ADA, "Ada");
        let peers = vec![seen(ADA, None, 5, true), seen(RAE, Some("Rae"), 9, false)];
        let contacts: Vec<Addr> =
            contact_rows(&peers, &s, View::Contacts, "").iter().map(|r| r.addr).collect();
        let seen_rows: Vec<Addr> = contact_rows(&peers, &s, View::Seen, "").iter().map(|r| r.addr).collect();
        assert_eq!(contacts, vec![ADA], "only what the user kept");
        assert_eq!(seen_rows, vec![RAE], "heard from, but not yet a contact");
    }

    #[test]
    fn the_seen_list_is_freshest_first() {
        let s = ContactStore::new();
        let peers = vec![seen(ADA, None, 90, false), seen(RAE, None, 3, false)];
        let order: Vec<Addr> = contact_rows(&peers, &s, View::Seen, "").iter().map(|r| r.addr).collect();
        assert_eq!(order, vec![RAE, ADA]);
    }

    #[test]
    fn search_matches_a_label_a_claim_or_an_address() {
        let mut s = ContactStore::new();
        s.set_label(ADA, "Ada");
        s.set_following(RAE, true);
        let peers = vec![seen(RAE, Some("Rae Dawn"), 5, false)];

        let find = |q: &str| -> Vec<Addr> {
            contact_rows(&peers, &s, View::Contacts, q).iter().map(|r| r.addr).collect()
        };
        assert_eq!(find("ada"), vec![ADA], "by label, case-insensitively");
        assert_eq!(find("dawn"), vec![RAE], "by claimed name");
        assert_eq!(find(&hex(&RAE)[..4]), vec![RAE], "by address");
        assert!(find("nobody").is_empty());
    }

    #[test]
    fn labelled_contacts_sort_before_unlabelled_ones() {
        let mut s = ContactStore::new();
        s.set_following(RAE, true); // no label
        s.set_label(ADA, "zzz last alphabetically");
        let order: Vec<Addr> = s.all().iter().map(|c| c.addr).collect();
        assert_eq!(order, vec![ADA, RAE], "a name the user chose outranks one they did not");
    }

    #[test]
    fn non_ascii_labels_sort_by_codepoint_and_that_is_a_known_divergence() {
        // The JS sorts with `localeCompare`. This sorts by Unicode scalar value
        // after lowercasing. They agree on ASCII and can disagree past it.
        //
        // German is the clean example: `localeCompare` there treats "ä" as "a",
        // so "Ärger" sorts *before* "Zeta". Here 'ä' is U+00E4 and 'z' is
        // U+007A, so it sorts after. (Danish is not a counterexample — it puts
        // Æ after Z too, so both orders agree there, which is exactly why the
        // divergence is easy to miss by testing one language.)
        //
        // Asserted rather than left in a doc comment, so the divergence is a
        // fact CI knows about. Matching `localeCompare` needs ICU-grade
        // collation data, which rule 3 forbids and an Xtensa build would not
        // thank us for. A host that wants locale order sorts `all()` itself,
        // where it knows the user's locale and this module never can.
        let mut s = ContactStore::new();
        s.set_label(ADA, "Ärger");
        s.set_label(RAE, "Zeta");
        let order: Vec<&str> = s.all().iter().filter_map(|c| c.label.as_deref()).collect();
        assert_eq!(order, vec!["Zeta", "Ärger"], "codepoint order; German collation gives the reverse");
    }

    #[test]
    fn a_round_trip_through_storage_preserves_everything() {
        let mut s = ContactStore::new();
        s.set_label(ADA, "Ada");
        s.set_blocked(RAE, true);
        s.set_following(JO, true);

        let b = ContactStore::decode(&s.encode()).expect("its own output must decode");
        assert_eq!(b.label_for(&ADA), Some("Ada"));
        assert!(b.is_blocked(&RAE));
        assert!(b.is_following(&JO));
        assert_eq!(b.label_for(&RAE), None);
        assert_eq!(b.all().len(), 3);
    }

    #[test]
    fn an_empty_stored_label_comes_back_as_no_label_not_as_a_blank_one() {
        // `Some("")` would sort as labelled and render as a blank name — a row
        // the user cannot identify and cannot explain.
        let mut s = ContactStore::new();
        s.set_following(ADA, true);
        let b = ContactStore::decode(&s.encode()).unwrap();
        assert_eq!(b.get(&ADA).unwrap().label, None);
    }

    #[test]
    fn the_encoding_is_deterministic() {
        let build = || {
            let mut s = ContactStore::new();
            s.set_label(ADA, "Ada");
            s.set_blocked(RAE, true);
            s
        };
        assert_eq!(build().encode(), build().encode());
    }

    #[test]
    fn a_corrupt_blob_decodes_to_nothing_rather_than_panicking() {
        assert!(ContactStore::decode(b"").is_none(), "empty");
        assert!(ContactStore::decode(b"not json at all").is_none(), "the JS store's format");
        assert!(ContactStore::decode(&[99, 0, 0, 0, 0]).is_none(), "unknown version");

        let mut s = ContactStore::new();
        s.set_label(ADA, "Ada");
        s.set_blocked(RAE, true);
        let good = s.encode();
        for cut in 0..good.len() {
            assert!(ContactStore::decode(&good[..cut]).is_none(), "truncated to {cut} decoded anyway");
        }
        let mut extra = good.clone();
        extra.push(0);
        assert!(ContactStore::decode(&extra).is_none(), "trailing bytes must be refused");
        assert!(ContactStore::decode(&good).is_some());
    }

    #[test]
    fn a_hostile_count_cannot_make_the_decoder_allocate() {
        let mut b = vec![ENCODING_VERSION];
        b.extend_from_slice(&u32::MAX.to_be_bytes());
        assert!(ContactStore::decode(&b).is_none());
    }

    #[test]
    fn removing_forgets_the_address_entirely() {
        let mut s = ContactStore::new();
        s.set_label(ADA, "Ada");
        assert!(s.remove(&ADA));
        assert_eq!(s.get(&ADA), None);
        assert!(!s.remove(&ADA), "and removing again says so");
    }

    #[test]
    fn peer_rows_come_from_the_node_rather_than_from_each_host_separately() {
        // The join of `peers()` and `peer_name()` lives here once, which is the
        // duplication M10 exists to end — three hosts doing it three ways is how
        // one of them ends up treating a claim as a name.
        let node = crate::Node::new("local", &[]);
        let rows = ContactStore::seen_from_node(&node, 1_000);
        assert!(rows.is_empty(), "a node that has heard from nobody reports nobody");
    }
}
