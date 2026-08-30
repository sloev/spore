//! The wired tether (M8/E4) — a phone or laptop as one more bridge.
//!
//! Nothing here is a "tether mode". Per M8's locked rule this registers on the
//! hub exactly like the radio does, the core floods between the two, and a
//! gateway is what that adds up to: a host on this link reaches the 802.11 mesh
//! through the board, and the mesh reaches whatever bridges the host has.
//! Unplugging removes an interface and changes nothing else.
//!
//! No new framing and no new transport. KISS over a byte stream is what
//! `bridge::kiss_stream` already does for the serial and TNC bridges, and
//! `stream_link::run_split` is the loop that drives it — so a host already
//! speaks this: `web/transports/webserial.mjs` is byte-for-byte compatible, and
//! [Hardware verification](../../docs/HARDWARE.md) row 4 is the procedure.
//!
//! # Why UART and not USB on the S2
//!
//! The ESP32-S2 has exactly one USB peripheral (USB-OTG) and no USB-Serial-JTAG,
//! which the S3 and C3 do have. The console already uses that one peripheral —
//! it has to, since a LOLIN S2 Mini wires the chip's own USB straight to the
//! connector with no UART bridge, so without it the board looks dead. Putting
//! KISS frames on the same CDC endpoint would interleave them with log text and
//! corrupt both.
//!
//! So on the S2 the tether is UART, which is free, needs no arbitration, and
//! costs a $2 USB-serial adapter on the host. On the S3 the console can move to
//! USB-Serial-JTAG and leave USB-OTG entirely for CDC data — which is the
//! arrangement the roadmap's "USB-CDC tether" assumed, and it is true there.
//! Doing it over USB on an S2 needs a TinyUSB composite device with two CDC
//! interfaces; that is a real option, just not a free one, and not something to
//! write blind.

use esp_idf_svc::sys as idf;

/// UART1, because UART0 is the console's on parts where the console is on a
/// UART at all. Nothing else claims it.
const PORT: idf::uart_port_t = 1;

/// Pins for UART1. Both are ordinary GPIOs on an S2 Mini's header, chosen to
/// avoid the strapping pins (0, 45, 46) that decide boot mode.
pub const TX_PIN: i32 = 17;
pub const RX_PIN: i32 = 18;

/// 115200, matching every other KISS link in the tree and what
/// `HARDWARE.md` row 4 tells an operator to set.
pub const BAUD: u32 = 115_200;

/// Where the VFS exposes this port once the driver is installed.
pub const DEVICE: &str = "/dev/uart/1";

/// Bring UART1 up and route it through the VFS, so it can be opened as a file.
///
/// The point of going through VFS rather than the UART driver's own read/write
/// is that `std::fs::File` then gives us `Read + Write`, which is exactly what
/// `stream_link::run_split` wants — the same shape `bridge::serial` hands it on
/// a desktop. The board ends up running the identical bridge code a daemon does.
pub fn start() -> Result<(), idf::EspError> {
    let cfg = idf::uart_config_t {
        baud_rate: BAUD as i32,
        data_bits: idf::uart_word_length_t_UART_DATA_8_BITS,
        parity: idf::uart_parity_t_UART_PARITY_DISABLE,
        stop_bits: idf::uart_stop_bits_t_UART_STOP_BITS_1,
        flow_ctrl: idf::uart_hw_flowcontrol_t_UART_HW_FLOWCTRL_DISABLE,
        // Clock source lives in an anonymous union in these bindings; the
        // zero default is the IDF default source, which is what we want.
        ..Default::default()
    };
    unsafe {
        idf::esp!(idf::uart_param_config(PORT, &cfg))?;
        idf::esp!(idf::uart_set_pin(PORT, TX_PIN, RX_PIN, idf::UART_PIN_NO_CHANGE, idf::UART_PIN_NO_CHANGE))?;
        // Buffers sized for a couple of KISS frames in flight. The MTU is 1400,
        // and a frame escapes worst-case to twice that, so 4 KB leaves room
        // without pretending this link needs to absorb a burst — backpressure
        // upward is the store's job, not a ring buffer's.
        idf::esp!(idf::uart_driver_install(PORT, 4096, 4096, 0, core::ptr::null_mut(), 0))?;
        // Without this the VFS does polling I/O and a read spins the CPU; with
        // it, reads block on the driver's interrupt like a file read should.
        idf::esp_vfs_dev_uart_use_driver(PORT as core::ffi::c_int);
    }
    Ok(())
}
