//! Folder sync (Syncthing shape, §6 + files): publish a directory as
//! content-addressed manifests on a shared topic, and materialise the files a
//! subscriber has fully fetched. A newer signed manifest for the same name
//! supersedes the old.

use crate::*;
use std::path::Path;
use std::{fs, io};

/// Publish every file in `dir` as a manifest on `topic`. Returns the
/// manifest-flood forwards; the data itself is pulled on demand.
pub fn publish_dir(node: &mut Node, dir: &Path, topic: Addr, now: u32) -> io::Result<Vec<Forward>> {
    let mut out = Vec::new();
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if !path.is_file() {
            continue;
        }
        let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("file").to_string();
        let bytes = fs::read(&path)?;
        let (_magnet, f) = node.publish_file(&name, &bytes, topic, now);
        out.extend(f);
    }
    Ok(out)
}

/// Write every complete file the node holds into `out_dir` (newest manifest per
/// name). Returns how many files were written.
pub fn materialize(node: &Node, out_dir: &Path) -> io::Result<usize> {
    fs::create_dir_all(out_dir)?;
    let mut n = 0;
    for (name, bytes) in node.complete_files() {
        // Guard against path traversal in a name from the wire.
        let safe = Path::new(&name).file_name().map(|s| s.to_owned());
        if let Some(fname) = safe {
            fs::write(out_dir.join(fname), bytes)?;
            n += 1;
        }
    }
    Ok(n)
}
