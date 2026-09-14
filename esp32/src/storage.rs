//! The storage nutrient on flash (M8/E3).
//!
//! There is no new `SpillBackend` here, and that is the point. ESP-IDF exposes
//! filesystems through VFS, so once a partition is mounted the ordinary
//! `std::fs` calls work — which means the core's own [`spore::FsSpill`], the
//! same implementation every daemon and Android node already uses and that CI
//! already tests, runs unmodified on the board. E3 was written as "implement
//! the existing M2 contract, not a new one"; mounting a filesystem turns out to
//! satisfy it literally.
//!
//! **SPIFFS, with LittleFS wanted but blocked.** LittleFS is the better
//! filesystem here — it is built around power-loss atomicity, which is exactly
//! what E3 is about — and it was tried first. Pulling `joltwallet/littlefs` in
//! through the component manager works, but then `esp-idf-svc` 0.51 fails to
//! compile: its `io` module references `crate::fs::littlefs` unconditionally
//! while the module itself stays gated. Taking a broken build for a property
//! that cannot be verified from here anyway is the wrong trade, so this uses
//! SPIFFS, which ESP-IDF ships built in. Worth revisiting when esp-idf-svc
//! fixes that, and noted in the roadmap so the reason survives.
//!
//! What the difference actually costs is smaller than it sounds: spilled
//! envelopes are content-addressed and re-verified against their id on read
//! (C-ST4), so a file SPIFFS corrupts reads as "not held" and the mesh
//! re-fetches it. The store is a cache, not a database. The real exposure is
//! losing the *filesystem* — a failed mount loses everything at once — and that
//! is what LittleFS would buy.

use esp_idf_svc::nvs::{EspDefaultNvsPartition, EspNvs, NvsDefault};
use esp_idf_svc::sys as idf;

/// Where the partition is mounted, and therefore where `FsSpill` writes.
pub const MOUNT: &str = "/spore";

/// Partition label — must match the `spore` entry in `partitions.csv`.
const LABEL: &str = "spore";

/// Mount the flash partition, formatting it the first time.
///
/// Returns the number of bytes free, which is what decides how much of the
/// store can outlive a reboot.
pub fn mount() -> Result<(usize, usize), idf::EspError> {
    let base = std::ffi::CString::new(MOUNT).unwrap();
    let label = std::ffi::CString::new(LABEL).unwrap();

    let conf = idf::esp_vfs_spiffs_conf_t {
        base_path: base.as_ptr(),
        partition_label: label.as_ptr(),
        max_files: 8,
        // A board that has never run this firmware has no filesystem yet.
        // Formatting on first mount is what makes "flash it and it works" true
        // for storage too — no provisioning step and no host tool.
        format_if_mount_failed: true,
    };
    unsafe { idf::esp!(idf::esp_vfs_spiffs_register(&conf))? };

    let (mut total, mut used) = (0usize, 0usize);
    unsafe {
        idf::esp!(idf::esp_spiffs_info(label.as_ptr(), &mut total, &mut used))?;
    }
    Ok((total, used))
}

// -- the identity nutrient (M8/E3) -------------------------------------------
//
// **In NVS, not in the SPIFFS partition beside the envelopes**, and the reason
// is three lines above: `format_if_mount_failed: true`. That flag is what makes
// "flash it and it works" true for a board that has never run this firmware —
// and it means a filesystem that comes back damaged is *erased* on the next
// boot. Keeping the seed there would make a corrupt store a new identity, which
// is precisely the failure being fixed.
//
// The two also have different worth. Spilled envelopes are a cache: they are
// content-addressed, re-verified on read, and anything lost can be asked for
// again. A seed cannot be asked for again. It is the node.
//
// NVS is ESP-IDF's own small key/value store, with wear levelling and
// power-loss-safe commits, living in its own partition — which is what you want
// for 32 bytes written once and read every boot.

/// Largest a serialised ring can be: `[ver][n]` then 16 entries of
/// `pub32 + sec32 + born4`. Bounded by `MAX_PREKEY_RING`, so a fixed buffer is
/// exact rather than hopeful.
const RING_MAX: usize = 2 + 16 * 68;

/// NVS namespace and key. Short on purpose: NVS keys are capped at 15 bytes.
const NVS_NS: &str = "spore";
const NVS_SEED: &str = "seed";
const NVS_RING: &str = "prekeys";

/// The seed for this board, minting and storing one on first boot.
///
/// Returns the seed and whether it was newly minted, so the caller can log which
/// happened — "same address as last boot" is the one line that tells a person
/// the board can be left in a field.
///
/// Errors are the caller's to survive rather than to panic on: a board whose NVS
/// is unusable is still a relay, it just forgets who it is when the power goes.
/// Saying so beats refusing to boot.
pub fn load_or_create_seed() -> Result<([u8; 32], bool), idf::EspError> {
    let mut nvs = open_nvs()?;

    let mut buf = [0u8; 32];
    match nvs.get_blob(NVS_SEED, &mut buf) {
        // A blob of exactly 32 bytes is the seed from a previous boot.
        Ok(Some(b)) if b.len() == 32 => {
            let mut seed = [0u8; 32];
            seed.copy_from_slice(b);
            Ok((seed, false))
        }
        // Anything else stored under that key is not a seed. Overwrite rather
        // than refuse: unlike a desktop, nobody is going to open a serial
        // console and recover 32 bytes by hand from a board in a box, and a
        // relay that will not boot is worth less than one with a new name. The
        // log line below is what makes it visible.
        Ok(_) => {
            let seed = fresh_seed();
            nvs.set_blob(NVS_SEED, &seed)?;
            Ok((seed, true))
        }
        Err(e) => Err(e),
    }
}

/// A new seed, drawn the same way every other platform draws one.
///
/// `Node::new` takes it from `OsRng`, which on this target getrandom routes to
/// ESP-IDF's hardware TRNG. Minting it through the core rather than calling
/// `esp_fill_random` here means there is one way to make an identity, not two —
/// and the second way is always the one that turns out to be weaker.
fn fresh_seed() -> [u8; 32] {
    spore::Node::new("bootstrap", &[]).seed()
}

/// Open the namespace both of these live in.
fn open_nvs() -> Result<EspNvs<NvsDefault>, idf::EspError> {
    let part = EspDefaultNvsPartition::take()?;
    EspNvs::new(part, NVS_NS, true)
}

/// The prekey ring from the last boot, if there is one.
///
/// Kept beside the seed because `Node::prekey_ring`'s own documentation says to:
/// "**This is secret material** — every byte of it opens mail. Store it where
/// you would store the identity seed."
///
/// The seed restores *who the board is*; the ring restores *what it can still
/// open*. A board that keeps the seed and loses the ring comes back with the
/// same address and silently drops every message sealed to a prekey it had
/// rotated to — and rotation is daily, so that is almost all of them. It looks
/// exactly like a working relay that nobody is writing to.
pub fn load_prekey_ring() -> Result<Option<Vec<u8>>, idf::EspError> {
    let nvs = open_nvs()?;
    let mut buf = [0u8; RING_MAX];
    match nvs.get_blob(NVS_RING, &mut buf) {
        Ok(Some(b)) => Ok(Some(b.to_vec())),
        Ok(None) => Ok(None),
        Err(e) => Err(e),
    }
}

/// Write the ring back after a rotation.
pub fn save_prekey_ring(blob: &[u8]) -> Result<(), idf::EspError> {
    let mut nvs = open_nvs()?;
    nvs.set_blob(NVS_RING, blob)?;
    Ok(())
}
