//! The layers that sit *under* the envelope: text armor (paper, chat, voice) and
//! the KISS framings every stream and radio bridge is built on.
//!
//! `KissStream` keeps state across reads, so it is fed incrementally here — that
//! is how a real bridge uses it, and where a length bound is most easily lost.
#![no_main]
use libfuzzer_sys::fuzz_target;
use spore::*;

fuzz_target!(|data: &[u8]| {
    let _ = armor::unwrap(&String::from_utf8_lossy(data));
    let _ = kiss::decode(data);
    let _ = file::Manifest::decode(data);
    let _ = spore::bridge::icmp::decode_echo(data);

    let mut framer = spore::bridge::KissStream::new();
    for piece in data.chunks(7) {
        let _ = framer.push(piece);
    }
});
