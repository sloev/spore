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
        // A synced folder is written by Syncthing/Dropbox/whatever, not by us.
        let Ok(bytes) = crate::store::read_capped(&path, crate::store::MAX_FILE_BYTES) else {
            continue;
        };
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
    // Names then stream, rather than `complete_files()`: that assembles every
    // file into RAM at once, which on a disk-backed store pulls the whole store
    // into memory — 256 MB on an Android node configured to keep 8 MB resident
    // (S-028). One file is in flight at a time here.
    for (name, magnet) in node.complete_file_names() {
        // A manifest's name is `from_utf8_lossy` of bytes a peer chose, so treat it
        // as hostile. `file_name()` strips every directory component, which is what
        // blocks traversal: "../../etc/passwd" becomes "passwd" *inside* out_dir,
        // and "." or ".." have no file name at all and are dropped.
        let Some(fname) = Path::new(&name).file_name().map(|s| s.to_owned()) else {
            continue;
        };
        // Writing must not use `?`. NUL is valid UTF-8, so a name like "a\0b"
        // survives decoding and `file_name()` returns it unchanged — but the OS
        // refuses it. Propagating that error aborted the whole loop, so one
        // poisoned manifest stopped *every other file* from ever being written,
        // for as long as it sat in the store. Skip what will not write and keep
        // going; the same applies to a permission error on one path (S-027).
        let Ok(mut fh) = fs::File::create(out_dir.join(&fname)) else {
            continue;
        };
        if node.write_file_to(&magnet, &mut fh).is_some() {
            n += 1;
        }
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// S-027. One hostile filename used to stop *every* file being written.
    ///
    /// The name in a manifest is `String::from_utf8_lossy` of wire bytes, and NUL
    /// is valid UTF-8, so it survives. `file_name()` strips directories — traversal
    /// is genuinely blocked — but it happily returns `"a\0b"`, which `fs::write`
    /// rejects. With `?` on that write, one poisoned manifest aborted the loop and
    /// the honest files behind it were never materialised.
    #[test]
    fn a_hostile_filename_cannot_stop_the_other_files_being_written() {
        let dir = std::env::temp_dir().join(format!("spore_fs_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let mut node = Node::new("host", &[]);
        let topic = topic_of("sync");

        // A file whose name a peer chose, containing a NUL, published before the
        // honest one so it is hit first however the map happens to iterate.
        node.publish_file("bad\u{0}name", b"poison", topic, 1_700_000_000);
        node.publish_file("good.txt", b"payload", topic, 1_700_000_000);
        node.publish_file("also\u{0}bad", b"poison", topic, 1_700_000_000);

        let n = materialize(&node, &dir).expect("a bad name must not fail the whole call");
        assert_eq!(n, 1, "the writable file is written");
        assert_eq!(fs::read(dir.join("good.txt")).unwrap(), b"payload");
        // Nothing was created for the unwritable names.
        let written: Vec<_> = fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()).collect();
        assert_eq!(written.len(), 1, "exactly one file, not a partial mess");
        let _ = fs::remove_dir_all(&dir);
    }

    /// Traversal is blocked, and this pins it so a future refactor of the guard
    /// cannot quietly reintroduce it.
    #[test]
    fn a_name_cannot_escape_the_output_directory() {
        let dir = std::env::temp_dir().join(format!("spore_fs_esc_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let mut node = Node::new("host", &[]);
        let topic = topic_of("sync");
        for name in ["../../etc/passwd", "/etc/shadow", "x/../../y", "..", "."] {
            node.publish_file(name, b"nope", topic, 1_700_000_000);
        }
        let n = materialize(&node, &dir).unwrap();
        // `..` and `.` have no file name at all and are skipped entirely.
        assert_eq!(n, 3, "three names reduce to a basename, two are skipped");
        for f in fs::read_dir(&dir).unwrap() {
            let p = f.unwrap().path();
            assert_eq!(p.parent(), Some(dir.as_path()), "{p:?} escaped the directory");
        }
        assert!(dir.join("passwd").exists() && dir.join("shadow").exists() && dir.join("y").exists());
        let _ = fs::remove_dir_all(&dir);
    }
}
