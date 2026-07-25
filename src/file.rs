use super::*;

/// First payload byte of a leaf manifest — its ids name data chunks.
pub const MANIFEST_TAG: u8 = 0x01;
/// First payload byte of a chunk: `[CHUNK_TAG][file_id:16][index:4][bytes]`.
pub const CHUNK_TAG: u8 = 0x07;
/// First payload byte of an interior manifest — its ids name manifests one
/// level down. Followed immediately by the depth byte.
pub const TREE_TAG: u8 = 0x08;

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
const fn header_len(depth: u8, name_len: usize) -> usize {
    // Leaf manifests carry only the tag; interior ones also carry a depth byte.
    if depth == 0 {
        1 + FIXED + name_len
    } else {
        2 + FIXED + name_len
    }
}

/// How many ids fit in one unsigned interior node at `mtu`. Interior nodes go
/// unnamed, so this is the same at every level.
pub fn interior_fanout(mtu: usize) -> usize {
    mtu.saturating_sub(INTERIOR_ENV_OVERHEAD + header_len(1, 0)) / 16
}

/// How many ids fit in the signed root at `mtu`, for a file whose name is
/// `name_len` bytes and whose tree is `depth` levels deep.
pub fn root_fanout(mtu: usize, name_len: usize, depth: u8) -> usize {
    mtu.saturating_sub(ROOT_ENV_OVERHEAD + header_len(depth, name_len)) / 16
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
}

impl Manifest {
    pub fn encode(&self) -> Vec<u8> {
        let name = self.name.as_bytes();
        let mut p = Vec::with_capacity(header_len(self.depth, name.len()) + 16 * self.chunk_ids.len());
        // A depth-0 manifest encodes exactly as it did before trees existed, so
        // every file that fits one envelope stays byte-identical on the wire.
        if self.depth == 0 {
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
        Some(Manifest { file_id, chunk_size, count, total_len, name, chunk_ids, depth })
    }
}
