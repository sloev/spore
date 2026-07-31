//! Audio modem bridge (ggwave-style) — send SPORE envelopes *as sound*.
//!
//! Data-over-sound: two phones on a table, a radio's speaker to another radio's
//! mic, a room full of laptops. The medium is broadcast-only and has no address
//! (`U = ()`), so everyone in earshot hears every frame and the envelope's own
//! `dest` decides who cares — exactly the "null address" row of the bridge
//! matrix.
//!
//! ## What's portable vs. platform
//! Everything that turns bytes into samples and back lives here and is fully
//! tested: a 16-tone FSK modem ([`modulate`]) with a Goertzel receiver
//! ([`demodulate`] / [`Demod`]). The only platform-specific part is moving `f32`
//! PCM to and from a speaker/mic. [`run_pipe`] does that over stdin/stdout so you
//! can wire any sound backend without a Rust audio dependency, e.g.:
//!
//! ```text
//! # receive: mic -> sox -> spore ; transmit: spore -> sox -> speaker
//! sox -d -t f32 -r 48000 -c 1 - | spore-audio | sox -t f32 -r 48000 -c 1 - -d
//! ```
//!
//! In the browser the same [`modulate`]/[`demodulate`] compiled to wasm feed a
//! `WebAudio` `ScriptProcessor`/`AudioWorklet`.
//!
//! ## Wire format (per frame)
//! `SYNC(6 symbols)` · `LEN(2 bytes)` · `PAYLOAD(LEN bytes)` · `CRC(4 bytes)`.
//! Bytes are sent high-nibble-first, one 4-bit symbol per tone. The CRC is the
//! shared `SHA-256(frame)[0..4]` tail (`bridge::crc_append`).

use crate::bridge::{crc_append, crc_check};
use core::f32::consts::PI;

/// Samples per second the modem assumes. Sound cards universally support 48 kHz.
pub const SAMPLE_RATE: u32 = 48_000;
/// Samples per symbol. Bin-aligned to `SAMPLE_RATE / SYMBOL_LEN = 46.875 Hz` so
/// Goertzel lands exactly on each tone. ~21 ms/symbol → ~47 symbols/s.
pub const SYMBOL_LEN: usize = 1024;
/// 16 tones ⇒ 4 bits per symbol ⇒ ~23 bytes/s.
const TONES: usize = 16;

/// What this link will carry of *other people's file chunks*, per second — see
/// [`crate::bridge::hub::Hub::register_limited`].
///
/// Zero. At ~23 bytes/s one 1336-byte chunk occupies the channel for a minute,
/// so a sound link that served bulk would serve nothing else. It still carries
/// messages, announces and manifests at full speed, which is what an audio link
/// is actually for: telling the mesh you exist and what you have. Whoever wants
/// the chunks asks again and a faster path answers.
pub const BULK_BYTES_PER_SEC: u32 = 0;
const BASE_BIN: usize = 32; // 1500 Hz
const SPACING_BIN: usize = 4; // 187.5 Hz between tones — well separated at this window
const SYNC: [u8; 6] = [15, 0, 15, 0, 12, 3];
const AMPLITUDE: f32 = 0.5;

fn tone_freq(symbol: u8) -> f32 {
    ((BASE_BIN + symbol as usize * SPACING_BIN) * SAMPLE_RATE as usize) as f32 / SYMBOL_LEN as f32
}

/// Modulate a payload into mono `f32` PCM at [`SAMPLE_RATE`]. Prepend/append
/// silence yourself if your output needs a lead-in.
pub fn modulate(payload: &[u8]) -> Vec<f32> {
    // frame = LEN(2) || payload, then a 4-byte CRC tail.
    let len = payload.len().min(0xFFFF);
    let mut frame = vec![(len >> 8) as u8, (len & 0xFF) as u8];
    frame.extend_from_slice(&payload[..len]);
    let frame = crc_append(&frame);

    let mut symbols: Vec<u8> = SYNC.to_vec();
    for &b in &frame {
        symbols.push(b >> 4);
        symbols.push(b & 0x0F);
    }

    let mut out = Vec::with_capacity(symbols.len() * SYMBOL_LEN);
    for &s in &symbols {
        let f = tone_freq(s);
        for n in 0..SYMBOL_LEN {
            let t = n as f32 / SAMPLE_RATE as f32;
            out.push(AMPLITUDE * (2.0 * PI * f * t).sin());
        }
    }
    out
}

