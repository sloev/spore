//! Invites — "here's how to reach me", as one short string.
//!
//! Meeting someone should not mean reading sixteen hex characters aloud. An
//! invite packs the three things a newcomer needs into a line you can show as a
//! QR code, paste into a chat, or read over the radio:
//!
//! * **address** — who you are (8 bytes),
//! * **name** — what you'd like to be called (a *suggestion*; the scanner
//!   assigns the petname it actually trusts),
//! * **bridges** — the links you're reachable on, so they can join the same mesh
//!   rather than merely knowing your address.
//!
//! ```text
//! spore:1c9a4f0e77b32d51?n=Jo&b=ws%3Awss%3A%2F%2Frelay.example%2Fspore&b=wt%3Aspore%2Fpublic&k=3f2a
//! ```
//!
//! `k` is a short checksum over the whole invite, so a mistyped or truncated
//! string is rejected rather than silently producing a wrong address.
//!
//! ## Trust
//! An invite is **not** authenticated — anyone can mint one claiming any name,
//! and a scanned invite's bridges point wherever its author chose. Treat the
//! name as a hint and **confirm the bridges before joining them**: a hostile
//! invite could otherwise steer a node onto a relay of the attacker's choosing.
//! The address is self-certifying in use — messages from it must verify against
//! it — so a forged invite gets you a wrong contact, never a forged identity.

use crate::Addr;
use sha2::{Digest, Sha256};

/// What an invite carries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Invite {
    pub addr: Addr,
    /// The name the sender suggests (may be empty).
    pub name: String,
    /// Bridge specs in the daemon's `kind:value` form, e.g. `ws:wss://…`,
    /// `wt:spore/public`, `nostr:wss://…`, `tcp:10.0.0.5:7373`.
    pub bridges: Vec<String>,
}

const PREFIX: &str = "spore:";

fn pct_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(*b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn pct_decode(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            if i + 2 >= b.len() {
                return None;
            }
            let hex = std::str::from_utf8(&b[i + 1..i + 3]).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

fn hex8(a: &Addr) -> String {
    a.iter().map(|b| format!("{b:02x}")).collect()
}

// A 2-byte checksum over the invite's meaning (not its spelling).
fn checksum(addr: &Addr, name: &str, bridges: &[String]) -> String {
    let mut h = Vec::new();
    h.extend_from_slice(addr);
    h.extend_from_slice(name.as_bytes());
    for b in bridges {
        h.push(0);
        h.extend_from_slice(b.as_bytes());
    }
    let d = Sha256::digest(&h);
    format!("{:02x}{:02x}", d[0], d[1])
}

/// Render an invite as one shareable line.
pub fn encode(addr: &Addr, name: &str, bridges: &[String]) -> String {
    let mut s = format!("{PREFIX}{}", hex8(addr));
    s.push_str(&format!("?n={}", pct_encode(name)));
    for b in bridges {
        s.push_str(&format!("&b={}", pct_encode(b)));
    }
    s.push_str(&format!("&k={}", checksum(addr, name, bridges)));
    s
}

/// Parse an invite. `None` if it isn't one, is malformed, or fails its checksum
/// (a truncated or mistyped string must never yield a plausible-looking address).
pub fn decode(text: &str) -> Option<Invite> {
    let t = text.trim();
    let rest = t.strip_prefix(PREFIX)?;
    let (addr_hex, query) = match rest.split_once('?') {
        Some((a, q)) => (a, q),
        None => (rest, ""),
    };
    if addr_hex.len() != 16 {
        return None;
    }
    let mut addr = [0u8; 8];
    for (i, byte) in addr.iter_mut().enumerate() {
        *byte = u8::from_str_radix(addr_hex.get(i * 2..i * 2 + 2)?, 16).ok()?;
    }

    let mut name = String::new();
    let mut bridges = Vec::new();
    let mut given_k: Option<String> = None;
    for pair in query.split('&').filter(|p| !p.is_empty()) {
        let (k, v) = pair.split_once('=')?;
        let v = pct_decode(v)?;
        match k {
            "n" => name = v.chars().filter(|c| !c.is_control()).take(32).collect(),
            "b" => {
                if bridges.len() < 8 {
                    bridges.push(v.chars().filter(|c| !c.is_control()).take(128).collect())
                }
            }
            "k" => given_k = Some(v),
            _ => {} // forward-compatible: ignore unknown keys
        }
    }
    // A checksum is required once present in the format; reject if it disagrees.
    if let Some(k) = given_k {
        if k != checksum(&addr, &name, &bridges) {
            return None;
        }
    }
    Some(Invite { addr, name, bridges })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invite_roundtrip_with_awkward_text() {
        let addr: Addr = [0x1c, 0x9a, 0x4f, 0x0e, 0x77, 0xb3, 0x2d, 0x51];
        let bridges = vec!["ws:wss://relay.example/spore".to_string(), "wt:spore/public".to_string()];
        // A name with a space, an ampersand and an emoji — all must survive.
        let s = encode(&addr, "Jo & the 🍄", &bridges);
        let got = decode(&s).expect("round-trips");
        assert_eq!(got.addr, addr);
        assert_eq!(got.name, "Jo & the 🍄");
        assert_eq!(got.bridges, bridges);
        // The separators must not leak into the fields.
        assert!(!s.contains("Jo & the"), "name is percent-encoded: {s}");
    }

    #[test]
    fn bad_invites_are_rejected_not_guessed() {
        let addr: Addr = [1, 2, 3, 4, 5, 6, 7, 8];
        let good = encode(&addr, "a", &["ws:x".into()]);

        assert!(decode("").is_none());
        assert!(decode("https://example.com").is_none());
        assert!(decode("spore:tooshort").is_none());
        assert!(decode("spore:zzzzzzzzzzzzzzzz?n=a").is_none(), "non-hex address");
        // A single flipped character must fail the checksum, not decode wrongly.
        let tampered = good.replace("spore:0102", "spore:0103");
        assert!(decode(&tampered).is_none(), "checksum catches a changed address");
        // Truncation, too.
        assert!(decode(&good[..good.len() - 3]).is_none(), "checksum catches truncation");
    }

    #[test]
    fn invite_without_extras_still_works() {
        let addr: Addr = [9; 8];
        let got = decode(&encode(&addr, "", &[])).unwrap();
        assert_eq!(got.addr, addr);
        assert!(got.name.is_empty() && got.bridges.is_empty());
        // Unknown keys are ignored, so older apps read newer invites.
        let fwd = format!("spore:{}?n=&x=future&k={}", "0909090909090909", checksum(&addr, "", &[]));
        assert_eq!(decode(&fwd).unwrap().addr, addr);
    }
}
