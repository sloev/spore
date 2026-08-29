//! M8/E1 — ESP32-S3 bring-up.
//!
//! This is the toolchain scaffold, not the relay. Its whole job is to answer the
//! question E1 exists to answer: does the unmodified SPORE core compile, link,
//! and *run* on this board under esp-idf-sys, and how much room is left over?
//! Radio, flash store, USB and BLE all come later (E2–E5) and none of them are
//! worth writing until this says yes.
//!
//! So it deliberately does the smallest thing that exercises the parts most
//! likely to break on an MCU: the four nutrients the core needs from whatever
//! hosts it (randomness, time, scheduling, storage) and one real signature.

mod radio;
mod storage;

use std::sync::atomic::AtomicBool;

use esp_idf_svc::hal::delay::FreeRtos;
use esp_idf_svc::sys as idf;
use spore::bridge::driver::run_datagram;
use spore::bridge::hub::Hub;
use spore::{ty, Envelope, Node, ZERO_DEST};

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Free heap, which is the number E1's checkpoint actually turns on: it decides
/// whether esp-idf-sys stays the locked toolchain or the roadmap's stated
/// exception ("revisit only if a real MCU target proves it necessary") applies.
fn free_heap() -> u32 {
    unsafe { idf::esp_get_free_heap_size() }
}

