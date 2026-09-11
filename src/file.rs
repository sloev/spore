use super::*;

/// First payload byte of a leaf manifest — its ids name data chunks.
pub const MANIFEST_TAG: u8 = 0x01;
/// First payload byte of a chunk: `[CHUNK_TAG][bytes]`.
///
/// It used to be `[CHUNK_TAG][file_id:16][index:4][bytes]`, and those twenty
/// bytes are why a file published twice shared nothing with itself: `file_id`
/// was 16 random bytes minted per publish, so identical content produced a
/// different chunk every time (M11-M). Neither field was load-bearing — the
/// manifest already says which chunks are this file and in what order, and a
/// sealed chunk's AEAD nonce comes from its position in that order, which the
/// reader knows from walking the tree rather than from trusting the payload.
pub const CHUNK_TAG: u8 = 0x07;
/// First payload byte of an interior manifest — its ids name manifests one
/// level down. Followed immediately by the depth byte.
pub const TREE_TAG: u8 = 0x08;
/// First payload byte of a **sealed** root manifest: a manifest whose chunks are
/// each encrypted on their own, with the file key and the real file name sealed
/// to one recipient in a header. Followed by the depth byte, then
/// `[hdr_len:2][hdr]`. The recipient decrypts a chunk at a time, so a sealed
/// file costs one chunk of memory rather than all of it.
pub const SEALED_TAG: u8 = 0x09;

/// The size of every chunk but the last (M11-M).
///
/// **Static, and fixed by the protocol rather than by the publisher.** It used to
/// be `mtu - 64`, which was a leftover from before link fragmentation: a sender
/// had to cut for the narrowest hop it might meet, because nothing downstream
/// could re-cut. M11-D ended that — a bridge splits what its own link cannot
/// carry — and after it, all the publisher's MTU still decided was that a Wi-Fi
/// node and a LoRa node cut the same file differently and therefore shared no
/// content ids at all. Identical bytes have to produce identical chunks *for
/// everyone*, or content addressing names nothing twice.
///
/// That is the separation this constant exists to keep. A **chunk** is a
/// file-layer object, content-addressed, the same size everywhere. A **fragment**
/// is a link-layer artefact, sized by one hop's MTU, carrying whatever crosses
/// that hop — a chunk, or any other envelope. Neither should be derived from the
/// other, and for a while the chunk was.
///
/// 4 KiB is chosen against the smallest node rather than the largest file:
/// `Limits::for_budget` floors link reassembly at 16 KiB, so four chunks fit the
/// tightest profile's buffer at once, and a 1 MB file is 256 ids rather than the
/// ~6000 an MTU-sized chunk needed on LoRa (chunk-fragment-ok: naming the
/// rejected sizing). Over a 237-byte frame one chunk is
/// about eighteen fragments, which erasure repair covers better than it covers a
/// five-piece set — a fixed *fraction* of repair symbols gets more reliable as a
/// set grows, not less.
pub const CHUNK_BYTES: usize = 4096;

/// How deep a manifest tree may go. Each level multiplies capacity by the
/// interior fan-out (~84 at a 1400-byte MTU), so four levels is already ~5 TB —
/// the cap exists to bound recursion on a hostile tree, not to bound files.
pub const MAX_DEPTH: u8 = 4;

/// Wire overhead of an *unsigned* interior node: 16-byte header + 2-byte plen.
/// Interior nodes need no signature — the parent lists them by content id, and
/// an id is the hash of the wire, so the parent authenticates them.
pub const INTERIOR_ENV_OVERHEAD: usize = 18;
/// Wire overhead of the *signed* root: header 16 + full source key 32 + plen 2
/// + signature 64.
pub const ROOT_ENV_OVERHEAD: usize = 114;

/// Fixed manifest fields ahead of the id list: file_id, chunk_size, count,
/// total_len, name_len.
const FIXED: usize = 16 + 4 + 4 + 8 + 2;

