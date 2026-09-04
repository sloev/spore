//! Meshtastic frame codec: wrap a SPORE envelope as a Meshtastic `MeshPacket`
//! (portnum 256 = PRIVATE_APP, spec Part II) and read it back. Hand-rolled
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
/// What a Meshtastic link will carry of *other people's file chunks*, per second
/// — see [`crate::bridge::hub::Hub::register_limited`].
///
/// A conservative default, not a measurement: LoRa is duty-cycle limited and
/// shared by everyone in earshot, and a 237-byte packet is the whole channel for
/// a moment. Messages, announces and manifests are never counted against it, so
/// the mesh stays fully conversational; only bulk is paced. Raise it with
/// `Hub::set_bulk_budget` if your region and preset can afford more.
pub const BULK_BYTES_PER_SEC: u32 = 32;

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
                // `len` is a varint off the air, so it reaches u64::MAX. Plain
                // addition overflows — a panic wherever overflow checks are on,
                // which includes every `cargo build`/`cargo run` without
                // `--release`, and in release wraps to a bogus range instead.
                // `from_radio_packet` below already does this correctly; `decode`
                // did not.
                let end = no.checked_add(len as usize)?;
                if end > frame.len() {
                    return None;
                }
                if field == 4 {
                    decoded = Some(&frame[no..end]);
                }
                o = end;
            }
            5 => o = o.checked_add(4)?,
            1 => o = o.checked_add(8)?,
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
                // Same checked arithmetic as the outer loop above: this is a
                // second protobuf parse, over a sub-message whose lengths are just
                // as attacker-chosen as the frame's.
                let end = no.checked_add(len as usize)?;
                if end > d.len() {
                    return None;
                }
                if field == 2 {
                    payload = d[no..end].to_vec();
                }
                o = end;
            }
            5 => o = o.checked_add(4)?,
            1 => o = o.checked_add(8)?,
            _ => return None,
        }
    }
    Some((from, portnum, payload))
}

/// Meshtastic WiFi-UDP transport: the medium-specific `recv`/`send` only; the
/// snooping, resolution and relay come from `driver::run_datagram`.
#[cfg(not(target_arch = "wasm32"))]
struct Mesh {
    sock: std::net::UdpSocket,
    dst: std::net::SocketAddrV4,
    my_node: u32,
    rng: u32,
}

#[cfg(not(target_arch = "wasm32"))]
impl Mesh {
    fn next_id(&mut self) -> u32 {
        self.rng = self.rng.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        self.rng
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl crate::bridge::driver::DatagramTransport for Mesh {
    type Addr = u32;

    fn mtu(&self) -> Option<usize> {
        Some(200) // fit the LoRa payload budget so SPORE auto-fragments
    }

    fn recv(&mut self) -> std::io::Result<Option<(Vec<u8>, Option<u32>)>> {
        use std::io::ErrorKind;
        let mut buf = [0u8; 1024];
        loop {
            match self.sock.recv_from(&mut buf) {
                Ok((n, _peer)) => match decode(&buf[..n]) {
                    Some((from_node, portnum, payload)) if portnum == PORT_PRIVATE_APP => {
                        return Ok(Some((payload, Some(from_node))));
                    }
                    _ => continue, // not ours — keep reading until timeout
                },
                Err(e) if e.kind() == ErrorKind::WouldBlock || e.kind() == ErrorKind::TimedOut => {
                    return Ok(None);
                }
                Err(e) => return Err(e),
            }
        }
    }

    fn send(&mut self, to: Option<&u32>, env: &[u8]) -> std::io::Result<()> {
        let node = to.copied().unwrap_or(BROADCAST);
        let id = self.next_id();
        let frame = encode(env, self.my_node, node, id);
        self.sock.send_to(&frame, self.dst)?;
        Ok(())
    }
}

/// Join the Meshtastic WiFi-UDP multicast group and bridge the LoRa mesh.
#[cfg(not(target_arch = "wasm32"))]
pub fn run(
    hub: crate::bridge::hub::Shared,
    iface: crate::Iface,
    rx: std::sync::mpsc::Receiver<crate::Forward>,
) -> std::io::Result<()> {
    use std::net::{Ipv4Addr, SocketAddrV4, UdpSocket};
    use std::time::Duration;

    let group = Ipv4Addr::from(UDP_GROUP);
    let dst = SocketAddrV4::new(group, UDP_PORT);
    let sock = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, UDP_PORT))?;
    sock.join_multicast_v4(&group, &Ipv4Addr::UNSPECIFIED)?;
    sock.set_read_timeout(Some(Duration::from_millis(200)))?;

