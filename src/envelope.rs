//! §2 the envelope — the only object that goes on the wire.
//!
//! Everything SPORE moves is one of these: a fixed 16-byte header, an optional
//! source (a full key or an 8-byte address), a length-prefixed payload, and an
//! optional signature. The id is the hash of the body with `hops` zeroed, which is
//! what makes it stable while the envelope is relayed and decremented.
//!
//! Extracted from `lib.rs` unchanged — same layout, same bytes, same golden
//! vectors — and re-exported at the crate root, so `spore::Envelope`,
//! `spore::Src`, `spore::ty`, `spore::fl` and `spore::VER` all still resolve.
//!
//! `decode` is the front door for every hostile byte the project will ever see.
//! It is the most-fuzzed function here (`fuzz/fuzz_targets/envelope_decode.rs`)
//! and it must return `Err` rather than panic on anything at all — see the
//! reproductions in `docs/SECURITY_FINDINGS.md`.

use crate::*;

// ---------------------------------------------------------------------------
// §2 Envelope — the only object
// ---------------------------------------------------------------------------

pub const VER: u8 = 0x01;

pub mod ty {
    pub const DATA: u8 = 0;
    pub const INV: u8 = 1;
    pub const WANT: u8 = 2;
    pub const ANNOUNCE: u8 = 3;
}
pub mod fl {
    pub const ENCRYPTED: u8 = 1;
    pub const SIGNED: u8 = 2;
    pub const FRAGMENT: u8 = 4;
    pub const ACKREQ: u8 = 8;
    pub const FLOOD: u8 = 16; // multicast / topic / public / route-discovery
    pub const SRC8: u8 = 32; // src carried as 8-byte address, not 32-byte key
    /// §7: an ENCRYPTED DATA payload is ratchet-encrypted (`Ratchet::encrypt`),
    /// not a one-shot prekey seal. Bits 64/128 were unused; `decode` copies
    /// flags verbatim with no known-bit validation, so this is a forward- and
    /// backward-compatible addition — older code simply never reads it.
    pub const RATCHET: u8 = 64;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Src {
    None,
    Full([u8; 32]),
    Short(Addr),
}

#[derive(Clone, Debug)]
pub struct Envelope {
    pub typ: u8,
    pub flags: u8,
    pub hops: u8,
    pub expiry: u32,
    pub dest: Addr,
    pub src: Src,
    pub payload: Vec<u8>,
    pub sig: Option<[u8; 64]>,
}

#[derive(Debug)]
pub enum Err {
    Short,
    Version,
    Bad,
}

impl Envelope {
    pub fn new(typ: u8, dest: Addr, expiry: u32, payload: Vec<u8>) -> Self {
        Envelope { typ, flags: 0, hops: 16, expiry, dest, src: Src::None, payload, sig: None }
    }

    /// Header + src + plen + payload (no signature). `zero_hops` for the
    /// signing/ID pre-image; false for the wire form.
    fn body(&self, zero_hops: bool) -> Vec<u8> {
        let mut b = Vec::with_capacity(64 + self.payload.len());
        b.push(VER);
        b.push(self.typ);
        b.push(self.flags);
        b.push(if zero_hops { 0 } else { self.hops });
        b.extend_from_slice(&self.expiry.to_be_bytes());
        b.extend_from_slice(&self.dest);
        match &self.src {
            Src::None => {}
            Src::Full(pk) => b.extend_from_slice(pk),
            Src::Short(a) => b.extend_from_slice(a),
        }
        b.extend_from_slice(&(self.payload.len() as u16).to_be_bytes());
        b.extend_from_slice(&self.payload);
        b
    }

    /// The exact bytes put on the wire.
    pub fn wire(&self) -> Vec<u8> {
        let mut b = self.body(false);
        if let Some(sig) = &self.sig {
            b.extend_from_slice(sig);
        }
        b
    }

    /// ID = SHA-256(full envelope with hops byte zeroed)[..16]. Ties the id to
    /// the signature, and is stable under relays decrementing `hops`.
    pub fn id(&self) -> Id {
        let mut b = self.body(true);
        if let Some(sig) = &self.sig {
            b.extend_from_slice(sig);
        }
        let d = Sha256::digest(&b);
        let mut id = [0u8; 16];
        id.copy_from_slice(&d[..16]);
        id
    }

    /// Priority stamp: leading zero bits of the ID (proof-of-work, §10).
    pub fn stamp(&self) -> u8 {
        let id = self.id();
        let mut n = 0u8;
        for byte in id {
            if byte == 0 {
                n += 8;
            } else {
                n += byte.leading_zeros() as u8;
                break;
            }
        }
        n
    }

    pub fn sign(&mut self, sk: &SigningKey) {
        self.src = Src::Full(sk.verifying_key().to_bytes());
        self.flags |= fl::SIGNED;
        self.flags &= !fl::SRC8;
        let sig: Signature = sk.sign(&self.body(true));
        self.sig = Some(sig.to_bytes());
    }

    /// Verify a full-key signed envelope. SRC8 envelopes need `verify_with`.
    pub fn verify(&self) -> bool {
        match &self.src {
            Src::Full(pk) => self.verify_with(pk),
            _ => false,
        }
    }
    pub fn verify_with(&self, pubkey: &[u8; 32]) -> bool {
        if self.flags & fl::SIGNED == 0 {
            return false;
        }
        let (Some(sig), Ok(vk)) = (self.sig, VerifyingKey::from_bytes(pubkey)) else {
            return false;
        };
        vk.verify(&self.body(true), &Signature::from_bytes(&sig)).is_ok()
    }

    pub fn decode(buf: &[u8]) -> Result<(Envelope, usize), Err> {
        if buf.len() < 16 {
            return std::result::Result::Err(Err::Short);
        }
        if buf[0] != VER {
            return std::result::Result::Err(Err::Version);
        }
        let typ = buf[1];
        let flags = buf[2];
        let hops = buf[3];
        let expiry = u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]);
        let mut dest = [0u8; 8];
        dest.copy_from_slice(&buf[8..16]);
        let mut off = 16;
        let need = |off: usize, n: usize| -> Result<(), Err> {
            if off + n <= buf.len() {
                Ok(())
            } else {
                std::result::Result::Err(Err::Short)
            }
        };
        let src = if flags & fl::SIGNED != 0 {
            if flags & fl::SRC8 != 0 {
                need(off, 8)?;
                let mut a = [0u8; 8];
                a.copy_from_slice(&buf[off..off + 8]);
                off += 8;
                Src::Short(a)
            } else {
                need(off, 32)?;
                let mut pk = [0u8; 32];
                pk.copy_from_slice(&buf[off..off + 32]);
                off += 32;
                Src::Full(pk)
            }
        } else {
            Src::None
        };
        need(off, 2)?;
        let plen = u16::from_be_bytes([buf[off], buf[off + 1]]) as usize;
        off += 2;
        need(off, plen)?;
        let payload = buf[off..off + plen].to_vec();
        off += plen;
        let sig = if flags & fl::SIGNED != 0 {
            need(off, 64)?;
            let mut s = [0u8; 64];
            s.copy_from_slice(&buf[off..off + 64]);
            off += 64;
            Some(s)
        } else {
            None
        };
        Ok((Envelope { typ, flags, hops, expiry, dest, src, payload, sig }, off))
    }
}
