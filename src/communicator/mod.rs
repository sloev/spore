//! **The application layer (M10)** — the state every host currently reimplements.
//!
//! The kernel owns no application state at all: no contact book, no conversation
//! store, no message history anywhere in `src/`. `petname` exists as an ANNOUNCE
//! field and a CLI config key and nowhere else. Above that sit three host layers
//! that have already diverged — `wasm.rs`, `ffi.rs` and `android/jni` export
//! different subsets of the same kernel — and on top of *those*, the communicator
//! is written a third and fourth time, in ~6130 lines of Kotlin and ~1300 of JS.
//!
//! This module is where that stops. It is consolidation rather than invention:
//! `android/jni/src/lib.rs` is already 1637 lines of Rust holding a stateful
//! runtime over the kernel, and the JS stores under `web/app/stores/` are
//! deliberately written as plain data-in/questions-out objects so this port could
//! reproduce them without a rewrite.
//!
//! # The four rules
//!
//! The communicator lives in the crate's **portable half**, which the wasm and
//! ESP32 CI jobs already enforce for everything below it. Three of the four rules
//! are therefore mechanical — break them and a build stops — and the fourth is
//! not, which is exactly why it is written here.
//!
//! 1. **No filesystem.** Storage is reached through the host port. The browser is
//!    the only target with no filesystem at all, and `wasm.rs`'s `JsSpill` is the
//!    precedent: the host owns the store, as it owns the clock and the CSPRNG.
//! 2. **No threads.** `wasm32-unknown-unknown` has none. The layer is driven by
//!    its host — "here is a tick, do your work" — as the kernel already is.
//! 3. **No new dependencies.** The optional `iroh` feature moved the MSRV floor
//!    from 1.75 to 1.85 because Cargo resolves optional dependencies too; that is
//!    the scar tissue this rule comes from.
//! 4. **No reading the clock.** This is the one with no compile guard:
//!    `SystemTime::now()` and `Instant::now()` *compile* on
//!    `wasm32-unknown-unknown` and then panic at runtime. Time arrives as a
//!    parameter, the way it already does throughout the kernel. Nothing in this
//!    module calls a clock, and a reviewer should treat an addition that does as
//!    a defect rather than a style question.
//!
//! # Why it is a feature
//!
//! `communicator` is default-on and **off for ESP32**, which is a headless relay
//! with 320–512 KB of SRAM, no screen and no conversations. Excluded by design,
//! not by limitation: portability of the *kernel* is the win, and the application
//! layer goes where there is a user.

pub mod api;
pub mod contact;
pub mod thread;
pub mod topic;

mod cursor;
pub(crate) use cursor::{Cursor, Writer};

pub use api::Communicator;
pub use contact::{contact_rows, Contact, ContactRow, ContactStore, PeerSeen, View};
pub use thread::{MessageStatus, ThreadStore};
pub use topic::{Post, TopicStore};

#[cfg(test)]
mod portability {
    //! The four rules, enforced (M10).
    //!
    //! Three of them already have a compile guard: `wasm32-unknown-unknown` has
    //! no filesystem and no threads, and a new dependency that will not build on
    //! Espressif's Xtensa fork breaks the ESP32 job. **Rule 4 has none** —
    //! `SystemTime::now()` and `Instant::now()` compile happily for wasm and then
    //! panic at runtime, so the first sign of trouble is a browser node dying in
    //! a user's tab. That is the gap this module exists to close.
    //!
    //! Scanning source text is a blunt instrument, and it is the right one here:
    //! the failure mode is someone reaching for the obvious call while writing a
    //! new store, and a grep catches that at the moment it is introduced rather
    //! than on the next wasm build somebody happens to run.

    /// Every source file in this module. A new file must be added here, which is
    /// the point — a store that is not on this list is not checked, so forgetting
    /// should be visible rather than silent.
    const SOURCES: &[(&str, &str)] =
        &[("mod.rs", include_str!("mod.rs")), ("thread.rs", include_str!("thread.rs"))];

    /// The shipped code of one file: everything before the first `#[cfg(test)]`,
    /// with comment bodies stripped.
    ///
    /// Both exclusions are load-bearing. **Test code is cut** because it does not
    /// ship — a test may name `SystemTime` freely, and the first version of this
    /// guard failed on its own list of banned strings. **Comments are stripped**
    /// so the rules can be discussed in the prose without the discussion tripping
    /// the check that enforces them.
    fn code_only(src: &str) -> String {
        let src = match src.find("#[cfg(test)]") {
            Some(i) => &src[..i],
            None => src,
        };
        src.lines()
            .map(|l| match l.find("//") {
                Some(i) => &l[..i],
                None => l,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn rule_four_nothing_here_reads_a_clock() {
        // Time arrives as a `now: u32` parameter, the way it already does
        // throughout the kernel. These two compile for wasm and panic at run
        // time, which is why no build catches them.
        for (name, src) in SOURCES {
            let code = code_only(src);
            for banned in ["SystemTime", "Instant::now", "UNIX_EPOCH"] {
                assert!(
                    !code.contains(banned),
                    "communicator/{name} reads a clock (`{banned}`). Time is a parameter here: \
                     these compile on wasm32-unknown-unknown and panic at runtime, so no build \
                     will tell you. Take `now: u32` from the host instead."
                );
            }
        }
    }

    #[test]
    fn rule_one_nothing_here_touches_a_filesystem() {
        // The browser has no filesystem at all. Storage is the host's, reached
        // through the port — `wasm.rs`'s `JsSpill` is the precedent.
        for (name, src) in SOURCES {
            let code = code_only(src);
            for banned in ["std::fs", "File::", "PathBuf", "std::path"] {
                assert!(
                    !code.contains(banned),
                    "communicator/{name} reaches a filesystem (`{banned}`). Persist through the \
                     host storage port instead; encode/decode to bytes and let the host keep them."
                );
            }
        }
    }

    #[test]
    fn rule_two_nothing_here_spawns_a_thread() {
        // `wasm32-unknown-unknown` has none. The layer is driven by its host.
        for (name, src) in SOURCES {
            let code = code_only(src);
            for banned in ["std::thread", "thread::spawn", "Mutex", "RwLock"] {
                assert!(
                    !code.contains(banned),
                    "communicator/{name} uses threading (`{banned}`). There are no threads on \
                     wasm32-unknown-unknown; this layer is driven by the host, one tick at a time."
                );
            }
        }
    }
}