    let a = hub.addr();
    let my_node = u32::from_be_bytes([a[0], a[1], a[2], a[3]]);
    println!("  [meshtastic] iface {iface} on {group}:{UDP_PORT} (node !{my_node:08x})");

    let t = Mesh { sock, dst, my_node, rng: my_node ^ 0x9e37_79b9 };
    // This CLI/daemon-only runner has no stop control of its own yet (the
    // process itself is the unit of shutdown here); `run_datagram`'s stop
    // check is a no-op flag that never gets set.
    crate::bridge::driver::run_datagram(hub, iface, rx, &std::sync::atomic::AtomicBool::new(false), t)
}

// ---------------------------------------------------------------------------
// Serial / USB — Meshtastic's stream API.
//
// The same MeshPacket codec as the UDP path; only the pipe differs. On serial a
// packet is wrapped in a `ToRadio` (outbound) or arrives inside a `FromRadio`
// (inbound), and each is framed `0x94 0xc3 <len:u16 be> <body>`. The device
// interleaves plain-text debug logs on the same line, so the de-framer
// resynchronises on the magic rather than assuming it is at the start.
// ---------------------------------------------------------------------------

/// Stream API frame magic, first byte.
pub const STREAM_START1: u8 = 0x94;
/// Stream API frame magic, second byte.
pub const STREAM_START2: u8 = 0xc3;
/// Longest body the firmware will emit; anything claiming more is not a header.
pub const STREAM_MAX_LEN: usize = 512;

/// Frame a MeshPacket as a `ToRadio` for the serial stream.
pub fn stream_encode(pkt: &[u8]) -> Vec<u8> {
    let mut to_radio = Vec::new();
    put_bytes(&mut to_radio, 1, pkt); // ToRadio.packet = 1
    let mut out = Vec::with_capacity(4 + to_radio.len());
    out.push(STREAM_START1);
    out.push(STREAM_START2);
    out.extend_from_slice(&(to_radio.len() as u16).to_be_bytes());
    out.extend_from_slice(&to_radio);
    out
}

/// Pull the `MeshPacket` out of a `FromRadio` body (field 2), if it has one.
/// Other `FromRadio` variants — config, node info, log records — return `None`.
pub fn from_radio_packet(body: &[u8]) -> Option<Vec<u8>> {
    let mut o = 0usize;
    while o < body.len() {
        let (tag, no) = get_varint(body, o)?;
        o = no;
        let (field, wire) = ((tag >> 3) as u32, (tag & 7) as u8);
        match wire {
            0 => o = get_varint(body, o)?.1,
            5 => o = o.checked_add(4)?,
            1 => o = o.checked_add(8)?,
            2 => {
                let (len, no) = get_varint(body, o)?;
                let (start, len) = (no, len as usize);
                let end = start.checked_add(len)?;
                if end > body.len() {
                    return None;
                }
                if field == 2 {
                    return Some(body[start..end].to_vec());
                }
                o = end;
            }
            _ => return None,
        }
    }
    None
}

/// Streaming de-framer for the serial link.
#[derive(Default)]
pub struct StreamFramer {
    buf: Vec<u8>,
}

impl StreamFramer {
    /// How many bytes are held pending a complete frame.
    ///
    /// Exposed so a fuzz target and the robustness harness can assert the framer
    /// stays *bounded*, not merely that it does not panic. A framer that grows
    /// quietly is the S-013 failure mode — no crash, no error, just memory — and
    /// "it returned" is not evidence against it.
    pub fn buffered(&self) -> usize {
        self.buf.len()
    }

    pub fn new() -> StreamFramer {
        StreamFramer { buf: Vec::new() }
    }

