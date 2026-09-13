//! A bounds-checked reader for the stores' persistence formats.
//!
//! Shared because every store faces the same input: **bytes the host kept**,
//! which are neither signed nor verified and may have been truncated by a
//! half-finished write, altered by a backup tool, or simply lost. Every read
//! here is fallible, so a short blob returns `None` rather than panicking on a
//! slice — one panic in a decoder is a browser tab that will not open again
//! until its storage is cleared by hand.
//!
//! Hand-rolled rather than a crate, per M10's rule 3: no new dependencies, in a
//! tree that has to build on Espressif's Xtensa fork.

use crate::{Addr, Id};

/// A bounds-checked reader. Every read is fallible, so a truncated blob returns
/// `None` rather than panicking on a slice — the host's storage is the one input
/// here that is neither signed nor verified.
pub(crate) struct Cursor<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> Cursor<'a> {
    pub(crate) fn new(b: &'a [u8]) -> Cursor<'a> {
        Cursor { b, i: 0 }
    }

    /// Whether every byte was consumed. Trailing bytes mean the blob is not what
    /// it claims to be, and a parse that stopped early and "worked" would
    /// silently drop whatever followed.
    pub(crate) fn at_end(&self) -> bool {
        self.i == self.b.len()
    }

    pub(crate) fn remaining(&self) -> usize {
        self.b.len() - self.i
    }

    pub(crate) fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let end = self.i.checked_add(n)?;
        let out = self.b.get(self.i..end)?;
        self.i = end;
        Some(out)
    }

    pub(crate) fn u8(&mut self) -> Option<u8> {
        Some(self.take(1)?[0])
    }

    pub(crate) fn bool(&mut self) -> Option<bool> {
        match self.u8()? {
            0 => Some(false),
            1 => Some(true),
            _ => None,
        }
    }

    pub(crate) fn u32(&mut self) -> Option<u32> {
        let b = self.take(4)?;
        Some(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub(crate) fn addr(&mut self) -> Option<Addr> {
        let b = self.take(8)?;
        let mut a = [0u8; 8];
        a.copy_from_slice(b);
        Some(a)
    }

    pub(crate) fn id(&mut self) -> Option<Id> {
        let b = self.take(16)?;
        let mut id = [0u8; 16];
        id.copy_from_slice(b);
        Some(id)
    }
}
