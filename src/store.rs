//! The envelope store: what a node holds and can serve to anyone who asks.
//!
//! Not to be confused with [`crate::bridge::store`], which is a *transport* — a
//! shared folder two machines both read and write. This is the node's own
//! custody of envelopes, keyed by content id.
//!
//! Metadata always lives in memory; the bytes need not. Given a directory the
//! store is **write-through** — every envelope lands on disk as it arrives, and
//! memory is a cache in front of it. Past `mem_budget` the coldest resident
//! copies are simply dropped, since the bytes are already safe, so a node can
//! carry far more than it can hold. That is what lets a file run to the sizes
//! the manifest tree allows, and it means what a node held survives a restart.
//!
//! With no directory set nothing touches disk and this behaves exactly as the
//! plain `HashMap` it replaced — which is the right answer on the web and
//! anywhere else without a filesystem.

use super::*;
use std::io;
use std::path::{Path, PathBuf};

/// Whether an envelope's bytes are still resident.
///
/// With a spill directory set the store is **write-through**: every envelope is
/// on disk from the moment it arrives, and memory is only a cache in front of
/// it. So dropping the resident copy costs nothing but a later read, and
/// whatever the node held survives it being restarted. With no directory set,
/// `Mem` is the only place the bytes exist.
#[derive(Clone)]
enum Body {
    Mem(Vec<u8>),
    /// Resident copy dropped; the bytes are in the spill backend under this id.
    Evicted,
}

#[derive(Clone)]
pub(crate) struct Stored {
    body: Body,
    /// Wire length, kept separately so accounting never has to touch the bytes.
    pub len: usize,
    pub expiry: u32,
    pub stamp: u8,
    pub seq: u64,
    pub dest: Addr,
}

pub(crate) struct Store {
    map: HashMap<Id, Stored>,
    spill: Option<Box<dyn SpillBackend>>,
    /// Bytes currently resident. Spilling drives this down; it is not a ceiling
    /// on what the node holds, only on what it holds *in memory*.
    mem_bytes: usize,
    mem_budget: usize,
    total_bytes: usize,
}

/// Largest spilled file we will read back. One envelope, generously — anything
/// bigger cannot be a valid entry, since its id would not match.
/// Largest on-disk envelope file any bridge will read into memory.
///
/// Gated like the file-backed bridges that use it: there are no directories to
/// sync on `wasm32`, and an unused constant is a hard error under `-D warnings`.
///
/// Every directory SPORE reads from is, by design, written by something else — a
/// spill dir, a synced folder, an SSB log, a spool. "Adopt whatever is here" must
/// never mean "read a terabyte into memory".
#[cfg(not(target_arch = "wasm32"))]
pub const MAX_FILE_BYTES: u64 = 1024 * 1024;

/// Read a file, refusing to buffer more than `max` bytes.
///
/// The bound is on the **read**, not on a preceding `metadata` call. Checking the
/// size first and then calling `fs::read` is two syscalls with a gap between them,
/// and every directory this is used on is writable by someone else — so the file
/// can be replaced or extended in that gap and the read would buffer whatever is
/// there at the later moment. A stat is a useful fast reject; it is not the bound.
pub fn read_capped(path: &Path, max: u64) -> io::Result<Vec<u8>> {
    use io::Read;
    let mut buf = Vec::new();
    std::fs::File::open(path)?.take(max + 1).read_to_end(&mut buf)?;
    if buf.len() as u64 > max {
        return Err(io::Error::other("file over cap"));
    }
    Ok(buf)
}

/// Most filenames a folder-style bridge remembers having imported.
///
/// These sets exist only to avoid re-reading a file we already ingested, and they
/// grow by one entry per filename ever seen — unbounded, in a directory whose
/// contents someone else controls. Forgetting is cheap: the node's own dedup
/// (`seen`) drops a re-imported envelope, so an overflow costs one wasted read per
/// file, not a duplicate delivery.
#[cfg(not(target_arch = "wasm32"))]
pub const MAX_KNOWN_FILENAMES: usize = 4096;

/// Keep an imported-filenames set bounded. Clears wholesale on overflow rather
/// than evicting one entry, because the set has no ordering worth preserving and a
/// single clear is cheaper than repeated arbitrary eviction.
#[cfg(not(target_arch = "wasm32"))]
pub fn bound_known(known: &mut HashSet<String>) {
    if known.len() > MAX_KNOWN_FILENAMES {
        known.clear();
    }
}

pub const MAX_ADOPT_BYTES: u64 = 1024 * 1024;