fn main() {
    // Required once before anything else touches an IDF service.
    idf::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    log::info!("SPORE ESP32-S3 bring-up — heap at boot: {} bytes", free_heap());

    // --- Randomness nutrient -------------------------------------------------
    // `Node::new` draws its seed from `OsRng`. On this target getrandom routes
    // that to ESP-IDF's hardware TRNG (`esp_fill_random`), so no shim is needed;
    // if that ever stopped being true this line is where it would show up.
    let heap_before = free_heap();
    let node = Node::new("esp32-relay", &["news"]);
    let addr = node.addr;
    log::info!("identity: addr={} (seed is not logged)", hex(&addr));

    // --- One real signature --------------------------------------------------
    // The heaviest crypto the core does, and the most likely thing to blow a
    // small stack. Signing and verifying here means ed25519-dalek works on
    // Xtensa, not merely that it compiled for it.
    let mut env = Envelope::new(ty::DATA, ZERO_DEST, now(), b"the dam holds".to_vec());
    env.sign(&node.sk);
    let wire = env.wire();
    log::info!("signed envelope: {} bytes, id={}, verify={}", wire.len(), hex(&env.id()), env.verify());
    let boot_sig_ok = env.verify();
    assert!(boot_sig_ok, "a signature this node just made must verify");

    // Round-trip it through the same front door a bridge would use, including
    // the E2 pre-filter the raw-802.11 receive path is built on.
    let probed = Envelope::probe(&wire);
    let (decoded, _) = Envelope::decode(&wire).expect("our own envelope must decode");
    let boot_probe_ok = probed == Some(wire.len()) && decoded.verify();
    log::info!("probe={:?} (wire is {} bytes), decoded ok={}", probed, wire.len(), decoded.verify());

    log::info!("heap used by identity + one envelope: {} bytes", heap_before.saturating_sub(free_heap()));

    // --- Storage nutrient (M8/E3) --------------------------------------------
    // Mount flash before the node is handed to the hub, so whatever the last run
    // spilled is adopted at boot rather than after the first message arrives.
    // Without this the store is memory-only and a power cycle is a new node —
    // which is exactly what the first hardware run showed, with the address
    // changing between boots.
    let mut node = node;
    match storage::mount() {
        Ok((total, used)) => {
            log::info!("flash store mounted at {} — {used} of {total} bytes used", storage::MOUNT);
            // set_spill_dir builds the backend and adopts in one call. Adoption
            // re-verifies every id against its bytes, so a file SPIFFS damaged
            // is discarded rather than trusted — an id *is* the hash of its
            // content, which is why adopting from flash needs no other check.
            match node.set_spill_dir(std::path::Path::new(storage::MOUNT), now()) {
                Ok(n) => log::info!("adopted {n} envelope(s) from the last run"),
                Err(e) => log::error!("spill dir {} unusable: {e} — memory-only", storage::MOUNT),
            }
        }
        // Same reasoning as a radio that will not start: a node with no flash is
        // still a node, it just forgets on reboot. Saying so beats refusing to
        // boot, and it keeps "flash it and it works" honest on a board whose
        // partition table is wrong.
        Err(e) => log::error!("flash store unavailable ({e}) — running memory-only"),
    }

    // --- The radio (M8/E2) ---------------------------------------------------
    // Everything above proved the core runs here. This is the part that makes it
    // a relay rather than a node talking to itself: raw 802.11, no access point,
    // nothing associated, on a fixed channel so two boards out of the same box
    // find each other with no configuration.
    let hub = Hub::new(node);
    let mut radio_mac = None;
    match radio::Wifi80211::new(radio::DEFAULT_CHANNEL) {
        Ok(t) => {
            let mac = t.mac();
            radio_mac = Some(mac);
            log::info!(
                "radio up: raw 802.11 ch{} mac={} mtu={}",
                radio::DEFAULT_CHANNEL,
                hexb(&mac),
                spore::bridge::ieee80211::MAX_PAYLOAD
            );
            // One thread, owning the bridge loop. `run_datagram` is the same
            // shared loop every other dgram bridge uses — nothing here is
            // ESP-specific except the transport it was handed.
            let (iface, rx) = hub.register();
            let hub_for_radio = hub.clone();
            std::thread::Builder::new()
                .stack_size(8192)
                .spawn(move || {
                    static STOP: AtomicBool = AtomicBool::new(false);
                    if let Err(e) = run_datagram(hub_for_radio, iface, rx, &STOP, t) {
                        log::error!("radio bridge stopped: {e}");
                    }
                })
                .expect("spawning the radio bridge");
        }
        // A board with no working radio is still a node: it holds what it has and
        // keeps its own state. Saying so beats refusing to boot, and it keeps the
        // "flash it and it works" promise honest on a board that cannot transmit.
        Err(e) => log::error!("radio failed to start ({e}) — continuing without it"),
    }

    // --- Scheduling nutrient -------------------------------------------------
    // Without this the node only ever maintains itself when traffic happens to
    // arrive, which on a solo, often-offline relay means effectively never.
    // `Hub::tick` drives it and dispatches whatever falls due to every bridge.
    log::info!("entering tick loop — heap now {} bytes", free_heap());
    let mut ticks: u32 = 0;
    loop {
        hub.tick();
        ticks += 1;
        // Every third tick, not every sixth: a diagnostic has to see at least two
        // of these to say anything about whether uptime advances or heap holds,
        // and at 30s apart that took longer to observe than anyone waits.
        //
        // It carries the probe result too, so every check is answerable from
        // this line alone. Depending on the boot output was a mistake — on a
        // board whose console is its own USB port, boot has already happened by
        // the time anything can listen, and resetting to catch it does not work
        // here either: an S2 ignores the DTR/RTS toggle that resets other chips.
        if ticks % 3 == 0 {
            log::info!(
                "up {}s · addr={} · sig={} · probe={} · heap={} · radio={} · rxdrop={}",
                now(),
                hex(&addr),
                if boot_sig_ok { "ok" } else { "FAILED" },
                if boot_probe_ok { "ok" } else { "FAILED" },
                free_heap(),
                match &radio_mac {
                    Some(m) => hexb(m),
                    None => "down".into(),
                },
                radio::dropped()
            );
        }
        FreeRtos::delay_ms(5_000);
    }
}

/// MAC addresses read better colon-separated than as a hex run.
fn hexb(mac: &[u8; 6]) -> String {
    mac.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(":")
}

/// Time nutrient. The board has no RTC battery and no network, so on a cold
/// boot this is seconds-since-reset, not wall clock — which SPEC §Time already
/// covers: a node with no trusted clock must not drop on expiry, it relays
/// regardless and ages by dwell. Wiring SNTP later replaces this and nothing
/// else, because the core never reads a clock itself — time arrives per call.
fn now() -> u32 {
    (unsafe { idf::esp_timer_get_time() } / 1_000_000) as u32
}
