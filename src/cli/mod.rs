//! CLI surface for the `spore` binary, split from `main.rs` (task #23).

#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod config;
#[cfg(not(target_arch = "wasm32"))]
// The console is application-layer: it reads the shared thread store. A build
// with `--no-default-features` is the kernel-only shape ESP32 uses, and a
// headless relay has nobody to type at it.
#[cfg(feature = "communicator")]
pub(crate) mod console;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod direct;
pub(crate) mod home;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod run;
pub(crate) mod sim;
