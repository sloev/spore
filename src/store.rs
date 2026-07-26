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
    /// Resident copy dropped; the bytes are at `<dir>/<hexid>.spore`.
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
    spill: Option<PathBuf>,
    /// Bytes currently resident. Spilling drives this down; it is not a ceiling
    /// on what the node holds, only on what it holds *in memory*.
    mem_bytes: usize,
    mem_budget: usize,
    total_bytes: usize,
}

/// Largest spilled file we will read back. One envelope, generously — anything
/// bigger cannot be a valid entry, since its id would not match.
const MAX_ADOPT_BYTES: u64 = 1024 * 1024;

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
    pub fn wire(&self, id: &Id) -> Option<Vec<u8>> {
        match &self.map.get(id)?.body {
            Body::Mem(w) => Some(w.clone()),
            Body::Evicted => std::fs::read(self.spill.as_ref()?.join(filename(id))).ok(),
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
        if let Some(dir) = &self.spill {
            let _ = std::fs::write(dir.join(filename(&id)), &wire);
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
        if let Some(dir) = &self.spill {
            let _ = std::fs::remove_file(dir.join(filename(id)));
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
        std::fs::create_dir_all(dir)?;
        self.spill = Some(dir.to_path_buf());

        let mut adopted = Vec::new();
        let mut seq = self.map.len() as u64;
        for entry in std::fs::read_dir(dir)? {
            let path = entry?.path();
            let Some(name) = path.file_name().and_then(|s| s.to_str()) else { continue };
            let Some(id) = id_from_filename(name) else { continue };
            if self.map.contains_key(&id) {
                continue;
            }
            // Check the size before reading it. The directory is ours, but a
            // spill dir is on disk where anything can drop a file, and "adopt
            // whatever is here" must not mean "read a terabyte into memory".
            match std::fs::metadata(&path) {
                Ok(m) if m.len() <= MAX_ADOPT_BYTES => {}
                Ok(_) => {
                    let _ = std::fs::remove_file(&path);
                    continue;
                }
                Err(_) => continue,
            }
            let Ok(wire) = std::fs::read(&path) else { continue };
            let Ok((e, n)) = Envelope::decode(&wire) else {
                let _ = std::fs::remove_file(&path);
                continue;
            };
            // The name must match the content, the wire must be exactly one
            // envelope, and it must not already have expired.
            if n != wire.len() || e.id() != id || e.expiry <= now {
                let _ = std::fs::remove_file(&path);
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
        self.shed();
        Ok(adopted)
    }
}