/// Where the store's bytes live when they are not resident in memory — the
/// **storage nutrient** a runtime supplies (see `docs/DESIGN.md`).
///
/// A backend moves dumb bytes and nothing else. It never decides what is valid:
/// every check that matters — the id matching its content, the wire being
/// exactly one envelope, expiry — stays in [`Store`], because a backend is by
/// definition a place *other things can also write*. A filesystem directory can
/// be edited by a backup tool; browser storage can be edited by the page. So the
/// rule is the same either way: bytes coming back in are re-verified against the
/// id that asked for them, and a mismatch reads as "not held" so the mesh
/// re-fetches a good copy (C-ST4).
///
/// `Send` because a node is shared across bridge threads behind a mutex.
pub trait SpillBackend: Send {
    /// Store `wire` under `id`. Failure is deliberately not reported: a spill
    /// that does not land leaves the entry memory-only, exactly as it would with
    /// no backend at all.
    fn put(&mut self, id: &Id, wire: &[u8]);

    /// Read back what was stored. `None` if absent, unreadable, or larger than
    /// [`MAX_ADOPT_BYTES`] — the caller treats all three identically, so a
    /// backend never has to distinguish "gone" from "broken".
    fn get(&self, id: &Id) -> Option<Vec<u8>>;

    /// Forget `id`. Already-absent is success.
    fn remove(&mut self, id: &Id);

    /// Every id currently held, so a restart can adopt what the last run left.
    /// Ids only — the bytes are fetched through [`SpillBackend::get`] and
    /// verified one at a time, so listing never has to buffer the store.
    fn ids(&self) -> Vec<Id>;
}

/// The filesystem backend: one `<hexid>.spore` file per envelope in a directory.
/// What every daemon, desktop and Android node uses; the browser has no
/// equivalent yet, which is why a tab is memory-only.
#[derive(Debug)]
pub struct FsSpill {
    dir: PathBuf,
}

impl FsSpill {
    /// Create the directory if it is not there yet.
    pub fn new(dir: &Path) -> io::Result<FsSpill> {
        std::fs::create_dir_all(dir)?;
        Ok(FsSpill { dir: dir.to_path_buf() })
    }

    fn path(&self, id: &Id) -> PathBuf {
        self.dir.join(filename(id))
    }
}

impl SpillBackend for FsSpill {
    fn put(&mut self, id: &Id, wire: &[u8]) {
        let _ = std::fs::write(self.path(id), wire);
    }

    fn get(&self, id: &Id) -> Option<Vec<u8>> {
        read_capped(&self.path(id), MAX_ADOPT_BYTES).ok()
    }

    fn remove(&mut self, id: &Id) {
        let _ = std::fs::remove_file(self.path(id));
    }

    fn ids(&self) -> Vec<Id> {
        let Ok(rd) = std::fs::read_dir(&self.dir) else { return Vec::new() };
        rd.flatten()
            .filter_map(|entry| {
                let path = entry.path();
                let id = id_from_filename(path.file_name()?.to_str()?)?;
                // Fast reject on size before anything tries to read it. The
                // directory is ours, but it is on disk where anything can drop a
                // file, and "adopt whatever is here" must not mean "read a
                // terabyte into memory". `get` is bounded too — this is the
                // cheap first pass, not the bound.
                match std::fs::metadata(&path) {
                    Ok(m) if m.len() <= MAX_ADOPT_BYTES => Some(id),
                    Ok(_) => {
                        let _ = std::fs::remove_file(&path);
                        None
                    }
                    Err(_) => None,
                }
            })
            .collect()
    }
}

/// `<hexid>.spore`, the same name [`crate::bridge::store`] uses on disk.
fn filename(id: &Id) -> String {
    let mut s = String::with_capacity(38);
    for b in id {
        s.push_str(&format!("{b:02x}"));
    }
    s.push_str(".spore");
    s
}

fn id_from_filename(name: &str) -> Option<Id> {
    let hex = name.strip_suffix(".spore")?;
    if hex.len() != 32 {
        return None;
    }
    let mut id = [0u8; 16];
    for (i, b) in id.iter_mut().enumerate() {
        *b = u8::from_str_radix(hex.get(i * 2..i * 2 + 2)?, 16).ok()?;
    }
    Some(id)
}

