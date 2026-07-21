//! Meshtastic frame codec: wrap a SPORE envelope as a Meshtastic `MeshPacket`
//! (portnum 256 = PRIVATE_APP, spec Page 2) and read it back. Hand-rolled
//! protobuf so there's no build-time codegen; portable, so a browser bridging to
//! Meshtastic over WebSocket uses the same code as a native UDP bridge.
//!
//! CAVEATS — this is a template, not hardware-verified here:
//! - Field numbers follow Meshtastic `mesh.proto`; confirm them against the
//!   firmware you target (they're all in one place below).
//! - Only the unencrypted `decoded` payload variant is handled. An *encrypted*
//!   channel puts ciphertext in field 5 (AES-CTR with the channel key); to
//!   interoperate there, add that key. Use an unencrypted channel to start.
//! - The LAN multicast group/port for the WiFi-UDP broadcast feature are the
//!   values below; verify for your firmware version.

/// PRIVATE_APP portnum SPORE rides on.
pub const PORT_PRIVATE_APP: u32 = 256;
/// Meshtastic broadcast node number.
pub const BROADCAST: u32 = 0xFFFF_FFFF;
/// Hop limit stamped on outgoing packets.
pub const DEFAULT_HOP_LIMIT: u32 = 3;
/// LAN multicast group + port of Meshtastic's WiFi-UDP broadcast feature.
pub const UDP_GROUP: [u8; 4] = [224, 0, 0, 69];
pub const UDP_PORT: u16 = 4403;

// --- minimal protobuf writer ---
fn put_varint(v: &mut Vec<u8>, mut n: u64) {
    loop {
        let b = (n & 0x7f) as u8;
        n >>= 7;
        if n != 0 {
            v.push(b | 0x80);
        } else {
            v.push(b);
            break;
        }
    }
}
fn put_tag(v: &mut Vec<u8>, field: u32, wire: u8) {
    put_varint(v, ((field as u64) << 3) | wire as u64);
}
fn put_uint(v: &mut Vec<u8>, field: u32, val: u64) {
    put_tag(v, field, 0);
    put_varint(v, val);
}
fn put_bytes(v: &mut Vec<u8>, field: u32, data: &[u8]) {
    put_tag(v, field, 2);
    put_varint(v, data.len() as u64);
    v.extend_from_slice(data);
}
fn put_fixed32(v: &mut Vec<u8>, field: u32, val: u32) {
    put_tag(v, field, 5);
    v.extend_from_slice(&val.to_le_bytes());
}
fn get_varint(buf: &[u8], mut o: usize) -> Option<(u64, usize)> {
    let (mut val, mut shift) = (0u64, 0u32);
    loop {
        if o >= buf.len() || shift >= 64 {
            return None;
        }
        let b = buf[o];
        o += 1;
        val |= ((b & 0x7f) as u64) << shift;
        if b & 0x80 == 0 {
            return Some((val, o));
        }
        shift += 7;
    }
}

// MeshPacket fields (mesh.proto): from=1 to=2 decoded=4 encrypted=5 id=6
//                                 hop_limit=9
// Data fields:                    portnum=1 payload=2

/// Wrap `env_wire` as a Meshtastic packet from `from_node` to `to_node`
/// (use `BROADCAST` for a flood), tagged PRIVATE_APP.
pub fn encode(env_wire: &[u8], from_node: u32, to_node: u32, packet_id: u32) -> Vec<u8> {
    let mut data = Vec::new();
    put_uint(&mut data, 1, PORT_PRIVATE_APP as u64); // portnum
    put_bytes(&mut data, 2, env_wire); // payload

    let mut pkt = Vec::new();
    put_uint(&mut pkt, 1, from_node as u64);
    put_uint(&mut pkt, 2, to_node as u64);
    put_bytes(&mut pkt, 4, &data); // decoded
    put_fixed32(&mut pkt, 6, packet_id);
    put_uint(&mut pkt, 9, DEFAULT_HOP_LIMIT as u64);
    pkt
}

/// Read a Meshtastic packet's `(from, portnum, payload)`. `None` if it's
/// malformed or carries only an encrypted variant we can't open.
pub fn decode(frame: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let mut from = 0u32;
    let mut decoded: Option<&[u8]> = None;
    let mut o = 0;
    while o < frame.len() {
        let (tag, no) = get_varint(frame, o)?;
        o = no;
        let (field, wire) = ((tag >> 3) as u32, (tag & 7) as u8);
        match wire {
            0 => {
                let (v, no) = get_varint(frame, o)?;
                o = no;
                if field == 1 {
                    from = v as u32;
                }
            }
            2 => {
                let (len, no) = get_varint(frame, o)?;
                let end = no + len as usize;
                if end > frame.len() {
                    return None;
                }
                if field == 4 {
                    decoded = Some(&frame[no..end]);
                }
                o = end;
            }
            5 => o += 4,
            1 => o += 8,
            _ => return None,
        }
        if o > frame.len() {
            return None;
        }
    }
    let d = decoded?;
    let (mut portnum, mut payload) = (0u32, Vec::new());
    let mut o = 0;
    while o < d.len() {
        let (tag, no) = get_varint(d, o)?;
        o = no;
        let (field, wire) = ((tag >> 3) as u32, (tag & 7) as u8);
        match wire {
            0 => {
                let (v, no) = get_varint(d, o)?;
                o = no;
                if field == 1 {
                    portnum = v as u32;
                }
            }
            2 => {
                let (len, no) = get_varint(d, o)?;
                let end = no + len as usize;
                if end > d.len() {
                    return None;
                }
                if field == 2 {
                    payload = d[no..end].to_vec();
                }
                o = end;
            }
            5 => o += 4,
            1 => o += 8,
            _ => return None,
        }
    }
    Some((from, portnum, payload))
}