// Goertzel magnitude at a bin-aligned frequency over exactly one symbol window.
fn goertzel(samples: &[f32], freq: f32) -> f32 {
    let n = samples.len() as f32;
    let k = (n * freq / SAMPLE_RATE as f32).round();
    let w = 2.0 * PI * k / n;
    let coeff = 2.0 * w.cos();
    let (mut s1, mut s2) = (0.0f32, 0.0f32);
    for &x in samples {
        let s0 = x + coeff * s1 - s2;
        s2 = s1;
        s1 = s0;
    }
    (s1 * s1 + s2 * s2 - coeff * s1 * s2).max(0.0).sqrt()
}

// Best-matching tone for one symbol window, with its confidence (peak / total).
fn symbol_at(samples: &[f32], off: usize) -> Option<(u8, f32)> {
    if off + SYMBOL_LEN > samples.len() {
        return None;
    }
    let win = &samples[off..off + SYMBOL_LEN];
    let (mut best, mut best_mag, mut sum) = (0u8, 0.0f32, 1e-9f32);
    for s in 0..TONES as u8 {
        let m = goertzel(win, tone_freq(s));
        sum += m;
        if m > best_mag {
            best_mag = m;
            best = s;
        }
    }
    Some((best, best_mag / sum))
}

fn sync_matches(samples: &[f32], off: usize) -> bool {
    for (i, &want) in SYNC.iter().enumerate() {
        match symbol_at(samples, off + i * SYMBOL_LEN) {
            Some((sym, conf)) if sym == want && conf > 0.30 => {}
            _ => return false,
        }
    }
    true
}

fn read_bytes(samples: &[f32], start_off: usize, count: usize) -> Option<Vec<u8>> {
    let mut bytes = Vec::with_capacity(count);
    for j in 0..count {
        let (hi, _) = symbol_at(samples, start_off + (2 * j) * SYMBOL_LEN)?;
        let (lo, _) = symbol_at(samples, start_off + (2 * j + 1) * SYMBOL_LEN)?;
        bytes.push((hi << 4) | lo);
    }
    Some(bytes)
}

/// The largest payload a single frame carries (keeps a bogus LEN from allocating
/// wildly). Bigger objects fragment above the bridge, like every other medium.
pub const MAX_FRAME_PAYLOAD: usize = 4096;

// Try to decode one frame whose SYNC starts at `off`. Returns the payload and
// the sample offset just past the frame, or `None` if it wasn't a real frame.
fn decode_frame(samples: &[f32], off: usize) -> Option<(Vec<u8>, usize)> {
    let data_off = off + SYNC.len() * SYMBOL_LEN;
    let len_bytes = read_bytes(samples, data_off, 2)?;
    let payload_len = ((len_bytes[0] as usize) << 8) | len_bytes[1] as usize;
    if payload_len > MAX_FRAME_PAYLOAD {
        return None;
    }
    let frame_bytes = 2 + payload_len + 4; // LEN + payload + CRC tail
    let frame = read_bytes(samples, data_off, frame_bytes)?;
    let body = crc_check(&frame)?; // rejects false syncs and corruption
    let end = data_off + (2 * frame_bytes) * SYMBOL_LEN;
    Some((body[2..].to_vec(), end)) // strip the 2 LEN bytes
}

/// Demodulate a whole PCM buffer, returning every payload recovered from it.
/// Scans for the sync word at a fraction of a symbol so it tolerates arbitrary
/// lead-in silence and coarse timing offset.
pub fn demodulate(samples: &[f32]) -> Vec<Vec<u8>> {
    let step = SYMBOL_LEN / 8;
    let mut out = Vec::new();
    let mut off = 0usize;
    while off + SYNC.len() * SYMBOL_LEN <= samples.len() {
        if sync_matches(samples, off) {
            if let Some((payload, end)) = decode_frame(samples, off) {
                out.push(payload);
                off = end; // jump past the frame we just read
                continue;
            }
        }
        off += step;
    }
    out
}

/// Streaming receiver: push PCM as it arrives, get back completed payloads.
/// Buffers just enough to span one in-flight frame.
#[derive(Default)]
pub struct Demod {
    buf: Vec<f32>,
    /// Offsets below this have been tested for sync and cannot match later.
    ///
    /// Without it `push` restarted at zero every call, so the work per call was
    /// proportional to the whole retained buffer rather than to the new samples.
    /// Measured before the fix: a 100 ms push cost 13 ms with 1 s buffered, 94 ms
    /// with 6 s, and the buffer caps at 175 s — about 27x real time, one core
    /// saturated for as long as the room is noisy. Any sound that does not decode
    /// gets you there, with no key and no protocol participation (S-031).
    scanned: usize,
}

