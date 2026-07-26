//! A **spool**: separate outbound and inbound directories, moved between hosts by
//! something else. This is the shape of every store-and-forward mailer —
//! NNCP, UUCP, a mail queue, an `rsync` cron, a USB stick carried between two
//! machines — where you *drop* a file in one place and *collect* from another,
//! rather than sharing one folder both ways.
//!
//! It differs from [`super::store`] (one folder, read and written by both sides,
//! which is what Syncthing gives you) in exactly that split:
//!
//! ```text
//!   tx/   we write outbound envelopes here; the mover delivers them to the peer
//!   rx/   the peer's mover deposits envelopes here; we consume and remove them
//! ```
//!
//! Consuming means removing: a spool that never drains grows without bound, and
//! the file is ours once it is in our store. Outbound files are left for the
//! mover to pick up (and it removes them). Names are `<hexid>.spore`, validated
//! as hex on the way in so a hostile spool cannot make us read a path we did not
//! choose, and each read is size-bounded.
//!
//! Nothing here is trusted — the mover may be a stranger's server or a courier's
//! pocket. Envelopes are signed and sealed, so a spool can withhold, reorder or
//! duplicate, but cannot forge.

use crate::*;
use std::path::{Path, PathBuf};
use std::{fs, io};

/// Largest spool file we will read. One envelope, generously; a bigger file
/// cannot be a valid entry since its id would not match its name.
pub const MAX_SPOOL_FILE: u64 = 1024 * 1024;

/// How often to sweep the inbound directory.
pub const POLL: std::time::Duration = std::time::Duration::from_millis(500);

fn is_spore_name(name: &str) -> bool {
    match name.strip_suffix(".spore") {
        Some(hex) => hex.len() == 32 && hex.bytes().all(|c| c.is_ascii_hexdigit()),
        None => false,
    }
}

/// Bridge two spool directories: write outbound envelopes to `tx`, consume
/// inbound ones from `rx`. Point NNCP, UUCP, rsync or a USB stick at them.
pub fn run(
    hub: super::hub::Shared,
    iface: Iface,
    rx_chan: std::sync::mpsc::Receiver<Forward>,
    tx: PathBuf,
    rx: PathBuf,
) -> io::Result<()> {
    fs::create_dir_all(&tx)?;
    fs::create_dir_all(&rx)?;
    println!("  [spool] iface {iface} tx={} rx={}", tx.display(), rx.display());
    loop {
        ingest_dir(&hub, iface, &rx)?;

        while let Ok(f) = rx_chan.try_recv() {
            let (Forward::Flood { bytes, .. } | Forward::Directed { bytes, .. }) = f;
            if let Ok((e, _)) = Envelope::decode(&bytes) {
                let path = tx.join(super::store::filename(&e.id()));
                // Content-addressed: same id means same bytes, so never rewrite.
                if !path.exists() {
                    // Write to a temp name and rename in, so the mover never sees
                    // a half-written file.
                    let tmp = tx.join(format!(".{}.tmp", super::store::filename(&e.id())));
                    if fs::write(&tmp, &bytes).is_ok() {
                        let _ = fs::rename(&tmp, &path);
                    }
                }
            }
        }
        std::thread::sleep(POLL);
    }
}

/// Feed every valid inbound file to the node, then remove it. Skips anything
/// whose name is not a content id, or whose body is too big or malformed —
/// removing the malformed ones so a poisoned spool does not jam forever.
fn ingest_dir(hub: &super::hub::Shared, iface: Iface, dir: &Path) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else { continue };
        if !is_spore_name(name) {
            continue; // not ours, or a temp file — leave it alone
        }
        match fs::metadata(&path) {
            Ok(m) if m.len() <= MAX_SPOOL_FILE => {}
            Ok(_) => {
                let _ = fs::remove_file(&path); // too big to be an envelope
                continue;
            }
            Err(_) => continue,
        }
        let Ok(bytes) = crate::store::read_capped(&path, MAX_SPOOL_FILE) else { continue };
        // Only accept a file whose bytes actually hash to the id in its name,
        // so a spool cannot smuggle in something under a chosen name.
        match Envelope::decode(&bytes) {
            Ok((e, n)) if n == bytes.len() && is_named_for(name, &e.id()) => {
                hub.on_rx(iface, &bytes, None);
            }
            _ => {}
        }
        // Consumed (or junk): remove it, so the spool drains.
        let _ = fs::remove_file(&path);
    }
    Ok(())
}

