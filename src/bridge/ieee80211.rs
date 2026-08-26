//! Raw 802.11 framing — the codec both raw-Wi-Fi implementations share.
//!
//! [Bridges](../../docs/BRIDGES.md#wi-fi-80211) specifies this format; this is
//! it in code. The ESP32 firmware (M8/E2) and the Linux daemon's monitor-mode
//! bridge (M8/E2d) are two implementations of one wire format, so the framing
//! lives here — portable, `std`-only, and tested in CI — rather than twice in
//! two platform-specific files where they could drift apart.
//!
//! SPORE rides a **vendor-specific Action frame**: management type, subtype
//! Action, category 127. That is the mechanism ESP-NOW is built on, and the one
//! frame shape an ESP32 is known to inject and receive without associating to
//! anything. It needs no LLC/SNAP header and no registered EtherType.
//!
//! Everything here parses attacker-chosen bytes off the air — on a medium that
//! delivers *every* frame nearby, most of which belong to other people — so
//! `parse` must reject without panicking, exactly like `Envelope::decode`.

use crate::Envelope;

/// Broadcast destination. SPORE floods and dedups by content id, so unicast at
/// this layer would duplicate routing the core already does.
pub const BROADCAST: [u8; 6] = [0xff; 6];

/// A constant, not an access point: `02` + "SPORE" in ASCII, with the
/// locally-administered bit set and the multicast bit clear. Nothing joins it
/// and no beacons are sent — it exists so a capture filter can select SPORE
/// traffic cheaply, and so two boards out of the same box agree with no
/// configuration.
pub const BSSID: [u8; 6] = [0x02, b'S', b'P', b'O', b'R', b'E'];

/// Locally administered, not IEEE-registered. A collision with another vendor
/// using the same value is possible and harmless: the magic byte, the version,
/// and then `Envelope::probe` reject anything that is not ours before it costs
/// a decode.
pub const OUI: [u8; 3] = [0x02, 0x53, 0x50];

const FC_MGMT_ACTION: [u8; 2] = [0xd0, 0x00];
const CATEGORY_VENDOR: u8 = 0x7f;
const MAGIC: u8 = b'S';
const VERSION: u8 = 0x01;

/// 24-byte MAC header + category + OUI + magic + version.
pub const HEADER_LEN: usize = 30;

/// Largest envelope that fits one frame. The 802.11 MSDU ceiling is 2304, but
/// that is the standard's number rather than what a driver will actually inject
/// — ESP-IDF and mac80211 both impose their own lower limits. Kept conservative
/// until a device run establishes the real one (M8/E2, 🧪 until then).
pub const MAX_PAYLOAD: usize = 1400;

/// Build a frame carrying `env`, sent from `src_mac`.
///
/// The FCS is appended by the radio hardware, not here.
pub fn build(src_mac: &[u8; 6], env: &[u8]) -> Vec<u8> {
    let mut f = Vec::with_capacity(HEADER_LEN + env.len());
    f.extend_from_slice(&FC_MGMT_ACTION);
    f.extend_from_slice(&[0x00, 0x00]); // duration — hardware fills it in
    f.extend_from_slice(&BROADCAST); // addr1: destination
    f.extend_from_slice(src_mac); // addr2: transmitter, learned as `U`
    f.extend_from_slice(&BSSID); // addr3
    f.extend_from_slice(&[0x00, 0x00]); // sequence control — hardware fills it in
    f.push(CATEGORY_VENDOR);
    f.extend_from_slice(&OUI);
    f.push(MAGIC);
    f.push(VERSION);
    f.extend_from_slice(env);
    f
}

