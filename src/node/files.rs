//! Node — the manifest+chunk file layer: publish, fetch, assemble, open, onion-peel.
//!
//! Split out of the 3977-line `lib.rs` (task #23): `impl Node` in a descendant
//! module of the crate root, so it keeps full access to `Node`'s private fields
//! with no visibility change. A pure move — wire format and public API identical.

use crate::*;

impl Node {
    /// Publish `bytes` as a content-addressed file. Splits it into chunk
    /// envelopes (each addressed by its own content ID), stores them, and builds
    /// a signed manifest that lists those IDs. Returns the manifest ID — the
    /// **magnet** — and the `Forward`s to flood the small manifest. The data
    /// itself is pulled on demand (§6 custody / swarm), BitTorrent-style.
    ///
    /// A manifest is one envelope, so it can only name so many chunks. Past that
    /// the chunk ids are grouped under **interior manifests** and those are
    /// grouped again, until what remains fits the signed root — a Merkle tree of
    /// manifests whose root is still a single 16-byte magnet. Files small enough
    /// for one manifest produce exactly the bytes they always did.
    pub fn publish_file(&mut self, name: &str, bytes: &[u8], dest: Addr, now: u32) -> (Id, Vec<Forward>) {
        self.publish_object(name, bytes, dest, now, None, Vec::new())
    }

    /// The shared body of [`Node::publish_file`] and
    /// [`Node::publish_file_sealed`]: chunk, encrypt the chunks if there is a
    /// key, grow interior levels until what remains fits the signed root.
    ///
    /// Sealing changes only what goes *inside* a chunk. The tree above is the
    /// same shape either way, because it carries nothing but hashes.
    fn publish_object(
        &mut self,
        name: &str,
        bytes: &[u8],
        dest: Addr,
        now: u32,
        key: Option<&[u8; 32]>,
        sealed_hdr: Vec<u8>,
    ) -> (Id, Vec<Forward>) {
        let chunk_size = self.mtu.saturating_sub(64).max(1);
        let count = bytes.len().div_ceil(chunk_size).max(1);
        let expiry = now + 7 * 86400;
        let mut file_id = [0u8; 16];
        OsRng.fill_bytes(&mut file_id);
        // Chunks ride a per-file topic so only interested nodes carry them.
        let mut ft = [0u8; 8];
        ft.copy_from_slice(&Sha256::digest(file_id)[..8]);

        // (id, plaintext bytes of file covered) for the level being grouped.
        let mut level: Vec<(Id, u64)> = Vec::with_capacity(count);
        for i in 0..count {
            let start = i * chunk_size;
            let end = ((i + 1) * chunk_size).min(bytes.len());
            // The AEAD tag adds 16 bytes, which the chunk size already has room
            // for — a sealed chunk rides the same frame an open one does.
            let body = match key {
                Some(k) => chunk_seal(&bytes[start..end], k, i as u32),
                None => bytes[start..end].to_vec(),
            };
            let mut payload = Vec::with_capacity(21 + body.len());
            payload.push(file::CHUNK_TAG);
            payload.extend_from_slice(&file_id);
            payload.extend_from_slice(&(i as u32).to_be_bytes());
            payload.extend_from_slice(&body);
            let mut ce = Envelope::new(ty::DATA, ft, expiry, payload);
            ce.flags |= fl::FLOOD;
            level.push((ce.id(), (end - start) as u64));
            self.mark_seen(&ce);
            self.store_put(&ce, now);
        }

        // Grow interior levels until the remaining ids fit the signed root.
        // Interior nodes are unsigned and unnamed: the parent names them by
        // content id, which is hash enough, and dropping the signature buys back
        // ~96 bytes of fan-out per node.
        let fanout = file::interior_fanout(self.mtu).max(2);
        // `depth` is the depth a manifest would have if it named the current
        // level directly — 0 while `level` is still chunks. Each grouping pass
        // buries the level one deeper.
        let mut depth = 0u8;
        while depth < file::MAX_DEPTH
            && level.len() > file::root_fanout(self.mtu, name.len(), depth, sealed_hdr.len())
        {
            let mut next = Vec::with_capacity(level.len().div_ceil(fanout));
            for group in level.chunks(fanout) {
                let covered: u64 = group.iter().map(|(_, n)| *n).sum();
                let node = file::Manifest {
                    file_id,
                    chunk_size: chunk_size as u32,
                    count: group.len() as u32,
                    total_len: covered,
                    name: String::new(),
                    chunk_ids: group.iter().map(|(id, _)| *id).collect(),
                    depth,
                    sealed_hdr: Vec::new(),
                };
                let mut ne = Envelope::new(ty::DATA, ft, expiry, node.encode());
                ne.flags |= fl::FLOOD;
                next.push((ne.id(), covered));
                self.mark_seen(&ne);
                self.store_put(&ne, now);
            }
            level = next;
            depth += 1;
        }

        let manifest = file::Manifest {
            file_id,
            chunk_size: chunk_size as u32,
            count: level.len() as u32,
            total_len: bytes.len() as u64,
            name: name.to_string(),
            chunk_ids: level.iter().map(|(id, _)| *id).collect(),
            depth,
            sealed_hdr,
        };
        let mut me = Envelope::new(ty::DATA, dest, expiry, manifest.encode());
        if dest == ZERO_DEST || self.topics.contains(&dest) {
            me.flags |= fl::FLOOD;
        }
        me.sign(&self.sk);
        let magnet = me.id();
        self.manifests.insert(magnet, manifest);
        self.mark_seen(&me);
        self.store_put(&me, now);
        let forwards = self.forward_intents(&me, NO_IFACE, now);
        (magnet, forwards)
    }

