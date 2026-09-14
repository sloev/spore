//! Where a desktop node keeps the things that must outlive the process.
//!
//! Until this existed, `run_config` called `Node::new` and nothing else: a
//! restart produced a **new address and an empty store**. Android and ESP32 both
//! solved this long ago; the daemon — the node the other two are debugged
//! against — did not. Issue #303 put it first for that reason.
//!
//! Three files, and the reasoning for each is the same reasoning Android uses:
//!
//! | file | what is lost without it |
//! |---|---|
//! | `seed` | the address. A node that changes identity every restart cannot be replied to, and cannot be left anywhere. |
//! | `prekeys` | inbound mail sealed to a prekey that has since rotated. The seed restores *who you are*; the ring restores *what you can still open*. |
//! | `store/` | everything the node was carrying for other people. Custody that does not survive a restart is not custody. |
//!
//! The directory is `$SPORE_HOME`, else `$XDG_DATA_HOME/spore`, else
//! `~/.local/share/spore` — and `%APPDATA%\spore` on Windows. Created on first
//! use with owner-only permissions where the platform has them.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Resolve the state directory without creating it.
pub(crate) fn dir() -> PathBuf {
    if let Ok(h) = std::env::var("SPORE_HOME") {
        if !h.is_empty() {
            return PathBuf::from(h);
        }
    }
    #[cfg(windows)]
    {
        if let Ok(app) = std::env::var("APPDATA") {
            if !app.is_empty() {
                return PathBuf::from(app).join("spore");
            }
        }
    }
    if let Ok(x) = std::env::var("XDG_DATA_HOME") {
        if !x.is_empty() {
            return PathBuf::from(x).join("spore");
        }
    }
    match std::env::var("HOME") {
        Ok(h) if !h.is_empty() => PathBuf::from(h).join(".local/share/spore"),
        // No HOME is unusual but not impossible (a bare container, a service
        // account). A relative directory beats refusing to start.
        _ => PathBuf::from(".spore"),
    }
}

/// Create the directory, owner-only where the platform supports it.
fn ensure(dir: &Path) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    restrict(dir)
}

/// `0700` / `0600`. The seed is key material: on a shared machine, a
/// world-readable state directory hands this node's identity to every other
/// account on it.
#[cfg(unix)]
fn restrict(p: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let meta = fs::metadata(p)?;
    let mode = if meta.is_dir() { 0o700 } else { 0o600 };
    fs::set_permissions(p, fs::Permissions::from_mode(mode))
}

#[cfg(not(unix))]
fn restrict(_p: &Path) -> io::Result<()> {
    // Windows inherits the user profile's ACL, which is already owner-only for
    // %APPDATA%. Saying so beats a no-op with no explanation.
    Ok(())
}

/// The seed for this home, creating one on first run.
///
/// Returns the seed and whether it was newly minted, so the caller can say which
/// happened — "same address as last time" is the thing a person actually wants
/// to see in the first line of output.
pub(crate) fn load_or_create_seed(dir: &Path) -> io::Result<([u8; 32], bool)> {
    ensure(dir)?;
    let path = dir.join("seed");
    match fs::read(&path) {
        Ok(b) if b.len() == 32 => {
            let mut seed = [0u8; 32];
            seed.copy_from_slice(&b);
            Ok((seed, false))
        }
        // A seed of the wrong length is not a seed. Refusing is the only safe
        // answer: silently minting a new one would change the node's identity
        // without saying so, which is precisely the failure this file exists to
        // stop, and overwriting it would destroy the only copy of an identity
        // that might be recoverable by hand.
        Ok(b) => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "{} is {} bytes, not 32. Refusing to start rather than silently \
                 becoming a different node. Move it aside to mint a new identity.",
                path.display(),
                b.len()
            ),
        )),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            // Minted by the core rather than by this file: `Node::new` already
            // draws a seed from the same CSPRNG every other platform uses, and
            // a second way to make an identity is a second way to make a weak
            // one.
            let seed = spore::Node::new("bootstrap", &[]).seed();
            fs::write(&path, seed)?;
            restrict(&path)?;
            Ok((seed, true))
        }
        Err(e) => Err(e),
    }
}

pub(crate) fn prekey_path(dir: &Path) -> PathBuf {
    dir.join("prekeys")
}

pub(crate) fn store_dir(dir: &Path) -> PathBuf {
    dir.join("store")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A scratch home, removed on drop. `SPORE_HOME` is read from the
    /// environment by `dir()`, and tests share a process, so these pass paths
    /// explicitly rather than setting it.
    struct Tmp(PathBuf);

    impl Tmp {
        fn new(name: &str) -> Tmp {
            let p = std::env::temp_dir().join(format!("spore-home-test-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&p);
            Tmp(p)
        }
    }

    impl Drop for Tmp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn a_seed_is_minted_once_and_then_restored() {
        // The whole point of #303's first item: a node that changes address
        // every restart cannot be replied to and cannot be left anywhere.
        let t = Tmp::new("mint");
        let (first, fresh) = load_or_create_seed(&t.0).expect("a fresh home mints one");
        assert!(fresh, "the first call mints");
        let (again, fresh2) = load_or_create_seed(&t.0).expect("and the second reads it back");
        assert!(!fresh2, "the second call restores");
        assert_eq!(first, again, "a restart must not change the identity");
    }

    #[test]
    fn the_same_seed_gives_the_same_address() {
        // The seed is only interesting because of what it derives.
        let t = Tmp::new("addr");
        let (seed, _) = load_or_create_seed(&t.0).unwrap();
        let a = spore::Node::from_seed("n", &[], &seed).addr;
        let b = spore::Node::from_seed("different petname", &["and", "topics"], &seed).addr;
        assert_eq!(a, b, "the address comes from the seed, not from the config around it");
    }

    #[test]
    fn a_seed_of_the_wrong_length_stops_the_node_rather_than_replacing_it() {
        // Silently minting a new one would change identity without saying so —
        // the exact failure this file exists to prevent — and overwriting would
        // destroy the only copy of something that might be recoverable by hand.
        let t = Tmp::new("short");
        fs::create_dir_all(&t.0).unwrap();
        fs::write(t.0.join("seed"), b"too short").unwrap();
        let err = load_or_create_seed(&t.0).expect_err("a 9-byte seed is not a seed");
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        assert_eq!(fs::read(t.0.join("seed")).unwrap(), b"too short", "and it is left alone");
    }

    #[cfg(unix)]
    #[test]
    fn the_seed_is_not_readable_by_other_accounts() {
        use std::os::unix::fs::PermissionsExt;
        let t = Tmp::new("perms");
        load_or_create_seed(&t.0).unwrap();
        let mode = fs::metadata(t.0.join("seed")).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "the seed is key material on a possibly shared machine");
        let dmode = fs::metadata(&t.0).unwrap().permissions().mode() & 0o777;
        assert_eq!(dmode, 0o700);
    }

    #[test]
    fn spore_home_overrides_the_platform_default() {
        // Needed by the hardware procedures: two nodes on one laptop have to be
        // able to be two different nodes.
        let before = std::env::var("SPORE_HOME").ok();
        std::env::set_var("SPORE_HOME", "/tmp/spore-explicit");
        assert_eq!(dir(), PathBuf::from("/tmp/spore-explicit"));
        std::env::set_var("SPORE_HOME", "");
        assert_ne!(dir(), PathBuf::from(""), "an empty value falls through to the default");
        match before {
            Some(v) => std::env::set_var("SPORE_HOME", v),
            None => std::env::remove_var("SPORE_HOME"),
        }
    }
}