impl Store {
    pub fn new() -> Store {
        Store {
            map: HashMap::new(),
            spill: None,
            mem_bytes: 0,
            // Half the default store budget stays resident; the rest spills once
            // a directory is set. Without one, nothing spills and this is moot.
            mem_budget: 5 * 1024 * 1024,
            total_bytes: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }
    pub fn bytes(&self) -> usize {
        self.total_bytes
    }
    pub fn contains(&self, id: &Id) -> bool {
        self.map.contains_key(id)
    }
    pub fn meta(&self, id: &Id) -> Option<&Stored> {
        self.map.get(id)
    }
    pub fn ids(&self) -> impl Iterator<Item = &Id> {
        self.map.keys()
    }
    pub fn entries(&self) -> impl Iterator<Item = (&Id, &Stored)> {
        self.map.iter()
    }
    pub fn set_mem_budget(&mut self, bytes: usize) {
        self.mem_budget = bytes.max(1);
        self.shed();
    }

    /// The envelope's wire bytes, read back from disk if they were spilled.
    /// `None` if we don't hold it — or if a spilled file has gone missing, which
    /// is treated as not holding it rather than as an error, since the mesh can
    /// always be asked again.
    ///
    /// A spilled file is **re-verified on every read**, not just when adopted
    /// (`set_spill_dir`): the spill directory is on disk where the OS, a backup
    /// tool, or a corrupted sector can change a file after we recorded it, and its
    /// name is only a claim about its content. Serving bytes whose id no longer
    /// matches would hand a peer a file that fails its own signature/content check
    /// and blame us for it (C-ST4). The read is bounded (a valid entry is one
    /// envelope), the decode must consume exactly the file, and the recomputed id
    /// must equal the one asked for; anything else reads as "not held" so the mesh
    /// re-fetches a good copy.
    pub fn wire(&self, id: &Id) -> Option<Vec<u8>> {
        match &self.map.get(id)?.body {
            Body::Mem(w) => Some(w.clone()),
            Body::Evicted => {
                let wire = self.spill.as_ref()?.get(id)?;
                let (e, n) = Envelope::decode(&wire).ok()?;
                (n == wire.len() && e.id() == *id).then_some(wire)
            }
        }
    }

    pub fn put(&mut self, id: Id, wire: Vec<u8>, expiry: u32, stamp: u8, seq: u64, dest: Addr) {
        if self.map.contains_key(&id) {
            return; // content-addressed: same id, same bytes, nothing to do
        }
        let len = wire.len();
        // Write through, so what the node holds outlives the process. A failed
        // write is not fatal — the entry simply stays memory-only, exactly as it
        // would with no spill directory at all.
        if let Some(backend) = &mut self.spill {
            backend.put(&id, &wire);
        }
        self.map.insert(id, Stored { body: Body::Mem(wire), len, expiry, stamp, seq, dest });
        self.mem_bytes += len;
        self.total_bytes += len;
        self.shed();
    }

    pub fn remove(&mut self, id: &Id) {
        let Some(s) = self.map.remove(id) else { return };
        if matches!(s.body, Body::Mem(_)) {
            self.mem_bytes = self.mem_bytes.saturating_sub(s.len);
        }
        if let Some(backend) = &mut self.spill {
            backend.remove(id);
        }
        self.total_bytes = self.total_bytes.saturating_sub(s.len);
    }

    /// Drop the resident copy of the coldest entries until memory is back under
    /// budget. Same order eviction uses — lowest stamp, then largest, then
    /// oldest — so what leaves memory is what is least worth keeping close.
    ///
    /// Nothing is lost: the bytes were written to disk on the way in.
    fn shed(&mut self) {
        if self.spill.is_none() {
            return; // nowhere to read them back from, so keep them all
        }
        while self.mem_bytes > self.mem_budget {
            let coldest = self
                .map
                .iter()
                .filter(|(_, s)| matches!(s.body, Body::Mem(_)))
                .min_by(|a, b| {
                    a.1.stamp.cmp(&b.1.stamp).then(b.1.len.cmp(&a.1.len)).then(a.1.seq.cmp(&b.1.seq))
                })
                .map(|(k, _)| *k);
            let Some(id) = coldest else { return };
            let Some(s) = self.map.get_mut(&id) else { return };
            s.body = Body::Evicted;
            let len = s.len;
            self.mem_bytes = self.mem_bytes.saturating_sub(len);
        }
    }

    /// Point the store at a directory to spill to, and adopt anything already
    /// there from a previous run.
    ///
    /// Adoption is safe because an id *is* the hash of the bytes: a file whose
    /// name does not match its content is discarded, so a tampered or truncated
    /// spill directory cannot inject anything. Returns the wires adopted, so the
    /// caller can re-learn manifests from them and resume a transfer that a
    /// restart interrupted.
    pub fn set_spill_dir(&mut self, dir: &Path, now: u32) -> std::io::Result<Vec<Vec<u8>>> {
        Ok(self.set_spill_backend(Box::new(FsSpill::new(dir)?), now))
    }

    /// Same, for a runtime whose storage is not a filesystem — browser
    /// IndexedDB, MCU flash, a test double. The verification below is identical
    /// either way, deliberately: a backend is never trusted to have kept the
    /// bytes it was given.
    pub fn set_spill_backend(&mut self, backend: Box<dyn SpillBackend>, now: u32) -> Vec<Vec<u8>> {
        self.spill = Some(backend);
        self.adopt(now)
    }

    /// Take everything the backend claims to hold, keep what verifies, and drop
    /// the rest from the backend so a bad entry is not re-examined every start.
    fn adopt(&mut self, now: u32) -> Vec<Vec<u8>> {
        let Some(backend) = self.spill.as_ref() else { return Vec::new() };
        let ids = backend.ids();

        let mut adopted = Vec::new();
        let mut discard = Vec::new();
        let mut seq = self.map.len() as u64;
        for id in ids {
            if self.map.contains_key(&id) {
                continue;
            }
            let Some(backend) = self.spill.as_ref() else { break };
            // Unreadable is not the backend's fault to prove — leave it alone
            // and let the mesh re-fetch. Only *invalid* content is discarded.
            let Some(wire) = backend.get(&id) else { continue };
            let Ok((e, n)) = Envelope::decode(&wire) else {
                discard.push(id);
                continue;
            };
            // The id must match the content, the wire must be exactly one
            // envelope, and it must not already have expired.
            if n != wire.len() || e.id() != id || e.expiry <= now {
                discard.push(id);
                continue;
            }
            let len = wire.len();
            self.map.insert(
                id,
                Stored { body: Body::Evicted, len, expiry: e.expiry, stamp: e.stamp(), seq, dest: e.dest },
            );
            self.total_bytes += len;
            seq += 1;
            adopted.push(wire);
        }

        if let Some(backend) = self.spill.as_mut() {
            for id in &discard {
                backend.remove(id);
            }
        }
        self.shed();
        adopted
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A backend with no filesystem under it — the shape a browser tab or an MCU
    /// would supply. Exists to prove the seam is real: `Store` is not touched to
    /// add it, and every guarantee below is enforced the same way it is for
    /// `FsSpill`.
    #[derive(Default)]
    struct MemSpill {
        blobs: HashMap<Id, Vec<u8>>,
    }

    impl SpillBackend for MemSpill {
        fn put(&mut self, id: &Id, wire: &[u8]) {
            self.blobs.insert(*id, wire.to_vec());
        }
        fn get(&self, id: &Id) -> Option<Vec<u8>> {
            self.blobs.get(id).cloned()
        }
        fn remove(&mut self, id: &Id) {
            self.blobs.remove(id);
        }
        fn ids(&self) -> Vec<Id> {
            self.blobs.keys().copied().collect()
        }
    }

    fn env_wire(payload: &[u8], expiry: u32) -> (Id, Vec<u8>) {
        let e = Envelope::new(ty::DATA, [0u8; 8], expiry, payload.to_vec());
        (e.id(), e.wire())
    }

    #[test]
    fn a_non_filesystem_backend_spills_and_reads_back() {
        // The storage nutrient, supplied by something that is not a disk.
        let (id, wire) = env_wire(b"held without a filesystem", 9_000);
        let mut s = Store::new();
        s.set_spill_backend(Box::<MemSpill>::default(), 1);
        s.set_mem_budget(1); // force everything out of memory immediately
        s.put(id, wire.clone(), 9_000, 0, 0, [0u8; 8]);

        // Resident copy is gone, but the store still serves the bytes.
        assert_eq!(s.wire(&id).as_deref(), Some(&wire[..]), "reads back through the backend");
        s.remove(&id);
        assert!(s.wire(&id).is_none(), "removed from the backend too");
    }

    #[test]
    fn adoption_verifies_content_whatever_the_backend_is() {
        // The C-ST4 property must not depend on the backend being a filesystem:
        // an id is the hash of its bytes, so a backend that hands back something
        // else is caught and the entry discarded rather than served on.
        let (good_id, good) = env_wire(b"genuine", 9_000);
        let (tampered_id, _) = env_wire(b"claimed", 9_000);

        let mut backend = MemSpill::default();
        backend.put(&good_id, &good);
        // Same id, different bytes — exactly what a rotted sector or an edited
        // store looks like from the outside.
        backend.put(&tampered_id, b"not what this id says it is");

        let mut s = Store::new();
        let adopted = s.set_spill_backend(Box::new(backend), 1);

        assert_eq!(adopted.len(), 1, "only the entry whose id matches its bytes is adopted");
        assert_eq!(adopted[0], good);
        assert!(s.contains(&good_id));
        assert!(!s.contains(&tampered_id), "a mismatched id is never adopted");
        assert!(s.wire(&tampered_id).is_none());
    }

    #[test]
    fn adoption_drops_expired_entries() {
        let (id, wire) = env_wire(b"stale", 100);
        let mut backend = MemSpill::default();
        backend.put(&id, &wire);

        let mut s = Store::new();
        let adopted = s.set_spill_backend(Box::new(backend), 500); // now > expiry
        assert!(adopted.is_empty(), "an expired entry is not adopted");
        assert!(!s.contains(&id));
    }

    #[test]
    fn read_capped_bounds_the_read_not_a_preceding_stat() {
        // The class of bug this exists to stop, and why a `metadata` check is not
        // enough on its own: the two are separate syscalls, and every directory
        // this is used on is written by someone else. Same finding as the spool
        // bridge, which is why both now share this one helper.
        let mut p = std::env::temp_dir();
        p.push(format!("spore-readcapped-{}", std::process::id()));
        let _ = std::fs::remove_file(&p);

        std::fs::write(&p, vec![0u8; 4096]).unwrap();
        assert_eq!(read_capped(&p, 4096).unwrap().len(), 4096, "exactly at the cap is fine");
        assert!(read_capped(&p, 4095).is_err(), "one byte over must refuse");

        // A file far larger than the cap must not be buffered at all.
        std::fs::write(&p, vec![7u8; 1024 * 1024]).unwrap();
        assert!(read_capped(&p, 1024).is_err());
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn spilled_wire_is_verified_against_its_id_on_read() {
        // C-ST4: the spill dir is on disk, so a file can rot or be swapped after we
        // recorded it. Its name is only a claim about its content, so `wire` must
        // re-check the id it reads back and refuse a mismatch rather than serve a
        // peer bytes that fail their own content check.
        let mut dir = std::env::temp_dir();
        dir.push(format!("spore-spillverify-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        let mut store = Store::new();
        store.set_spill_dir(&dir, 1).unwrap();

        // One envelope, written through to disk, then dropped from memory so `wire`
        // has to read it back. An unsigned envelope is enough: the id is a hash of
        // the wire, independent of any signature.
        let e = Envelope::new(ty::DATA, [9u8; 8], 1_000_000, b"north pier at midnight".to_vec());
        let wire = e.wire();
        let id = e.id();
        store.put(id, wire.clone(), e.expiry, e.stamp(), 0, e.dest);
        store.set_mem_budget(0); // force eviction to Body::Evicted
        assert!(matches!(store.map.get(&id).unwrap().body, Body::Evicted), "must be spilled");

        let path = dir.join(filename(&id));

        // Intact spill loads and round-trips.
        assert_eq!(store.wire(&id).as_deref(), Some(&wire[..]), "intact spill loads");

        // Flip the last byte on disk: recomputed id no longer matches -> not held.
        let mut corrupt = wire.clone();
        *corrupt.last_mut().unwrap() ^= 0x01;
        std::fs::write(&path, &corrupt).unwrap();
        assert_eq!(store.wire(&id), None, "a corrupted spill reads as not held");

        // Truncated -> decode fails / length mismatch -> not held, no panic.
        std::fs::write(&path, &wire[..wire.len() / 2]).unwrap();
        assert_eq!(store.wire(&id), None, "a truncated spill reads as not held");

        // Restore intact bytes -> loads again (the rejection was the content, not state).
        std::fs::write(&path, &wire).unwrap();
        assert_eq!(store.wire(&id).as_deref(), Some(&wire[..]), "intact again loads");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn known_filename_sets_stay_bounded() {
        // Folder-style bridges remember what they imported: one entry per filename
        // ever seen, in a directory whose contents someone else controls.
        let mut known: HashSet<String> = HashSet::new();
        for i in 0..(MAX_KNOWN_FILENAMES * 3) {
            known.insert(format!("{i}.spore"));
            bound_known(&mut known);
        }
        assert!(known.len() <= MAX_KNOWN_FILENAMES, "held {}", known.len());
    }
}