    /// Feed freshly read bytes; returns every complete frame body they finished.
    /// Debug-log text between frames is skipped, and a length that could not be
    /// real is treated as a coincidence in that text rather than a frame.
    pub fn push(&mut self, bytes: &[u8]) -> Vec<Vec<u8>> {
        self.buf.extend_from_slice(bytes);
        let mut out = Vec::new();
        loop {
            let magic = self.buf.windows(2).position(|w| w[0] == STREAM_START1 && w[1] == STREAM_START2);
            let Some(i) = magic else {
                // No frame is starting. Keep only a trailing byte, which might
                // be the first half of a magic split across two reads.
                if self.buf.len() > 1 {
                    self.buf.drain(..self.buf.len() - 1);
                }
                return out;
            };
            if i > 0 {
                self.buf.drain(..i); // drop the log noise ahead of it
            }
            if self.buf.len() < 4 {
                return out; // header still arriving
            }
            let len = u16::from_be_bytes([self.buf[2], self.buf[3]]) as usize;
            if len > STREAM_MAX_LEN {
                self.buf.drain(..2); // not a header after all — resync past it
                continue;
            }
            if self.buf.len() < 4 + len {
                return out; // body still arriving
            }
            out.push(self.buf[4..4 + len].to_vec());
            self.buf.drain(..4 + len);
        }
    }
}

/// Bridge a Meshtastic device over any byte stream: read `FromRadio` frames from
/// `r`, write `ToRadio` frames to `w`.
#[cfg(not(target_arch = "wasm32"))]
pub fn run_stream<R, W>(
    hub: crate::bridge::hub::Shared,
    iface: crate::Iface,
    rx: std::sync::mpsc::Receiver<crate::Forward>,
    mut r: R,
    mut w: W,
) -> std::io::Result<()>
where
    R: std::io::Read + Send + 'static,
    W: std::io::Write,
{
    // A LoRa packet is 237 bytes; anything bigger has to be fountain-fragmented
    // by the core rather than discovered at the radio.
    hub.with_node(|n| n.mtu = n.mtu.min(237));

    let a = hub.addr();
    let my_node = u32::from_be_bytes([a[0], a[1], a[2], a[3]]);

    // Reader thread: FromRadio frames → the shared node.
    let rhub = hub.clone();
    std::thread::spawn(move || {
        let mut framer = StreamFramer::new();
        let mut buf = [0u8; 4096];
        loop {
            match r.read(&mut buf) {
                Ok(0) | Err(_) => break, // device unplugged or pipe closed
                Ok(n) => {
                    for body in framer.push(&buf[..n]) {
                        let Some(pkt) = from_radio_packet(&body) else { continue };
                        // Only our port, and never our own packet echoed back.
                        if let Some((from, port, payload)) = decode(&pkt) {
                            if port == PORT_PRIVATE_APP && from != my_node && !payload.is_empty() {
                                rhub.on_rx(iface, &payload, None);
                            }
                        }
                    }
                }
            }
        }
    });

    // Main loop: outbound forwards → ToRadio frames. `recv` blocks, so this
    // never busy-waits.
    let mut rng = my_node ^ 0x9e37_79b9;
    loop {
        let Ok(f) = rx.recv() else { return Ok(()) }; // hub gone
        let (crate::Forward::Flood { bytes, .. } | crate::Forward::Directed { bytes, .. }) = f;
        rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
        let pkt = encode(&bytes, my_node, BROADCAST, rng);
        w.write_all(&stream_encode(&pkt))?;
        w.flush()?;
    }
}

/// Bridge a Meshtastic device on a serial port, by path.
///
/// The line must already be configured — this deliberately links no termios, in
/// keeping with the audio bridge taking PCM on a pipe rather than owning a sound
/// card. On Linux: `stty -F /dev/ttyUSB0 115200 raw -echo` first.
#[cfg(not(target_arch = "wasm32"))]
pub fn run_serial(
    hub: crate::bridge::hub::Shared,
    iface: crate::Iface,
    rx: std::sync::mpsc::Receiver<crate::Forward>,
    path: &str,
) -> std::io::Result<()> {
    let r = std::fs::File::open(path)?;
    let w = std::fs::OpenOptions::new().write(true).open(path)?;
    println!("  [meshtastic] iface {iface} on {path} (stream API, 237-byte MTU)");
    run_stream(hub, iface, rx, r, w)
}

