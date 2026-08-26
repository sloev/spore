//! Raw 802.11 on the ESP32 (M8/E2) — the board half of the bridge.
//!
//! Monitor mode in, frame injection out, with no access point and nothing
//! associated. The framing itself lives in `spore::bridge::ieee80211`, shared
//! with the Linux daemon's monitor-mode bridge (M8/E2d) and tested in CI; this
//! file is only the ESP-IDF glue around it.
//!
//! The receive path is what makes this bridge unlike every other one. Promiscuous
//! mode hands us *every* frame in the air, most of it other people's, at whatever
//! rate the channel is busy — so the callback runs on the Wi-Fi task and has to
//! be cheap and allocation-shy. It filters to management frames in hardware,
//! rejects the rest with `ieee80211::parse` (a couple of byte compares before
//! anything expensive), and hands what survives to a bounded queue that `recv`
//! drains on the node's own thread.

use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock};

use esp_idf_svc::sys as idf;
use spore::bridge::driver::{DatagramTransport, Received};
use spore::bridge::ieee80211;

/// Frames waiting for `recv`. Bounded because the producer is the radio and the
/// consumer is a thread doing real work: an unbounded queue on a busy channel is
/// a memory leak with extra steps, and on a board with 226 KB of heap that is
/// fatal rather than untidy. Full means drop, which is the honest thing for a
/// medium that loses frames anyway — the fountain layer already handles loss.
const RX_DEPTH: usize = 16;

/// Channel 1, because M8's "flash it and it works" rule means two boards out of
/// the same box must find each other with no configuration. Both ends have to
/// agree; this bridge does not scan.
pub const DEFAULT_CHANNEL: u8 = 1;

static RX: OnceLock<Mutex<VecDeque<(Vec<u8>, [u8; 6])>>> = OnceLock::new();
/// Counted rather than logged: logging from the Wi-Fi task on every dropped
/// frame would itself become the reason frames are dropped.
static DROPPED: Mutex<u32> = Mutex::new(0);

fn queue() -> &'static Mutex<VecDeque<(Vec<u8>, [u8; 6])>> {
    RX.get_or_init(|| Mutex::new(VecDeque::with_capacity(RX_DEPTH)))
}

/// How many inbound frames have been dropped for a full queue since boot.
pub fn dropped() -> u32 {
    *DROPPED.lock().unwrap()
}

/// Promiscuous receive callback. Runs on the Wi-Fi task — keep it short.
unsafe extern "C" fn on_frame(buf: *mut core::ffi::c_void, kind: idf::wifi_promiscuous_pkt_type_t) {
    if buf.is_null() || kind != idf::wifi_promiscuous_pkt_type_t_WIFI_PKT_MGMT {
        return;
    }
    let pkt = &*(buf as *const idf::wifi_promiscuous_pkt_t);
    // sig_len counts the 4-byte FCS the hardware checked and we must not parse.
    let total = pkt.rx_ctrl.sig_len() as usize;
    let Some(len) = total.checked_sub(4) else { return };
    if len <= ieee80211::HEADER_LEN {
        return;
    }
    let frame = core::slice::from_raw_parts(pkt.payload.as_ptr(), len);

    // The cheap rejection happens here, on the Wi-Fi task, so foreign traffic
    // never reaches the queue or costs an allocation.
    let Some((env, src)) = ieee80211::parse(frame) else { return };

    let mut q = match queue().lock() {
        Ok(q) => q,
        Err(_) => return,
    };
    if q.len() >= RX_DEPTH {
        if let Ok(mut d) = DROPPED.lock() {
            *d = d.saturating_add(1);
        }
        return;
    }
    q.push_back((env, src));
}

/// Bring up the radio in monitor mode on `channel`, associated with nothing.
///
/// Channel 1 by default, because M8's "flash it and it works" rule means two
/// boards out of the same box have to find each other with no configuration.
/// Both ends must be on the same channel; scanning is not part of this bridge.
pub fn start(channel: u8) -> Result<[u8; 6], idf::EspError> {
    unsafe {
        let cfg = idf::wifi_init_config_t::default();
        idf::esp!(idf::esp_wifi_init(&cfg))?;
        // NULL mode: the radio is on and can inject, but no station or AP
        // interface tries to associate with anything.
        idf::esp!(idf::esp_wifi_set_mode(idf::wifi_mode_t_WIFI_MODE_NULL))?;
        idf::esp!(idf::esp_wifi_start())?;
        idf::esp!(idf::esp_wifi_set_channel(channel, idf::wifi_second_chan_t_WIFI_SECOND_CHAN_NONE))?;

        // Filter in hardware. We only ever send management/Action frames, so
        // every data and control frame nearby can be rejected before it becomes
        // a callback at all — which on a busy channel is most of the traffic.
        let filter = idf::wifi_promiscuous_filter_t {
            filter_mask: idf::WIFI_PROMIS_FILTER_MASK_MGMT,
        };
        idf::esp!(idf::esp_wifi_set_promiscuous_filter(&filter))?;
        idf::esp!(idf::esp_wifi_set_promiscuous_rx_cb(Some(on_frame)))?;
        idf::esp!(idf::esp_wifi_set_promiscuous(true))?;

        let mut mac = [0u8; 6];
        idf::esp!(idf::esp_wifi_get_mac(idf::wifi_interface_t_WIFI_IF_STA, mac.as_mut_ptr()))?;
        Ok(mac)
    }
}

/// The raw-802.11 transport: `dgram` driver form, `U` = the transmitter's MAC.
pub struct Wifi80211 {
    mac: [u8; 6],
}

impl Wifi80211 {
    /// Starts the radio and returns a transport bound to this board's MAC.
    pub fn new(channel: u8) -> Result<Self, idf::EspError> {
        Ok(Self { mac: start(channel)? })
    }

    pub fn mac(&self) -> [u8; 6] {
        self.mac
    }
}

impl DatagramTransport for Wifi80211 {
    /// The transmitter's MAC, snooped from addr2 — which is what
    /// `Neighbors<[u8;6]>` binds a SPORE address to.
    type Addr = [u8; 6];

    fn recv(&mut self) -> Received<Self::Addr> {
        let popped = queue().lock().ok().and_then(|mut q| q.pop_front());
        Ok(popped.map(|(env, src)| (env, Some(src))))
    }

    fn send(&mut self, _to: Option<&Self::Addr>, env: &[u8]) -> std::io::Result<()> {
        // `to` is ignored on purpose: every frame goes to the broadcast address.
        // SPORE floods and dedups by content id, so unicast here would duplicate
        // routing the core already does — see BRIDGES.md.
        if env.len() > ieee80211::MAX_PAYLOAD {
            return Err(std::io::Error::other(format!(
                "envelope {} bytes exceeds the {}-byte frame budget",
                env.len(),
                ieee80211::MAX_PAYLOAD
            )));
        }
        let frame = ieee80211::build(&self.mac, env);
        // en_sys_seq: let the hardware own the sequence number, which it must,
        // since the frame we handed it has zeros there.
        let err = unsafe {
            idf::esp_wifi_80211_tx(
                idf::wifi_interface_t_WIFI_IF_STA,
                frame.as_ptr() as *const core::ffi::c_void,
                frame.len() as core::ffi::c_int,
                true,
            )
        };
        if err != idf::ESP_OK {
            return Err(std::io::Error::other(format!("esp_wifi_80211_tx failed: {err}")));
        }
        Ok(())
    }

    fn mtu(&self) -> Option<usize> {
        Some(ieee80211::MAX_PAYLOAD)
    }
}
