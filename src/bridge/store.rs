//! A shared-store folder: envelopes are files named `<hexid>.spore`. The folder
//! *is* a persistent INV — reading it is receiving, writing to it is sending.
//! Backs USB sneakernet, Syncthing, NFS, Dropbox.

use crate::*;
use std::path::Path;
use std::{fs, io};

/// Folder bridge runner: periodically import new `*.spore` files (receiving) and
/// write outbound forwards into the folder (sending). Drop it in Syncthing or on
/// a USB stick and the folder becomes a link between two machines.
pub fn run(
    hub: super::hub::Shared,
    iface: Iface,
    rx: std::sync::mpsc::Receiver<Forward>,
    dir: std::path::PathBuf,
) -> io::Result<()> {
    use std::collections::HashSet;
    let mut known: HashSet<String> = HashSet::new();
    println!("  [folder] iface {iface} syncing {}", dir.display());
    loop {
        // Import new files (reading = receiving).
        if dir.exists() {
            for entry in fs::read_dir(&dir)? {
                let path = entry?.path();
                if path.extension().and_then(|e| e.to_str()) != Some("spore") {
                    continue;
                }
                let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("").to_string();
                if !known.insert(name) {
                    continue; // already imported / we wrote it
                }
                if let Ok(bytes) = fs::read(&path) {
                    hub.on_rx(iface, &bytes, None);
                }
            }
        }
        // Write outbound forwards into the folder (writing = sending).
        while let Ok(f) = rx.try_recv() {
            let bytes = match f {
                Forward::Flood { bytes, .. } => bytes,
                Forward::Directed { bytes, .. } => bytes,
            };
            if let Ok((e, _)) = Envelope::decode(&bytes) {
                let name = filename(&e.id());
                known.insert(name.clone());
                fs::create_dir_all(&dir)?;
                let path = dir.join(&name);
                if !path.exists() {
                    fs::write(path, &bytes)?;
                }
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
}

pub fn filename(id: &Id) -> String {
    let mut s = String::with_capacity(38);
    for b in id {
        s.push_str(&format!("{b:02x}"));
    }
    s.push_str(".spore");
    s
}

/// Write one envelope into `dir` if not already present. Returns whether it was
/// newly written.
pub fn export(dir: &Path, e: &Envelope) -> io::Result<bool> {
    fs::create_dir_all(dir)?;
    let path = dir.join(filename(&e.id()));
    if path.exists() {
        return Ok(false);
    }
    fs::write(path, e.wire())?;
    Ok(true)
}

/// Write the node's whole store into `dir`. Returns how many were new.
pub fn export_all(dir: &Path, node: &Node) -> io::Result<usize> {
    fs::create_dir_all(dir)?;
    let mut n = 0;
    for (id, wire) in node.store_wires() {
        let path = dir.join(filename(&id));
        if !path.exists() {
            fs::write(path, wire)?;
            n += 1;
        }
    }
    Ok(n)
}

/// Feed every `*.spore` file in `dir` to the node (reading = receiving).
/// Returns the aggregate `Rx` (delivered + forwards).
pub fn import(dir: &Path, node: &mut Node, iface: Iface, now: u32) -> io::Result<Rx> {
    let mut rx = Rx::default();
    if !dir.exists() {
        return Ok(rx);
    }
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.extension().and_then(|e| e.to_str()) != Some("spore") {
            continue;
        }
        let bytes = fs::read(&path)?;
        let mut r = node.on_rx(&bytes, iface, None, now);
        rx.delivered.append(&mut r.delivered);
        rx.forwards.append(&mut r.forwards);
    }
    Ok(rx)
}
