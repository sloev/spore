//! SPORE Direct — two peers negotiate a pipe and talk over it, in one process.
//!
//! No sockets: the `Loopback` transport stands in for a real underlay so the whole
//! path — OFFER → medium selection → ANSWER → key schedule → sealed records both
//! ways — runs deterministically. Swap `Loopback` for a UDP/TCP adapter and the
//! only thing that changes is which `DatagramPort` you hand to `answer`/`finish`.
//!
//! Run: `cargo run --example direct_loopback`

use spore::direct::{Answer, Candidate, DatagramPort, Loopback, Medium, Need, Offer, Pipe, RecordType};

fn main() {
    let alice = [0xA1u8; 8];
    let bob = [0xB0u8; 8];

    // One link, two ends. In the real world these come from the chosen medium's
    // adapter after negotiation; here we make them up front for clarity.
    let (alice_port, bob_port) = Loopback::pair(1200);

    // Alice offers a pipe: she needs ~5 kbps and a 64-byte MTU, and can be reached
    // over UDP at a locator Bob will dial.
    let (offer_bytes, pending) = Pipe::<Loopback>::offer(
        alice,
        bob,
        *b"pipe-id-16-bytes",
        Need { min_bps: 5_000, mtu_needed: 64, max_latency_ms: Some(150) },
        vec![Candidate {
            medium: Medium::Udp,
            locator: b"198.51.100.7:7373".to_vec(),
            est_bps: 2_000_000,
            mtu: 1200,
            rtt_hint_ms: 15,
        }],
    );
    println!("Alice → OFFER ({} bytes of SPDR, carried over send_direct)", offer_bytes.len());

    // Bob decodes the offer, is willing to use UDP, and answers over his port.
    let offer = Offer::decode(&offer_bytes).expect("valid offer");
    let (answer_bytes, bob_pipe) = Pipe::answer(&offer, bob, &[Medium::Udp], bob_port);
    let mut bob_pipe = bob_pipe.expect("Bob accepted");
    println!("Bob   → ANSWER (chose UDP)");

    // Alice finishes with Bob's answer; both now hold matching directional keys.
    let answer = Answer::decode(&answer_bytes).expect("valid answer");
    let mut alice_pipe = Pipe::finish(pending, &answer, alice_port).expect("Alice finished");
    println!("pipe up · id {:02x?}", &alice_pipe.pipe_id()[..4]);

    // Talk, best-effort, both ways.
    alice_pipe.send(RecordType::Data, b"north pier at midnight").unwrap();
    if let Some((ty, msg)) = bob_pipe.poll() {
        println!("Bob   ← {:?}: {}", ty, String::from_utf8_lossy(&msg));
    }

    bob_pipe.send(RecordType::Media, b"copy, moving now").unwrap();
    if let Some((ty, msg)) = alice_pipe.poll() {
        println!("Alice ← {:?}: {}", ty, String::from_utf8_lossy(&msg));
    }

    // A record sealed for this pipe carries no plaintext on the link.
    println!("link MTU: {} bytes", bob_pipe.port().mtu());
}