/// Samples one maximal frame can span — the most the buffer ever needs to hold.
fn max_frame_samples() -> usize {
    (SYNC.len() + 2 * (2 + MAX_FRAME_PAYLOAD + 4)) * SYMBOL_LEN
}

impl Demod {
    pub fn new() -> Self {
        Demod { buf: Vec::new(), scanned: 0 }
    }

    /// Append newly captured samples; returns any frames that completed.
    pub fn push(&mut self, samples: &[f32]) -> Vec<Vec<u8>> {
        self.buf.extend_from_slice(samples);
        let mut out = Vec::new();
        let step = SYMBOL_LEN / 8;
        // Resume where the last call stopped. An offset that had a full sync
        // window and did not match cannot start matching once *more* samples
        // arrive after it, so retesting it is pure waste.
        let mut off = self.scanned;
        let mut consumed = 0usize;
        while off + SYNC.len() * SYMBOL_LEN <= self.buf.len() {
            if sync_matches(&self.buf, off) {
                if let Some((payload, end)) = decode_frame(&self.buf, off) {
                    out.push(payload);
                    off = end;
                    consumed = end;
                    continue;
                }
                // Sync is here but the frame did not decode. Usually that means
                // the body is still arriving, so hold this offset and retry on the
                // next push — advancing past it would drop every frame whose sync
                // lands near the end of a chunk, which is most of them. Only give
                // up once the buffer is long enough that even a maximal frame
                // would have fitted, which bounds the stall.
                if self.buf.len() - off < max_frame_samples() {
                    break;
                }
            }
            off += step;
        }
        self.scanned = off;
        // Drop everything we've fully consumed; keep a tail that might hold the
        // start of a frame still being received. Cap the buffer so a silent mic
        // can't grow it without bound.
        let keep_from = consumed.max(self.buf.len().saturating_sub(max_frame_samples()));
        if keep_from > 0 {
            self.buf.drain(..keep_from);
            // Offsets shifted down by exactly what was removed.
            self.scanned = self.scanned.saturating_sub(keep_from);
        }
        out
    }
}

