//! Bridges — SPORE rides everything.
//!
//! Each medium on Earth has one of five shapes (spec Page 2); bind by shape and
//! the router never changes. A bridge only moves envelope bytes in and out of a
//! `Node` — it is not part of the protocol. HTTP, a folder, a serial line, a
//! Meshtastic mesh: all just bridges, none more special than another.
//!
//! Each concern lives in its own file; the shared pieces are re-exported here so
//! call sites stay `bridge::Neighbors`, `bridge::Csma`, `bridge::bag`, …

pub mod bag;
mod csma;
mod kiss_stream;
mod neighbors;

pub use bag::{bag, Bag};
pub use csma::{crc_append, crc_check, Csma};
pub use kiss_stream::KissStream;
pub use neighbors::Neighbors;

pub mod meshtastic;
pub mod reticulum;
#[cfg(not(target_arch = "wasm32"))]
pub mod serial;

// Portable codecs (also compile to wasm for browser bridges).
pub mod audio;
pub mod ssb;

#[cfg(not(target_arch = "wasm32"))]
pub mod ax25;
#[cfg(not(target_arch = "wasm32"))]
pub mod copyparty;
#[cfg(not(target_arch = "wasm32"))]
pub mod foldersync;
#[cfg(not(target_arch = "wasm32"))]
pub mod i2p;
#[cfg(not(target_arch = "wasm32"))]
pub mod store;
#[cfg(not(target_arch = "wasm32"))]
pub mod stream_link;

// Runner glue — one shared node, threads per bridge (native only).
#[cfg(not(target_arch = "wasm32"))]
pub mod driver;
#[cfg(not(target_arch = "wasm32"))]
pub mod hub;
#[cfg(not(target_arch = "wasm32"))]
pub mod tcp;
#[cfg(not(target_arch = "wasm32"))]
pub mod tor;
#[cfg(not(target_arch = "wasm32"))]
pub mod udp;
