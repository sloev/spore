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