/// Sound-card-agnostic runner: read mono `f32` PCM from stdin (the mic), demod
/// and feed frames to the shared node; write modulated `f32` PCM of outbound
/// forwards to stdout (the speaker). Pipe both ends through `sox`/`ffmpeg`/
/// `pw-cat` — that pipe is the only platform-specific glue.
#[cfg(not(target_arch = "wasm32"))]
pub fn run_pipe(
    hub: crate::bridge::hub::Shared,
    iface: crate::Iface,
    rx: std::sync::mpsc::Receiver<crate::Forward>,
) -> std::io::Result<()> {
    use std::io::{Read, Write};
    use std::sync::mpsc::TryRecvError;

    println!("  [audio] iface {iface} — f32 PCM {SAMPLE_RATE} Hz mono on stdin/stdout");
    let mut stdin = std::io::stdin().lock();
    let mut stdout = std::io::stdout().lock();
    let mut demod = Demod::new();
    let mut raw = [0u8; 4096];

    loop {
        // Transmit: modulate any pending forwards to the speaker.
        loop {
            match rx.try_recv() {
                Ok(f) => {
                    let bytes = match f {
                        crate::Forward::Flood { bytes, .. } => bytes,
                        crate::Forward::Directed { bytes, .. } => bytes,
                    };
                    let pcm = modulate(&bytes);
                    let mut out = Vec::with_capacity(pcm.len() * 4);
                    for s in pcm {
                        out.extend_from_slice(&s.to_le_bytes());
                    }
                    stdout.write_all(&out)?;
                    stdout.flush()?;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return Ok(()),
            }
        }
        // Receive: pull a chunk of mic PCM and demod it.
        let n = stdin.read(&mut raw)?;
        if n == 0 {
            return Ok(()); // mic stream ended
        }
        let mut samples = Vec::with_capacity(n / 4);
        for c in raw[..n / 4 * 4].chunks_exact(4) {
            samples.push(f32::from_le_bytes([c[0], c[1], c[2], c[3]]));
        }
        for frame in demod.push(&samples) {
            hub.on_rx(iface, &frame, None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // A cheap deterministic PRNG so the test is reproducible without deps.
    fn noise(seed: &mut u64) -> f32 {
        *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((*seed >> 40) as f32 / (1u64 << 24) as f32) - 0.5 // ~[-0.5, 0.5)
    }

    #[test]
    fn roundtrip_clean() {
        let msg = b"SPORE over sound";
        let pcm = modulate(msg);
        let got = demodulate(&pcm);
        assert_eq!(got, vec![msg.to_vec()]);
    }

    #[test]
    fn roundtrip_with_silence_and_noise() {
        let msg = b"the dam holds; meet at the north pier";
        let mut pcm = vec![0.0f32; 777]; // arbitrary (non-symbol-aligned) lead-in
        pcm.extend(modulate(msg));
        // `repeat_n` would be tidier but is 1.82; MSRV is 1.75.
        pcm.extend(std::iter::repeat_n(0.0, 500));
        // Add low-level channel noise.
        let mut seed = 0xDEADBEEFu64;
        for s in pcm.iter_mut() {
            *s += 0.06 * noise(&mut seed);
        }
        let got = demodulate(&pcm);
        assert_eq!(got, vec![msg.to_vec()]);
    }

    #[test]
    fn two_frames_back_to_back() {
        let a = b"first";
        let b = b"second frame";
        let mut pcm = modulate(a);
        pcm.extend(modulate(b));
        let got = demodulate(&pcm);
        assert_eq!(got, vec![a.to_vec(), b.to_vec()]);
    }

    #[test]
    fn streaming_demod_in_chunks() {
        let msg = b"streamed in little pieces";
        let pcm = modulate(msg);
        let mut d = Demod::new();
        let mut got = Vec::new();
        for chunk in pcm.chunks(333) {
            got.extend(d.push(chunk));
        }
        assert_eq!(got, vec![msg.to_vec()]);
    }
}

#[cfg(test)]
mod scan_tests {
    use super::*;

    /// S-031. The cursor must not cost us frames: a payload split across many
    /// small pushes — which is what a real mic delivers — must still decode.
    #[test]
    fn a_frame_split_across_many_pushes_still_decodes() {
        for chunk in [64usize, 257, 1024, 4096] {
            let payload = b"the dam holds, and the pier is clear".to_vec();
            let mut pcm = vec![0.0f32; 3000]; // lead-in silence, as a real mic has
            pcm.extend(modulate(&payload));
            pcm.extend(vec![0.0f32; 3000]);
            let mut d = Demod::new();
            let mut got = Vec::new();
            for part in pcm.chunks(chunk) {
                got.extend(d.push(part));
            }
            assert_eq!(got, vec![payload.clone()], "lost the frame at chunk size {chunk}");
        }
    }

    /// Two frames back to back, so the cursor has to survive a consume + drain.
    #[test]
    fn two_frames_in_a_stream_both_decode() {
        let mut pcm = vec![0.0f32; 2000];
        pcm.extend(modulate(b"first"));
        pcm.extend(vec![0.0f32; 2000]);
        pcm.extend(modulate(b"second"));
        pcm.extend(vec![0.0f32; 2000]);
        let mut d = Demod::new();
        let mut got = Vec::new();
        for part in pcm.chunks(512) {
            got.extend(d.push(part));
        }
        assert_eq!(got, vec![b"first".to_vec(), b"second".to_vec()]);
    }

    /// The property the cursor exists for: work per push must not grow with the
    /// buffer. Noise never syncs, so the buffer fills to its cap; if the scan
    /// still restarted at zero this would grow without bound.
    #[test]
    fn scanning_does_not_redo_work_as_the_buffer_fills() {
        let chunk = SAMPLE_RATE as usize / 10;
        let mut x = 12345u32;
        let noise: Vec<f32> = (0..chunk)
            .map(|_| {
                x = x.wrapping_mul(1103515245).wrapping_add(12345);
                (x >> 16) as f32 / 32768.0 - 1.0
            })
            .collect();
        let mut d = Demod::new();
        for _ in 0..5 {
            d.push(&noise);
        }
        let early = std::time::Instant::now();
        d.push(&noise);
        let early = early.elapsed();
        for _ in 0..40 {
            d.push(&noise);
        }
        let late = std::time::Instant::now();
        d.push(&noise);
        let late = late.elapsed();
        // Before the cursor this ratio tracked the buffer length and reached ~30x.
        // Generous bound: timing on shared CI is noisy, and the point is that it
        // is flat rather than linear.
        assert!(
            late < early * 8 + std::time::Duration::from_millis(5),
            "push cost grew with the buffer: {early:?} early vs {late:?} late"
        );
    }
}