    /// Register a manifest we received. Called automatically on delivery; also
    /// usable directly. Verifies the signature before trusting the chunk list.
    pub fn absorb_manifest(&mut self, e: &Envelope) -> Option<Id> {
        if !e.verify() {
            return None;
        }
        let m = file::Manifest::decode(&e.payload)?;
        let magnet = e.id();
        self.manifests.entry(magnet).or_insert(m);
        Some(magnet)
    }

    /// Read an interior manifest out of the store.
    ///
    /// Interior nodes are unsigned on purpose, so this must never be reached
    /// from anything but a parent's id list: the store is keyed by content hash,
    /// so an id a verified parent named can only resolve to the bytes that
    /// parent meant. `expect` pins the child's depth — it must be exactly one
    /// less than its parent's, or a crafted tree could recurse sideways.
    fn tree_node(&self, id: &Id, expect: u8) -> Option<file::Manifest> {
        let wire = self.store.wire(id)?;
        let (e, _) = Envelope::decode(&wire).ok()?;
        let m = file::Manifest::decode(&e.payload)?;
        (m.depth == expect).then_some(m)
    }

    /// Depth-first walk of a manifest tree, in file order.
    ///
    /// Calls `f(id, depth, held)` for every id the tree names — `depth == 0` for
    /// a data chunk, higher for an interior node — where `held` says whether we
    /// have it. A held interior node is descended into right after its call; an
    /// unheld one hides its whole subtree, which is exactly why the walk reports
    /// it. Returning `false` from `f` stops the walk, so callers that only need
    /// the next few ids never pay for the whole tree.
    pub(crate) fn walk_tree<F>(&self, m: &file::Manifest, f: &mut F) -> bool
    where
        F: FnMut(&Id, u8, bool) -> bool,
    {
        for id in &m.chunk_ids {
            if m.depth == 0 {
                if !f(id, 0, self.store.contains(id)) {
                    return false;
                }
                continue;
            }
            let child = self.tree_node(id, m.depth - 1);
            if !f(id, m.depth, child.is_some()) {
                return false;
            }
            if let Some(c) = child {
                if !self.walk_tree(&c, f) {
                    return false;
                }
            }
        }
        true
    }

    /// The next ids needed to make progress on `magnet`, at most `limit`, in
    /// file order. Interior nodes surface before the chunks beneath them because
    /// the walk reports a node before descending — so a tree resolves top-down
    /// without needing a separate scheduling pass.
    pub fn missing(&self, magnet: &Id, limit: usize) -> Vec<Id> {
        let Some(root) = self.manifests.get(magnet) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        if limit == 0 {
            return out;
        }
        self.walk_tree(root, &mut |id, _, held| {
            if !held {
                out.push(*id);
            }
            out.len() < limit
        });
        out
    }

    /// How many ids fit in one WANT frame at this node's MTU.
    fn want_window(&self) -> usize {
        (self.mtu.saturating_sub(file::INTERIOR_ENV_OVERHEAD) / 16).max(1)
    }

