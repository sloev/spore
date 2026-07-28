//! The `Node` router, split across files by concern (task #23).
//!
//! `Node` itself and every shared type/const stay in `lib.rs`; these are the
//! method groups, each an `impl Node` block. Being descendants of the crate
//! root, they reach `Node`'s private fields directly — the split widens no
//! field's visibility. Where one group's private method is called from another,
//! the compiler required it be `pub(crate)`; those are internal-API methods,
//! and the field-level invariants are untouched.

mod datagram;
mod files;
mod identity;
mod ingest;
mod send;
mod sync;