fn is_named_for(name: &str, id: &Id) -> bool {
    name == super::store::filename(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Tmp(PathBuf);
    impl Tmp {
        fn new(tag: &str) -> Tmp {
            let mut p = std::env::temp_dir();
            p.push(format!("spore-spool-{tag}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&p);
            fs::create_dir_all(&p).unwrap();
            Tmp(p)
        }
    }
    impl Drop for Tmp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn signed(text: &[u8]) -> Envelope {
        // Use wall-clock now so the envelope is not already expired when the
        // hub (which uses the real clock) ingests it.
        let now = super::super::hub::now();
        let mut n = Node::new("s", &[]);
        n.originate(ZERO_DEST, text.to_vec(), now); // stores it
        let wire = n.store_wires().into_iter().next().unwrap().1;
        Envelope::decode(&wire).unwrap().0
    }

    #[test]
    fn an_inbound_envelope_is_ingested_then_removed() {
        let d = Tmp::new("rx");
        let hub = super::super::hub::Hub::new(Node::new("n", &[]));
        let iface = hub.register_pull();

        let e = signed(b"the dam holds");
        let path = d.0.join(super::super::store::filename(&e.id()));
        fs::write(&path, e.wire()).unwrap();

        ingest_dir(&hub, iface, &d.0).unwrap();
        assert!(hub.with_node(|n| n.has(&e.id())), "the envelope reached the node");
        assert!(!path.exists(), "a consumed spool file is removed so the spool drains");
    }

    #[test]
    fn a_file_whose_name_lies_about_its_content_is_refused() {
        let d = Tmp::new("forged");
        let hub = super::super::hub::Hub::new(Node::new("n", &[]));
        let iface = hub.register_pull();

        let e = signed(b"real");
        // Correct bytes, wrong name — a spool trying to smuggle under a chosen id.
        let mut lie = e.id();
        lie[0] ^= 0xff;
        let wrong = d.0.join(super::super::store::filename(&lie));
        fs::write(&wrong, e.wire()).unwrap();

        ingest_dir(&hub, iface, &d.0).unwrap();
        assert!(!hub.with_node(|n| n.has(&e.id())), "content must match the name");
        assert!(!wrong.exists(), "and the junk is cleared rather than retried forever");
    }

    #[test]
    fn names_are_validated_as_content_ids() {
        assert!(is_spore_name("0123456789abcdef0123456789abcdef.spore"));
        assert!(!is_spore_name("notahex.spore"));
        assert!(!is_spore_name("0123.spore"));
        assert!(!is_spore_name(".0123456789abcdef0123456789abcdef.spore.tmp"));
        assert!(!is_spore_name("../escape.spore"));
    }

    #[test]
    fn the_read_itself_is_capped_not_just_the_stat() {
        // The size check and the read are two separate syscalls, and between them
        // the mover (or a hostile spool) can swap the file. Whatever `read_bounded`
        // is pointed at, it must refuse rather than buffer it — that is what makes
        // the stat an optimisation instead of the only defence.
        let d = Tmp::new("toctou");
        let path = d.0.join("b".repeat(32) + ".spore");
        fs::write(&path, vec![0u8; (MAX_SPOOL_FILE + 1) as usize]).unwrap();
        assert!(
            crate::store::read_capped(&path, MAX_SPOOL_FILE).is_err(),
            "over-cap file must not be read into memory"
        );

        // And a file just under the cap still arrives intact.
        fs::write(&path, vec![7u8; (MAX_SPOOL_FILE - 1) as usize]).unwrap();
        assert_eq!(
            crate::store::read_capped(&path, MAX_SPOOL_FILE).unwrap().len(),
            (MAX_SPOOL_FILE - 1) as usize
        );
    }

    #[test]
    fn an_oversized_inbound_file_is_dropped_not_read() {
        let d = Tmp::new("huge");
        let hub = super::super::hub::Hub::new(Node::new("n", &[]));
        let iface = hub.register_pull();
        // A plausible name, a body far too big to be an envelope.
        let name = "a".repeat(32) + ".spore";
        let path = d.0.join(&name);
        fs::write(&path, vec![0u8; (MAX_SPOOL_FILE + 1) as usize]).unwrap();

        ingest_dir(&hub, iface, &d.0).unwrap();
        assert!(!path.exists(), "oversized file removed rather than read into memory");
        assert_eq!(hub.with_node(|n| n.store_len()), 0);
    }
}