    /// Ask neighbours for the parts of `magnet` we don't hold yet. Reuses the
    /// WANT machinery: a chunk — and equally a sub-manifest — is an ordinary
    /// stored envelope, named by content, so any peer that has it answers from
    /// its store.
    ///
    /// One call asks for one frame's worth. A large file needs many, which is
    /// the point: the request is paced by the link rather than by the file.
    pub fn fetch(&mut self, magnet: &Id) -> Vec<Forward> {
        self.fetch_n(magnet, 1)
    }

    /// Ask for up to `frames` frames' worth at once — successive,
    /// non-overlapping windows, so a link with room for more than one packet in
    /// flight can use it. A slow link should stay at `1`: every id asked for is
    /// a reply someone else has to carry back.
    pub fn fetch_n(&mut self, magnet: &Id, frames: usize) -> Vec<Forward> {
        let window = self.want_window();
        self.missing(magnet, window.saturating_mul(frames.max(1)))
            .chunks(window)
            .map(|w| {
                let payload: Vec<u8> = w.iter().flatten().copied().collect();
                Forward::Flood {
                    except: NO_IFACE,
                    bytes: Envelope::new(ty::WANT, ZERO_DEST, 0, payload).wire(),
                }
            })
            .collect()
    }

    /// Request the chunks of every manifest we know but don't yet hold — the
    /// subscriber half of folder sync.
    pub fn fetch_all(&mut self) -> Vec<Forward> {
        let magnets: Vec<Id> = self.manifests.keys().copied().collect();
        let mut out = Vec::new();
        for m in magnets {
            out.append(&mut self.fetch(&m));
        }
        out
    }

    /// Every complete file we hold, as `(name, magnet)`, newest manifest per name
    /// winning (by envelope expiry).
    ///
    /// Names, not bytes. [`Node::complete_files`] assembles every file at once,
    /// which on a disk-backed store means pulling the whole store into RAM — an
    /// Android node is configured for 256 MB on disk and 8 MB resident, and
    /// assembling all of it defeats exactly that split. Callers that write files
    /// out should walk these and stream each one with [`Node::write_file_to`],
    /// which is what `foldersync::materialize` now does (S-028).
    pub fn complete_file_names(&self) -> Vec<(String, Id)> {
        let mut best: HashMap<String, (Id, u32)> = HashMap::new();
        for (magnet, m) in &self.manifests {
            if !self.has_file(magnet) {
                continue;
            }
            let exp = self.store.meta(magnet).map(|s| s.expiry).unwrap_or(0);
            best.entry(m.name.clone())
                .and_modify(|(id, e)| {
                    if exp > *e {
                        *id = *magnet;
                        *e = exp;
                    }
                })
                .or_insert((*magnet, exp));
        }
        best.into_iter().map(|(name, (magnet, _))| (name, magnet)).collect()
    }

    /// Every complete file we hold, as `(name, bytes)`, newest manifest per name
    /// winning (by envelope expiry).
    ///
    /// Assembles every file into memory at once. Prefer
    /// [`Node::complete_file_names`] plus [`Node::write_file_to`] when the result
    /// is going to disk; this is kept for small stores and for callers that
    /// genuinely want the bytes.
    pub fn complete_files(&self) -> Vec<(String, Vec<u8>)> {
        let mut best: HashMap<String, (Id, u32)> = HashMap::new();
        for (magnet, m) in &self.manifests {
            if !self.has_file(magnet) {
                continue;
            }
            let exp = self.store.meta(magnet).map(|s| s.expiry).unwrap_or(0);
            best.entry(m.name.clone())
                .and_modify(|(id, e)| {
                    if exp > *e {
                        *id = *magnet;
                        *e = exp;
                    }
                })
                .or_insert((*magnet, exp));
        }
        best.into_iter()
            .filter_map(|(name, (magnet, _))| self.file_bytes(&magnet).map(|b| (name, b)))
            .collect()
    }