/// Pull an envelope and its transmitter MAC out of a received frame.
///
/// `None` for anything that is not ours, which on this medium is the
/// overwhelming majority of what arrives. The checks run cheapest-first so
/// foreign traffic is dropped for the price of a couple of byte comparisons:
/// length, then frame control, then our OUI and magic, and only then the
/// envelope itself.
///
/// A `Some` answer means "structurally ours", never "authentic" — nothing here
/// looks at a signature. [`Envelope::verify`] still decides that.
pub fn parse(frame: &[u8]) -> Option<(Vec<u8>, [u8; 6])> {
    if frame.len() <= HEADER_LEN {
        return None;
    }
    if frame[0..2] != FC_MGMT_ACTION {
        return None;
    }
    if frame[16..22] != BSSID {
        return None;
    }
    if frame[24] != CATEGORY_VENDOR || frame[25..28] != OUI {
        return None;
    }
    if frame[28] != MAGIC || frame[29] != VERSION {
        return None;
    }

    // Trust the envelope's own header for its length rather than the frame's:
    // a radio may pad a short frame up to a minimum size, and the trailing
    // bytes are not ours to hand upward.
    let body = &frame[HEADER_LEN..];
    let len = Envelope::probe(body)?;

    let mut src = [0u8; 6];
    src.copy_from_slice(&frame[10..16]);
    Some((body[..len].to_vec(), src))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ty, Envelope, ZERO_DEST};
    use ed25519_dalek::SigningKey;

    fn envelope() -> Vec<u8> {
        let mut e = Envelope::new(ty::DATA, ZERO_DEST, 1_700_000_000, b"the dam holds".to_vec());
        e.sign(&SigningKey::from_bytes(&[7u8; 32]));
        e.wire()
    }

    const MAC: [u8; 6] = [0x80, 0x65, 0x99, 0x49, 0x8f, 0x6e];

    #[test]
    fn a_frame_round_trips() {
        let env = envelope();
        let frame = build(&MAC, &env);
        assert_eq!(frame.len(), HEADER_LEN + env.len());
        let (got, src) = parse(&frame).expect("our own frame must parse");
        assert_eq!(got, env);
        assert_eq!(src, MAC, "addr2 is the transmitter, which is what U is learned from");
    }

    #[test]
    fn the_bssid_spells_spore() {
        // Documented in BRIDGES.md as "02 + SPORE", so a change to either has to
        // be a change to both.
        assert_eq!(&BSSID[1..], b"SPORE");
        assert_eq!(BSSID[0] & 0x02, 0x02, "locally administered bit must be set");
        assert_eq!(BSSID[0] & 0x01, 0x00, "must not be a multicast address");
    }

    #[test]
    fn foreign_air_traffic_is_discarded() {
        let env = envelope();
        let good = build(&MAC, &env);

        // A beacon: management type, but subtype Beacon rather than Action.
        let mut beacon = good.clone();
        beacon[0] = 0x80;
        assert_eq!(parse(&beacon), None, "wrong subtype");

        // Someone else's vendor action frame — right shape, different OUI.
        let mut other_vendor = good.clone();
        other_vendor[25] = 0x18;
        assert_eq!(parse(&other_vendor), None, "not our OUI");

        // Our OUI, but a version we do not speak.
        let mut future = good.clone();
        future[29] = 0x02;
        assert_eq!(parse(&future), None, "unknown version");

        // Right frame, but the BSSID belongs to a real access point.
        let mut ap = good.clone();
        ap[16..22].copy_from_slice(&[0xde, 0xad, 0xbe, 0xef, 0x00, 0x01]);
        assert_eq!(parse(&ap), None, "not our BSSID");

        assert_eq!(parse(&[]), None);
        assert_eq!(parse(&good[..HEADER_LEN]), None, "header with no envelope");
    }

    #[test]
    fn a_padded_frame_yields_only_the_envelope() {
        // Radios pad short frames to a minimum length. The envelope's own header
        // says how long it is, so the padding must not be handed upward — if it
        // were, the id would change and dedup would stop working.
        let env = envelope();
        let mut padded = build(&MAC, &env);
        padded.extend_from_slice(&[0u8; 32]);
        let (got, _) = parse(&padded).expect("padding must not break parsing");
        assert_eq!(got, env, "padding must be trimmed off");
    }

    #[test]
    fn a_truncated_envelope_is_rejected() {
        // The frame arrived, but the envelope inside it is incomplete — probe
        // catches this, and it must not become a half envelope handed to decode.
        let env = envelope();
        let frame = build(&MAC, &env);
        for cut in [1usize, 40, env.len() - 1] {
            let short = &frame[..frame.len() - cut];
            assert_eq!(parse(short), None, "truncated by {cut}");
        }
    }

    #[test]
    fn parsing_never_panics_on_arbitrary_bytes() {
        // Monitor mode hands us every frame in the air. A shape that happens to
        // match our header prefix must still fail safely rather than panic.
        let mut frame = build(&MAC, &envelope());
        for i in 0..frame.len() {
            let mut mangled = frame.clone();
            mangled[i] ^= 0xff;
            let _ = parse(&mangled);
        }
        for len in 0..HEADER_LEN + 8 {
            frame.truncate(len);
            let _ = parse(&frame);
        }
    }
}
