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

use esp_idf_svc::hal::delay::FreeRtos;
use esp_idf_svc::sys as idf;
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
    let mut node = Node::new("esp32-bringup", &["news"]);
    log::info!("identity: addr={} (seed is not logged)", hex(&node.addr));

    // --- One real signature --------------------------------------------------
    // The heaviest crypto the core does, and the most likely thing to blow a
    // small stack. Signing and verifying here means ed25519-dalek works on
    // Xtensa, not merely that it compiled for it.
    let mut env = Envelope::new(ty::DATA, ZERO_DEST, now(), b"the dam holds".to_vec());
    env.sign(&node.sk);
    let wire = env.wire();
    log::info!(
        "signed envelope: {} bytes, id={}, verify={}",
        wire.len(),
        hex(&env.id()),
        env.verify()
    );
    let boot_sig_ok = env.verify();
    assert!(boot_sig_ok, "a signature this node just made must verify");

    // Round-trip it through the same front door a bridge would use, including
    // the E2 pre-filter the raw-802.11 receive path is built on.
    let probed = Envelope::probe(&wire);
    let (decoded, _) = Envelope::decode(&wire).expect("our own envelope must decode");
    let boot_probe_ok = probed == Some(wire.len()) && decoded.verify();
    log::info!("probe={:?} (wire is {} bytes), decoded ok={}", probed, wire.len(), decoded.verify());

    log::info!("heap used by identity + one envelope: {} bytes", heap_before.saturating_sub(free_heap()));

    // --- Scheduling nutrient -------------------------------------------------
    // Without this the node only ever maintains itself when traffic happens to
    // arrive, which on a solo, often-offline relay means effectively never. A
    // FreeRTOS delay loop is the whole contract: call `tick` on a timer.
    log::info!("entering tick loop — heap now {} bytes", free_heap());
    let mut ticks: u32 = 0;
    loop {
        let due = node.tick(now());
        ticks += 1;
        // Everything above this loop is printed once, at boot — and on a board
        // whose console *is* its USB port, nothing is listening then: the host
        // only attaches a moment later, by which time the identity and the
        // signature check have scrolled past unread. So repeat the summary on a
        // slow cycle. Attaching at any moment tells you which node this is and
        // that its crypto still works, rather than only telling you it is alive.
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
                "up {}s · addr={} · sig={} · probe={} · heap={} · due={}",
                now(),
                hex(&node.addr),
                if boot_sig_ok { "ok" } else { "FAILED" },
                if boot_probe_ok { "ok" } else { "FAILED" },
                free_heap(),
                due.len()
            );
        }
        FreeRtos::delay_ms(5_000);
    }
}

/// Time nutrient. The board has no RTC battery and no network yet, so on a cold
/// boot this is seconds-since-reset, not wall clock — which SPEC §Time already
/// covers: a node with no trusted clock must not drop on expiry, it relays
/// regardless and ages by dwell. Wiring SNTP later replaces this and nothing
/// else, because the core never reads a clock itself — time arrives per call.
fn now() -> u32 {
    (unsafe { idf::esp_timer_get_time() } / 1_000_000) as u32
}