    /// True once every chunk named by the manifest is in our store.
    /// Publish a file **sealed to `dest`'s prekey**: the bytes *and the file
    /// name* are encrypted, so relays carrying the chunks learn neither.
    ///
    /// Each chunk is encrypted **on its own** under a per-file key, and that key
    /// — with the real file name — travels sealed in the root manifest's header.
    /// So the recipient decrypts a chunk at a time straight to wherever it is
    /// putting the file, and a sealed file costs one chunk of memory instead of
    /// all of it. The manifest advertises the placeholder [`SEALED_FILE_NAME`].
    ///
    /// `None` if we haven't heard `dest`'s prekey yet (it arrives with their
    /// ANNOUNCE) — the caller can fall back to [`Node::publish_file`] and say so.
    pub fn publish_file_sealed(
        &mut self,
        name: &str,
        bytes: &[u8],
        dest: Addr,
        now: u32,
    ) -> Option<(Id, Vec<Forward>)> {
        let pk = self.peer_prekey(&dest)?;
        let mut key = [0u8; 32];
        OsRng.fill_bytes(&mut key);

        // Header (sealed to the recipient): the file key, then the real name.
        let nb = name.as_bytes();
        let nlen = nb.len().min(u16::MAX as usize);
        let mut hdr = Vec::with_capacity(34 + nlen);
        hdr.extend_from_slice(&key);
        hdr.extend_from_slice(&(nlen as u16).to_be_bytes());
        hdr.extend_from_slice(&nb[..nlen]);
        let sealed_hdr = seal(&hdr, &pk);

        // On a very small link the sealed header can fill the root by itself,
        // leaving no room for even one id. Say so, rather than minting a root
        // no link could carry — the caller can fall back to publishing in the
        // clear, which is a choice only they can make.
        if file::root_fanout(self.mtu, SEALED_FILE_NAME.len(), 0, sealed_hdr.len()) == 0 {
            return None;
        }

        Some(self.publish_object(SEALED_FILE_NAME, bytes, dest, now, Some(&key), sealed_hdr))
    }

    /// Open a sealed root's header with our prekey: `(file key, real name)`.
    /// `None` when it was sealed to someone else — which is most of what a relay
    /// carries, and it never learns more than that.
    pub(crate) fn open_sealed_header(&self, m: &file::Manifest) -> Option<([u8; 32], String)> {
        let opened = self.open(&m.sealed_hdr)?;
        if opened.len() < 34 {
            return None;
        }
        let mut key = [0u8; 32];
        key.copy_from_slice(&opened[..32]);
        let nlen = u16::from_be_bytes([opened[32], opened[33]]) as usize;
        if 34 + nlen > opened.len() {
            return None;
        }
        Some((key, String::from_utf8_lossy(&opened[34..34 + nlen]).to_string()))
    }

    /// Recover a complete file as `(name, bytes)`, decrypting it when it was
    /// sealed to us. `None` while parts are still missing, or when the file was
    /// sealed to someone else — we relay those without ever reading them.
    ///
    /// For anything large prefer [`Node::open_file_to`], which writes the file
    /// out as it decrypts instead of building the whole thing in memory.
    pub fn open_file(&self, magnet: &Id) -> Option<(String, Vec<u8>)> {
        let hint = self.manifests.get(magnet)?.total_len.min(1 << 20) as usize;
        let mut out = Vec::with_capacity(hint);
        let (name, _) = self.open_file_to(magnet, &mut out)?;
        Some((name, out))
    }

    /// The file's real name — decrypted out of the sealed header when it was
    /// sealed to us. `None` if we don't know the manifest, or it was sealed to
    /// someone else. Cheap for everything but the legacy whole-blob form: it
    /// never touches the chunks.
    pub fn file_name(&self, magnet: &Id) -> Option<String> {
        let m = self.manifests.get(magnet)?;
        if !m.sealed_hdr.is_empty() {
            return self.open_sealed_header(m).map(|(_, name)| name);
        }
        if m.name == SEALED_FILE_NAME {
            // Legacy: the name sits inside the sealed blob, so it costs a read.
            return self.open_file(magnet).map(|(name, _)| name);
        }
        Some(m.name.clone())
    }

