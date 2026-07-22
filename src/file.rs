use super::*;

/// First payload byte of a manifest.
pub const MANIFEST_TAG: u8 = 0x01;
/// First payload byte of a chunk: `[CHUNK_TAG][file_id:16][index:4][bytes]`.
pub const CHUNK_TAG: u8 = 0x07;

/// A published file: metadata plus the content IDs of its chunk envelopes in
/// index order. Signed on the wire, so the chunk IDs are authentic; because a
/// chunk envelope's ID is the hash of its bytes, holding a matching-ID
/// envelope is itself the integrity proof.
#[derive(Clone)]
pub struct Manifest {
    pub file_id: [u8; 16],
    pub chunk_size: u32,
    pub count: u32,
    pub total_len: u64,
    pub name: String,
    pub chunk_ids: Vec<Id>,
}

impl Manifest {
    pub fn encode(&self) -> Vec<u8> {
        let name = self.name.as_bytes();
        let mut p = Vec::with_capacity(35 + name.len() + 16 * self.chunk_ids.len());
        p.push(MANIFEST_TAG);
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
        if p.first() != Some(&MANIFEST_TAG) {
            return None;
        }
        let end = p.len();
        let mut o = 1usize;
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
        Some(Manifest { file_id, chunk_size, count, total_len, name, chunk_ids })
    }
}
