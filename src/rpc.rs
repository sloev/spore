pub const REQUEST_TAG: u8 = 0x02;
pub const RESPONSE_TAG: u8 = 0x03;

#[derive(Clone, Debug)]
pub struct Request {
    pub method: String,
    pub path: String,
    pub body: Vec<u8>,
}
#[derive(Clone, Debug)]
pub struct Response {
    pub status: u16,
    pub body: Vec<u8>,
}

// [0x02][req_id:8][mlen:1][method][plen:2][path][body]
pub(crate) fn encode_request(id: u64, r: &Request) -> Vec<u8> {
    let m = r.method.as_bytes();
    let m = &m[..m.len().min(255)];
    let pa = r.path.as_bytes();
    let mut v = Vec::with_capacity(12 + m.len() + pa.len() + r.body.len());
    v.push(REQUEST_TAG);
    v.extend_from_slice(&id.to_be_bytes());
    v.push(m.len() as u8);
    v.extend_from_slice(m);
    v.extend_from_slice(&(pa.len() as u16).to_be_bytes());
    v.extend_from_slice(pa);
    v.extend_from_slice(&r.body);
    v
}
pub(crate) fn decode_request(p: &[u8]) -> Option<(u64, Request)> {
    if p.first() != Some(&REQUEST_TAG) || p.len() < 12 {
        return None;
    }
    let id = u64::from_be_bytes(p[1..9].try_into().ok()?);
    let mut o = 9;
    let mlen = p[o] as usize;
    o += 1;
    if o + mlen + 2 > p.len() {
        return None;
    }
    let method = String::from_utf8_lossy(&p[o..o + mlen]).into_owned();
    o += mlen;
    let plen = u16::from_be_bytes([p[o], p[o + 1]]) as usize;
    o += 2;
    if o + plen > p.len() {
        return None;
    }
    let path = String::from_utf8_lossy(&p[o..o + plen]).into_owned();
    o += plen;
    Some((id, Request { method, path, body: p[o..].to_vec() }))
}

// [0x03][req_id:8][status:2][body]
pub(crate) fn encode_response(id: u64, r: &Response) -> Vec<u8> {
    let mut v = Vec::with_capacity(11 + r.body.len());
    v.push(RESPONSE_TAG);
    v.extend_from_slice(&id.to_be_bytes());
    v.extend_from_slice(&r.status.to_be_bytes());
    v.extend_from_slice(&r.body);
    v
}
pub(crate) fn decode_response(p: &[u8]) -> Option<(u64, Response)> {
    if p.first() != Some(&RESPONSE_TAG) || p.len() < 11 {
        return None;
    }
    let id = u64::from_be_bytes(p[1..9].try_into().ok()?);
    let status = u16::from_be_bytes([p[9], p[10]]);
    Some((id, Response { status, body: p[11..].to_vec() }))
}
