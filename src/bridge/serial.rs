//! Opening a serial port, without linking a serial crate.
//!
//! On Unix a tty is a file, so this is `File::open` twice — one handle to read,
//! one to write, because the reader runs in its own thread. What this
//! deliberately does *not* do is configure the line: baud rate, raw mode and
//! echo are `stty`'s job, exactly as the audio bridge takes PCM on a pipe rather
//! than owning a sound card. One less dependency, and the same command works for
//! every device.
//!
//! ```sh
//! stty -F /dev/ttyUSB0 9600 raw -echo     # Linux
//! stty -f /dev/tty.usbserial 9600 raw     # macOS
//! ```

use std::fs::{File, OpenOptions};

/// Read and write handles for the same port.
pub fn open(path: &str) -> std::io::Result<(File, File)> {
    let r = File::open(path)?;
    let w = OpenOptions::new().write(true).open(path)?;
    Ok((r, w))
}
