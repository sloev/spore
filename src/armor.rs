use super::*;
const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

fn b32enc(data: &[u8]) -> String {
    let mut out = String::new();
    let mut buf = 0u32;
    let mut bits = 0u32;
    for &b in data {
        buf = (buf << 8) | b as u32;
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(A[((buf >> bits) & 31) as usize] as char);
        }
    }
    if bits > 0 {
        out.push(A[((buf << (5 - bits)) & 31) as usize] as char);
    }
    out
}
fn b32dec(s: &str) -> Option<Vec<u8>> {
    let mut buf = 0u32;
    let mut bits = 0u32;
    let mut out = Vec::new();
    for c in s.chars() {
        if c.is_whitespace() {
            continue;
        }
        let u = c.to_ascii_uppercase();
        let v = A.iter().position(|&x| x as char == u)? as u32;
        buf = (buf << 5) | v;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            out.push(((buf >> bits) & 0xff) as u8);
        }
    }
    Some(out)
}

pub fn wrap(env_wire: &[u8]) -> String {
    let d = Sha256::digest(env_wire);
    format!("~S1.{}.{}~", b32enc(env_wire), b32enc(&d[..4]))
}
/// Recover envelope bytes from armor found anywhere in `text`.
pub fn unwrap(text: &str) -> Option<Vec<u8>> {
    let start = text.find("~S1.")? + 4;
    let end = text[start..].find('~')? + start;
    let body = &text[start..end];
    let (b32, ck) = body.rsplit_once('.')?;
    let env = b32dec(b32)?;
    let want = b32dec(ck)?;
    let got = Sha256::digest(&env);
    if got[..4] == want[..] {
        Some(env)
    } else {
        None
    }
}