    /// Write a complete file out as `(name, bytes written)`, decrypting it when
    /// it was sealed to us — **the form to prefer for anything large.**
    ///
    /// A sealed file is decrypted a chunk at a time on the way to `w`, so peak
    /// memory is one chunk however big the file is. `None` while parts are
    /// missing, or when it was sealed to someone else.
    pub fn open_file_to<W: std::io::Write>(&self, magnet: &Id, w: &mut W) -> Option<(String, u64)> {
        let m = self.manifests.get(magnet)?;

        // Sealed per chunk: decrypt on the way out.
        if !m.sealed_hdr.is_empty() {
            let (key, name) = self.open_sealed_header(m)?;
            let n = self.assemble(magnet, w, Some(&key))?;
            return Some((name, n));
        }
        // Not sealed at all: the bytes are the file.
        if m.name != SEALED_FILE_NAME {
            let n = self.assemble(magnet, w, None)?;
            return Some((m.name.clone(), n));
        }

        // Legacy: the whole file was sealed as one blob before chunking, so it
        // cannot be streamed — it has to be decrypted entire. Kept so files
        // published by older nodes still open.
        let raw = self.file_bytes(magnet)?;
        let inner = self.open(&raw)?;
        if inner.len() < 2 {
            return None;
        }
        let nlen = u16::from_be_bytes([inner[0], inner[1]]) as usize;
        if 2 + nlen > inner.len() {
            return None;
        }
        let name = String::from_utf8_lossy(&inner[2..2 + nlen]).to_string();
        let body = &inner[2 + nlen..];
        w.write_all(body).ok()?;
        Some((name, body.len() as u64))
    }

    /// The largest file [`Node::publish_file`] can announce at this node's MTU.
    ///
    /// A manifest is one envelope, so a single one can only name so many chunks
    /// (~94 KB of file at a 1400-byte MTU). Interior manifests lift that: each
    /// level multiplies capacity by the interior fan-out, up to
    /// [`file::MAX_DEPTH`] levels — some terabytes, i.e. no practical protocol
    /// limit. What actually bounds a transfer now is the store budget at each
    /// hop and what the slowest bridge on the path is willing to carry, not the
    /// manifest.
    ///
    /// Sealing (`publish_file_sealed`) grows the payload by ~48 bytes plus the
    /// file name, so leave a little headroom under this figure.
    pub fn max_file_bytes(&self) -> usize {
        let chunk = self.mtu.saturating_sub(64).max(1);
        // Allow for a long file name in the signed root.
        let mut ids = file::root_fanout(self.mtu, 96, file::MAX_DEPTH, 0);
        let fanout = file::interior_fanout(self.mtu);
        for _ in 0..file::MAX_DEPTH {
            ids = ids.saturating_mul(fanout);
        }
        ids.saturating_mul(chunk)
    }

    /// The largest file whose manifest fits a **single** envelope — the point
    /// past which [`Node::publish_file`] starts building a tree. Below this a
    /// file is announced by one manifest, exactly as it was before trees
    /// existed, and one round trip is enough to learn every chunk id.
    pub fn max_flat_file_bytes(&self) -> usize {
        let chunk = self.mtu.saturating_sub(64).max(1);
        file::root_fanout(self.mtu, 96, 0, 0) * chunk
    }

    /// The ceiling on everything this node holds at once, files included.
    pub fn store_budget(&self) -> usize {
        self.max_store_bytes
    }

    /// The largest file this node can publish and still serve from its own
    /// store — **the limit an application should actually enforce.**
    ///
    /// Since manifests grew into trees, the protocol stopped being what bounds a
    /// file; the store did. Every chunk lives there as its own envelope, so a
    /// file costs a little more than its own length, and a node that spent its
    /// whole budget on one file would have nothing left to relay with. Hence
    /// half the budget, not all of it. Raise it with [`Node::set_store_budget`].
    pub fn max_storable_file_bytes(&self) -> usize {
        let chunk = self.mtu.saturating_sub(64).max(1);
        // A chunk envelope costs about a whole frame once its header, file id
        // and index are counted, so charge the file a frame per chunk.
        (self.max_store_bytes / 2 / self.mtu.max(1)) * chunk
    }

    /// Every file we hold a manifest for: `(magnet, advertised name, total
    /// bytes, chunks held, chunks total)` — a transfer list with progress.
    /// Chunks held is a *lower bound* while the tree is still resolving: chunks
    /// under an interior node we haven't got yet are not yet nameable, so they
    /// cannot be counted. Costs one tree walk per file, so poll it at UI rates,
    /// not per packet.
    pub fn files(&self) -> Vec<(Id, String, u64, u32, u32)> {
        self.manifests
            .iter()
            .map(|(magnet, m)| {
                let total = if m.chunk_size == 0 {
                    m.count
                } else {
                    m.total_len.div_ceil(m.chunk_size as u64) as u32
                };
                let mut have = 0u32;
                self.walk_tree(m, &mut |_, depth, held| {
                    if depth == 0 && held {
                        have += 1;
                    }
                    true
                });
                (*magnet, m.name.clone(), m.total_len, have, total.max(1))
            })
            .collect()
    }