/// Payload bytes a manifest spends before its id list.
const fn header_len(depth: u8, name_len: usize, sealed: bool) -> usize {
    if sealed {
        // Sealed root: tag, depth, and the 16-byte id of the header object.
        1 + 1 + 16 + FIXED + name_len
    } else if depth == 0 {
        // Leaf manifests carry only the tag; interior ones also a depth byte.
        1 + FIXED + name_len
    } else {
        2 + FIXED + name_len
    }
}

/// How many ids fit in one unsigned interior node at `mtu`. Interior nodes go
/// unnamed and are never sealed — they carry only hashes — so this is the same
/// at every level of every tree.
pub fn interior_fanout(mtu: usize) -> usize {
    mtu.saturating_sub(INTERIOR_ENV_OVERHEAD + header_len(1, 0, false)) / 16
}

/// How many ids fit in the signed root at `mtu`, for a file whose name is
/// `name_len` bytes, whose tree is `depth` levels deep, and whose sealed header
/// is sealed or not.
pub fn root_fanout(mtu: usize, name_len: usize, depth: u8, sealed: bool) -> usize {
    mtu.saturating_sub(ROOT_ENV_OVERHEAD + header_len(depth, name_len, sealed)) / 16
}

/// A published file, or one interior level of one.
///
/// At `depth == 0` the ids name chunk envelopes; at `depth > 0` they name
/// manifest envelopes of `depth - 1`. Because a chunk — and equally a
/// sub-manifest — is addressed by the hash of its own bytes, holding an
/// envelope whose id matches the one its parent named *is* the integrity proof.
/// Only the root is signed; the hash chain covers everything beneath it.
#[derive(Clone)]
pub struct Manifest {
    pub file_id: [u8; 16],
    pub chunk_size: u32,
    pub count: u32,
    pub total_len: u64,
    pub name: String,
    pub chunk_ids: Vec<Id>,
    pub depth: u8,
    /// Sealed to one recipient: the file key and the real file name. Empty for
    /// a manifest in the clear. When set, `total_len` is the *plaintext* length
    /// and every chunk below carries ciphertext.
    /// For a **sealed** root: the content id of the envelope carrying the
    /// sealed header, rather than the header itself. All-zero when not sealed.
    ///
    /// The header — an ephemeral key, an AEAD tag, the file key and the real
    /// name — is ~82 bytes, and it used to sit *inside* the signed root on top
    /// of the root's own 114 bytes of source key and signature. That put a
    /// sealed root past 256 bytes before it could name a single chunk, so
    /// `publish_file_sealed` was impossible on every LoRa profile: it misses raw
    /// LoRa's ~255-byte frame by one byte and Meshtastic's 237 by nineteen.
    /// Naming the header instead of carrying it costs 16 bytes and brings the
    /// floor to ~188, which every LoRa profile clears.
    pub hdr_id: Id,
}

/// The **content id** of a file-layer payload: the first 16 bytes of its
/// SHA-256 (M11-M).
///
/// Distinct from an envelope id, which hashes the whole envelope and so covers
/// `expiry` and `dest`. That is correct for a message and wrong for bytes: it
/// means the same chunk published a second later is a different object. A
/// content id names the bytes and nothing else, so two publishers — or the same
/// publisher twice — produce the same name for the same content.
pub fn content_id(payload: &[u8]) -> Id {
    let mut id = [0u8; 16];
    id.copy_from_slice(&Sha256::digest(payload)[..16]);
    id
}

/// The content id of whatever file-layer object an envelope carries, if it
/// carries one. Used by the store to index content alongside envelopes.
pub fn content_id_of_wire(wire: &[u8]) -> Option<Id> {
    let (e, _) = Envelope::decode(wire).ok()?;
    match e.payload.first()? {
        &MANIFEST_TAG | &CHUNK_TAG | &TREE_TAG | &SEALED_TAG => Some(content_id(&e.payload)),
        _ => None,
    }
}

