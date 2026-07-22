//! Secure Scuttlebutt (SSB) bridge — carry SPORE envelopes on an append-only
//! gossip log. SSB is offline-first friend-to-friend replication: exactly the
//! store-and-forward shape SPORE already is, so the two compose naturally.
//!
//! An envelope becomes the `content` of an SSB message:
//!
//! ```json
//! { "type": "spore-v1", "envelope": "<base64 envelope wire>" }
//! ```
//!
//! ## Portable vs. platform
//! The `content` codec ([`wrap`] / [`unwrap`]) and a folder-log runner
//! ([`run`]) are here and tested. The one platform-specific piece is talking to
//! a real SSB peer — the secret-handshake + muxrpc replication to a pub/room, or
//! an `ssb-server`/`go-ssb` unix socket. Point [`run`] at a directory that your
//! SSB client imports/exports (many keep a plaintext message log), and the two
//! meshes exchange traffic with no protocol changes on either side.

const TYPE: &str = "spore-v1";

/// Wrap an envelope's wire bytes as an SSB message `content` object (JSON).
pub fn wrap(env_wire: &[u8]) -> String {
    format!("{{\"type\":\"{}\",\"envelope\":\"{}\"}}", TYPE, b64_encode(env_wire))
}

/// Recover envelope bytes from an SSB message. Accepts either a bare `content`
/// object or a full SSB message with a nested `content` — we just find the
/// `"envelope"` field and decode it, after checking the `spore-v1` type.
pub fn unwrap(json: &str) -> Option<Vec<u8>> {
    if !json.contains(TYPE) {
        return None;
    }
    let key = "\"envelope\"";
    let k = json.find(key)?;
    let rest = &json[k + key.len()..];
    let q1 = rest.find('"')?;
    let after = &rest[q1 + 1..];
    let q2 = after.find('"')?;
    b64_decode(&after[..q2])
}

/// Folder-log runner: import new `*.ssb` messages (receiving) and write outbound
/// forwards as new `*.ssb` messages (sending). Interoperates with any SSB client
/// that mirrors its log to that directory.
#[cfg(not(target_arch = "wasm32"))]
pub fn run(
    hub: super::hub::Shared,
    iface: crate::Iface,
    rx: std::sync::mpsc::Receiver<crate::Forward>,
    dir: std::path::PathBuf,
) -> std::io::Result<()> {
    use crate::{Envelope, Forward};
    use std::collections::HashSet;
    use std::fs;
    let mut known: HashSet<String> = HashSet::new();
    println!("  [ssb] iface {iface} log {}", dir.display());
    loop {
        if dir.exists() {
            for entry in fs::read_dir(&dir)? {
                let path = entry?.path();
                if path.extension().and_then(|e| e.to_str()) != Some("ssb") {
                    continue;
                }
                let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("").to_string();
                if !known.insert(name) {
                    continue;
                }
                if let Ok(text) = fs::read_to_string(&path) {
                    if let Some(env) = unwrap(&text) {
                        hub.on_rx(iface, &env, None);
                    }
                }
            }
        }
        while let Ok(f) = rx.try_recv() {
            let bytes = match f {
                Forward::Flood { bytes, .. } => bytes,
                Forward::Directed { bytes, .. } => bytes,
            };
            if let Ok((e, _)) = Envelope::decode(&bytes) {
                let id: String = e.id().iter().map(|b| format!("{b:02x}")).collect();
                let name = format!("{id}.ssb");
                known.insert(name.clone());
                fs::create_dir_all(&dir)?;
                let path = dir.join(&name);
                if !path.exists() {
                    fs::write(path, wrap(&bytes))?;
                }
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
}

// --- base64 (standard alphabet, padded) — self-contained, no dependency ---
const B64: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn b64_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(B64[((n >> 18) & 63) as usize] as char);
        out.push(B64[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 { B64[((n >> 6) & 63) as usize] as char } else { '=' });
        out.push(if chunk.len() > 2 { B64[(n & 63) as usize] as char } else { '=' });
    }
    out
}

fn b64_val(c: u8) -> Option<u32> {
    match c {
        b'A'..=b'Z' => Some((c - b'A') as u32),
        b'a'..=b'z' => Some((c - b'a' + 26) as u32),
        b'0'..=b'9' => Some((c - b'0' + 52) as u32),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

fn b64_decode(s: &str) -> Option<Vec<u8>> {
    let bytes: Vec<u8> = s.bytes().filter(|&c| c != b'=' && !c.is_ascii_whitespace()).collect();
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    for chunk in bytes.chunks(4) {
        let mut n = 0u32;
        for (i, &c) in chunk.iter().enumerate() {
            n |= b64_val(c)? << (18 - 6 * i);
        }
        out.push((n >> 16) as u8);
        if chunk.len() > 2 {
            out.push((n >> 8) as u8);
        }
        if chunk.len() > 3 {
            out.push(n as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::*;

    #[test]
    fn base64_roundtrip() {
        for len in 0..64usize {
            let data: Vec<u8> = (0..len).map(|i| (i.wrapping_mul(97)) as u8).collect();
            assert_eq!(b64_decode(&b64_encode(&data)).as_deref(), Some(&data[..]), "len {len}");
        }
    }

    #[test]
    fn base64_known_vector() {
        assert_eq!(b64_encode(b"SPORE"), "U1BPUkU=");
        assert_eq!(b64_decode("U1BPUkU=").as_deref(), Some(&b"SPORE"[..]));
    }

    #[test]
    fn envelope_wrap_unwrap() {
        let mut e = Envelope::new(ty::DATA, ZERO_DEST, 1_700_000_000, b"scuttle this".to_vec());
        e.flags |= fl::SIGNED;
        let wire = e.wire();
        let content = wrap(&wire);
        assert!(content.contains("spore-v1"));
        assert_eq!(unwrap(&content).as_deref(), Some(&wire[..]));
    }

    #[test]
    fn unwrap_full_ssb_message() {
        let wire = vec![1u8, 2, 3, 4, 5];
        let inner = wrap(&wire);
        // A full SSB message nests content; unwrap should still find the envelope.
        let full = format!(
            "{{\"previous\":\"%abc.sha256\",\"author\":\"@xyz.ed25519\",\"sequence\":42,\"content\":{inner}}}"
        );
        assert_eq!(unwrap(&full).as_deref(), Some(&wire[..]));
    }
}