    pub fn has_file(&self, magnet: &Id) -> bool {
        let Some(root) = self.manifests.get(magnet) else {
            return false;
        };
        let mut complete = true;
        self.walk_tree(root, &mut |_, _, held| {
            complete = held;
            held // the first gap ends the walk
        });
        complete
    }

    /// Write the file's raw bytes out to `w`, chunk by chunk, returning the
    /// count. `None` if any part is missing — or if the file is sealed, since
    /// its raw bytes are ciphertext and its length is the plaintext's; use
    /// [`Node::open_file_to`] for those.
    ///
    /// This is the form to prefer for anything large: only one chunk is in
    /// memory at a time, so a file bounded by disk is not also bounded by RAM.
    pub fn write_file_to<W: std::io::Write>(&self, magnet: &Id, w: &mut W) -> Option<u64> {
        if !self.manifests.get(magnet)?.sealed_hdr.is_empty() {
            return None;
        }
        self.assemble(magnet, w, None)
    }

    /// Walk the leaves in file order, writing each chunk out as it goes and
    /// decrypting first when `key` is set. One chunk is in memory at a time,
    /// which is what keeps a large file — sealed or not — off the heap.
    fn assemble<W: std::io::Write>(&self, magnet: &Id, w: &mut W, key: Option<&[u8; 32]>) -> Option<u64> {
        let root = self.manifests.get(magnet)?;
        let total = root.total_len;
        let mut written = 0u64;
        let mut ok = true;
        self.walk_tree(root, &mut |id, depth, held| {
            if !held {
                ok = false;
                return false;
            }
            if depth != 0 {
                return true; // an interior node carries ids, not bytes
            }
            let Some(wire) = self.store.wire(id) else {
                ok = false;
                return false;
            };
            let Ok((ce, _)) = Envelope::decode(&wire) else {
                ok = false;
                return false;
            };
            if ce.payload.len() < 21 {
                ok = false; // not a well-formed chunk
                return false;
            }
            // The chunk carries the index it was encrypted under, and the id
            // that named it is a hash of those very bytes, so it is as
            // trustworthy as the chunk itself.
            let index = u32::from_be_bytes([ce.payload[17], ce.payload[18], ce.payload[19], ce.payload[20]]);
            let plain;
            let body = match key {
                Some(k) => match chunk_open(&ce.payload[21..], k, index) {
                    Some(p) => {
                        plain = p;
                        &plain[..]
                    }
                    None => {
                        ok = false;
                        return false;
                    }
                },
                None => &ce.payload[21..],
            };
            let take = total.saturating_sub(written).min(body.len() as u64) as usize;
            if w.write_all(&body[..take]).is_err() {
                ok = false;
                return false;
            }
            written += take as u64;
            true
        });
        (ok && written == total).then_some(written)
    }

    /// Reassemble the file, or `None` if a part is still missing. Every chunk
    /// is content-verified for free: we only count it as present if the store
    /// holds an envelope whose ID equals the one its parent named, and the root
    /// that anchors those names is signed.
    pub fn file_bytes(&self, magnet: &Id) -> Option<Vec<u8>> {
        let root = self.manifests.get(magnet)?;
        // `total_len` comes off the wire, so reserve against it only up to a
        // sane bound — a manifest claiming u64::MAX must not be able to abort us
        // on the allocation. Beyond the hint the Vec just grows.
        let hint = root.total_len.min(1 << 20) as usize;
        let mut out = Vec::with_capacity(hint);
        self.write_file_to(magnet, &mut out)?;
        Some(out)
    }

    // ---- mix mode (§9) ---------------------------------------------------

    /// If `e` is an onion layer sealed to us, peel it: open the payload, confirm
    /// the `'O'` marker, and return the inner envelope's wire bytes (padding
    /// stripped by self-delimiting decode). A mix re-injects this as its own
    /// traffic. `None` if it isn't an onion for us.
    pub fn onion_peel(&self, e: &Envelope) -> Option<Vec<u8>> {
        let opened = self.open(&e.payload)?;
        if opened.first() != Some(&mix::ONION_TAG) {
            return None;
        }
        let (_, n) = Envelope::decode(&opened[1..]).ok()?;
        Some(opened[1..1 + n].to_vec())
    }
}