/// Bridge a Meshtastic device over stdin/stdout, so any external tool can supply
/// the port (`socat`, `cu`, an ssh tunnel to a remote radio).
#[cfg(not(target_arch = "wasm32"))]
pub fn run_pipe(
    hub: crate::bridge::hub::Shared,
    iface: crate::Iface,
    rx: std::sync::mpsc::Receiver<crate::Forward>,
) -> std::io::Result<()> {
    eprintln!("  [meshtastic] iface {iface} — stream API on stdin/stdout");
    run_stream(hub, iface, rx, std::io::stdin(), std::io::stdout())
}

#[cfg(test)]
mod serial_tests {
    use super::*;

    #[test]
    fn a_packet_survives_the_serial_framing_both_ways() {
        let env: &[u8] = &[1, 2, 3, 0x94, 0xc3, 0, 255];
        let pkt = encode(env, 0xdead_beef, BROADCAST, 7);
        let framed = stream_encode(&pkt);
        assert_eq!(&framed[..2], &[STREAM_START1, STREAM_START2]);

        // The device echoes a MeshPacket back inside a FromRadio (field 2).
        let mut from_radio = Vec::new();
        put_bytes(&mut from_radio, 2, &pkt);
        let mut wire = vec![STREAM_START1, STREAM_START2];
        wire.extend_from_slice(&(from_radio.len() as u16).to_be_bytes());
        wire.extend_from_slice(&from_radio);

        let mut framer = StreamFramer::new();
        let bodies = framer.push(&wire);
        assert_eq!(bodies.len(), 1);
        let got = from_radio_packet(&bodies[0]).expect("a FromRadio carrying a packet");
        let (from, port, payload) = decode(&got).expect("decodes");
        assert_eq!(from, 0xdead_beef);
        assert_eq!(port, PORT_PRIVATE_APP);
        assert_eq!(payload, env);
    }

    #[test]
    fn the_deframer_resynchronises_past_the_devices_debug_logs() {
        let pkt = encode(b"hello", 1, BROADCAST, 1);
        let mut from_radio = Vec::new();
        put_bytes(&mut from_radio, 2, &pkt);
        let mut frame = vec![STREAM_START1, STREAM_START2];
        frame.extend_from_slice(&(from_radio.len() as u16).to_be_bytes());
        frame.extend_from_slice(&from_radio);

        let mut wire = b"INFO  | Radio init\n".to_vec(); // logs share the line
        wire.extend_from_slice(&frame);
        wire.extend_from_slice(b"DEBUG | sent\n");

        // Split every byte into its own read: the framer must not care.
        let mut framer = StreamFramer::new();
        let mut bodies = Vec::new();
        for b in &wire {
            bodies.extend(framer.push(&[*b]));
        }
        assert_eq!(bodies.len(), 1, "one frame, found among the noise");
        assert!(from_radio_packet(&bodies[0]).is_some());

        // Trailing text must not accumulate without bound.
        for _ in 0..1000 {
            framer.push(b"chatter chatter chatter\n");
        }
        assert!(framer.buf.len() <= 1, "log noise cannot grow the buffer");
    }

    #[test]
    fn a_bogus_length_is_treated_as_coincidence_not_a_frame() {
        // 0x94 0xc3 can appear inside log text; a length past the firmware's
        // maximum is the tell.
        let mut framer = StreamFramer::new();
        let mut wire = vec![STREAM_START1, STREAM_START2, 0xff, 0xff];
        let pkt = encode(b"real", 1, BROADCAST, 1);
        let mut from_radio = Vec::new();
        put_bytes(&mut from_radio, 2, &pkt);
        wire.extend_from_slice(&[STREAM_START1, STREAM_START2]);
        wire.extend_from_slice(&(from_radio.len() as u16).to_be_bytes());
        wire.extend_from_slice(&from_radio);

        let bodies = framer.push(&wire);
        assert_eq!(bodies.len(), 1, "resynced onto the real frame behind it");
    }
}