impl Manifest {
    /// Is this a sealed root — one whose chunks are each encrypted and whose
    /// real name lives in a header object the root names?
    pub fn sealed(&self) -> bool {
        self.hdr_id != [0u8; 16]
    }

    pub fn encode(&self) -> Vec<u8> {
        let name = self.name.as_bytes();
        let mut p =
            Vec::with_capacity(header_len(self.depth, name.len(), self.sealed()) + 16 * self.chunk_ids.len());
        // A depth-0 manifest in the clear encodes exactly as it did before trees
        // existed, so every file that fits one envelope stays byte-identical.
        if self.sealed() {
            p.push(SEALED_TAG);
            p.push(self.depth);
            p.extend_from_slice(&self.hdr_id);
        } else if self.depth == 0 {
            p.push(MANIFEST_TAG);
        } else {
            p.push(TREE_TAG);
            p.push(self.depth);
        }
        p.extend_from_slice(&self.file_id);
        p.extend_from_slice(&self.chunk_size.to_be_bytes());
        p.extend_from_slice(&self.count.to_be_bytes());
        p.extend_from_slice(&self.total_len.to_be_bytes());
        p.extend_from_slice(&(name.len() as u16).to_be_bytes());
        p.extend_from_slice(name);
        for c in &self.chunk_ids {
            p.extend_from_slice(c);
        }
        p
    }

    pub fn decode(p: &[u8]) -> Option<Manifest> {
        let end = p.len();
        let mut o = 1usize;
        let mut hdr_id: Id = [0u8; 16];
        let depth = match p.first() {
            Some(&MANIFEST_TAG) => 0,
            Some(&TREE_TAG) => {
                let d = *p.get(1)?;
                // depth 0 belongs to the leaf tag; anything past MAX_DEPTH is a
                // tree we refuse to walk.
                if d == 0 || d > MAX_DEPTH {
                    return None;
                }
                o += 1;
                d
            }
            Some(&SEALED_TAG) => {
                // A sealed root may sit at any depth — sealing is about the
                // chunks, not the shape of the tree above them.
                let d = *p.get(1)?;
                if d > MAX_DEPTH {
                    return None;
                }
                o += 1;
                if o + 2 > end {
                    return None;
                }
                if o + 16 > end {
                    return None;
                }
                hdr_id.copy_from_slice(&p[o..o + 16]);
                o += 16;
                d
            }
            _ => return None,
        };
        if o + 16 > end {
            return None;
        }
        let mut file_id = [0u8; 16];
        file_id.copy_from_slice(&p[o..o + 16]);
        o += 16;
        if o + 4 > end {
            return None;
        }
        let chunk_size = u32::from_be_bytes([p[o], p[o + 1], p[o + 2], p[o + 3]]);
        o += 4;
        if o + 4 > end {
            return None;
        }
        let count = u32::from_be_bytes([p[o], p[o + 1], p[o + 2], p[o + 3]]);
        o += 4;
        if o + 8 > end {
            return None;
        }
        let mut tb = [0u8; 8];
        tb.copy_from_slice(&p[o..o + 8]);
        let total_len = u64::from_be_bytes(tb);
        o += 8;
        if o + 2 > end {
            return None;
        }
        let name_len = u16::from_be_bytes([p[o], p[o + 1]]) as usize;
        o += 2;
        if o + name_len > end {
            return None;
        }
        let name = String::from_utf8_lossy(&p[o..o + name_len]).into_owned();
        o += name_len;
        // Reject an implausible count before allocating for it.
        if count as usize > (end - o) / 16 {
            return None;
        }
        let mut chunk_ids = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let mut c = [0u8; 16];
            c.copy_from_slice(&p[o..o + 16]);
            o += 16;
            chunk_ids.push(c);
        }
        Some(Manifest { file_id, chunk_size, count, total_len, name, chunk_ids, depth, hdr_id })
    }
}
