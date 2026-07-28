//! CLI surface for the `spore` binary, split from `main.rs` (task #23).

#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod config;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod run;
pub(crate) mod sim;
