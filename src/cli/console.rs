//! The daemon's console: what it received, and a way to send.
//!
//! Until this existed the daemon could do neither. Delivered envelopes went to
//! the delivery sink, which was installed **only when Direct was configured** —
//! so a node running plain UDP received mail and showed nobody. And there was no
//! send path at all: the binary read a config, stood up bridges, and relayed.
//!
//! That made #303's Demo A — two desktop nodes, a message crosses, a second
//! survives the receiver being offline — impossible to perform, let alone
//! record. A relay you cannot speak into or read out of is a component, not a
//! node.
//!
//! **The conversation history is the shared one** (`spore::communicator`), not a
//! fourth copy. The CLI gets the same thread store the browser and the phone
//! use, which is the whole point of M10 — and it means `/history` here and the
//! chat list there are the same data structure with the same rules about who a
//! message can be attributed to.

use spore::bridge::hub::{now, Shared};
use spore::communicator::{thread::MessageStatus, Communicator};
use spore::{Addr, Envelope, Src, ZERO_DEST};

pub(crate) fn hex8(a: &Addr) -> String {
    a.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex8(s: &str) -> Option<Addr> {
    if s.len() != 16 || !s.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let mut a = [0u8; 8];
    for i in 0..8 {
        a[i] = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(a)
}

/// Where a typed line goes.
#[derive(Clone, Copy)]
enum Target {
    /// Everyone. The default, because a node with no contacts can still be
    /// demonstrated, and because it is the case that needs no setup.
    Public,
    /// A topic, by name — the name is hashed, so it never travels.
    Topic(Addr, [u8; 0]),
    /// One node, by address.
    Peer(Addr),
}

impl Target {
    fn dest(&self) -> Addr {
        match self {
            Target::Public => ZERO_DEST,
            Target::Topic(a, _) | Target::Peer(a) => *a,
        }
    }
}

const HELP: &str = "\
  /help              this
  /status            address, home, store, peers
  /peers             who this node has heard from
  /to <16-hex>       send to one node
  /to #<name>        send to a topic (the name is hashed, never sent)
  /to public         send to everyone (the default)
  /history           the conversation with the current target
  /offer             say what this node is holding, now
  /quit              stop the node

  anything else      sent to the current target
";

/// Print one delivered envelope, and file it in the shared thread store.
///
/// Returns the sender when the core could prove one. `None` covers an unsigned
/// envelope, a signature that did not verify, and an `SRC8` frame whose key we
/// do not hold — three different things that are the same thing here: **an
/// address the envelope cannot prove**, which must never be filed into a
/// conversation. A list keyed on anything weaker is one anyone in range can
/// write into.
pub(crate) fn on_delivered(comm: &mut Communicator, wire: &[u8]) -> Option<Addr> {
    let Ok((e, _)) = Envelope::decode(wire) else {
        println!("[recv] {} bytes that did not decode", wire.len());
        return None;
    };
    let from = match &e.src {
        Src::Full(pk) if e.verify() => Some(spore::addr_of(pk)),
        _ => None,
    };

    // **Only DATA is a message.** An ANNOUNCE is a node saying hello, and it
    // arrives at the delivery sink like anything else — the first version of
    // this printed every one as a line of mojibake and filed it in the
    // conversation, which is what a demo is for finding.
    if e.typ != spore::ty::DATA {
        return from;
    }

    let sealed = e.flags & spore::fl::ENCRYPTED != 0;
    // A sealed payload is ciphertext to anyone without the session, and so is a
    // file chunk. Printing the bytes as text produces replacement characters
    // that look like corruption rather than like privacy.
    let body = match std::str::from_utf8(&e.payload) {
        Ok(s) if !s.chars().any(|c| c.is_control() && c != '\n' && c != '\t') => s.to_string(),
        _ => format!("<{} bytes, not text>", e.payload.len()),
    };

    let who = match &from {
        Some(a) => hex8(a),
        None => "unattributed".to_string(),
    };
    let scope = if e.dest == ZERO_DEST { " (public)" } else { "" };
    println!("[recv] {who}{scope}{} {body}", if sealed { " 🔒" } else { "" });

    comm.threads.receive(from, &body, sealed, e.created_at);
    from
}

/// Run the console until stdin closes or `/quit`.
///
/// Blocking reads on the calling thread: the daemon's own work happens on the
/// bridge and tick threads, so this one is free to wait for a person.
pub(crate) fn run(hub: Shared, comm: &std::sync::Mutex<Communicator>, home: &std::path::Path) {
    use std::io::{BufRead, Write};

    let mut target = Target::Public;
    // Bridges announce themselves from their own threads, so without a moment's
    // settle the prompt appears above the lines saying what came up. Purely
    // cosmetic, and the first thing a person reads.
    std::thread::sleep(std::time::Duration::from_millis(200));
    println!("\nType /help for commands. Anything else is sent to everyone.");
    let stdin = std::io::stdin();
    let mut line = String::new();
    loop {
        print!("> ");
        let _ = std::io::stdout().flush();
        line.clear();
        match stdin.lock().read_line(&mut line) {
            Ok(0) | Err(_) => {
                // stdin closed: a service, a pipe, or `< /dev/null`. That is not
                // an error and must not stop the node — it just means nobody is
                // typing. The relay keeps running.
                println!("\n[console] stdin closed — relaying without a console");
                return;
            }
            Ok(_) => {}
        }
        let text = line.trim();
        if text.is_empty() {
            continue;
        }
        if !text.starts_with('/') {
            let dest = target.dest();
            match hub.send(dest, text.as_bytes().to_vec()) {
                Ok(()) => {
                    // Filed as ours, with the id the kernel minted, so `/history`
                    // shows both sides of a conversation rather than only the
                    // half that arrived.
                    let mut e = Envelope::new(spore::ty::DATA, dest, now(), text.as_bytes().to_vec());
                    e.flags |= spore::fl::FLOOD;
                    if let Ok(mut c) = comm.lock() {
                        c.threads.send(dest, e.id(), text, false, now());
                    }
                    println!(
                        "[sent] to {}",
                        match target {
                            Target::Public => "everyone".to_string(),
                            _ => hex8(&dest),
                        }
                    );
                }
                Err(_) => println!("[error] too large to send in one envelope"),
            }
            continue;
        }

        let (cmd, arg) = match text.split_once(char::is_whitespace) {
            Some((c, a)) => (c, a.trim()),
            None => (text, ""),
        };
        match cmd {
            "/help" => print!("{HELP}"),
            "/quit" | "/exit" => {
                println!("[console] stopping");
                std::process::exit(0);
            }
            "/status" => {
                let (addr, held, peers) = hub.with_node(|n| (n.addr, n.store_len(), n.peers(now()).len()));
                println!("  address   {}", hex8(&addr));
                println!("  home      {}", home.display());
                println!("  store     {held} envelope(s)");
                println!("  peers     {peers} heard from");
                println!(
                    "  target    {}",
                    match target {
                        Target::Public => "everyone".to_string(),
                        Target::Topic(a, _) => format!("topic {}", hex8(&a)),
                        Target::Peer(a) => hex8(&a),
                    }
                );
            }
            "/peers" => {
                let peers = hub.with_node(|n| n.peers(now()));
                if peers.is_empty() {
                    println!("  (nobody yet — a node announces on a trickle, so give it a moment)");
                }
                for (a, age, prekey) in peers {
                    println!(
                        "  {}  heard {age}s ago{}",
                        hex8(&a),
                        if prekey { ", prekey held" } else { ", no prekey (cannot seal to them)" }
                    );
                }
            }
            "/to" => match arg {
                "" | "public" | "everyone" => {
                    target = Target::Public;
                    println!("  target: everyone");
                }
                a if a.starts_with('#') => {
                    let t = spore::topic_of(&a[1..]);
                    target = Target::Topic(t, []);
                    println!("  target: topic {} ({})", &a[1..], hex8(&t));
                }
                a => match unhex8(a) {
                    Some(addr) => {
                        target = Target::Peer(addr);
                        println!("  target: {}", hex8(&addr));
                    }
                    None => println!("  not an address: expected 16 hex digits, #topic, or public"),
                },
            },
            "/offer" => {
                // The same INV `tick` sends on a cadence, on demand.
                //
                // A carrier offers what it holds every INV_OFFER_SECS, which is
                // right in the field — "a neighbour has arrived" is not
                // something a node can observe — and far too slow to watch. This
                // is the diagnostic version: it makes the offline half of a
                // store-and-forward demo take seconds instead of five minutes,
                // and it is the first thing to reach for when custody looks
                // stuck.
                let inv = hub.with_node(|n| n.build_inv(&std::collections::HashSet::new()));
                match Envelope::decode(&inv) {
                    Ok((e, _)) if !e.payload.is_empty() => {
                        hub.originate(vec![spore::Forward::Flood { except: spore::NO_IFACE, bytes: inv }]);
                        println!("  offered {} id(s)", e.payload.len() / 16);
                    }
                    _ => println!("  nothing held to offer"),
                }
            }
            "/history" => {
                let dest = target.dest();
                let Ok(c) = comm.lock() else { continue };
                let msgs = c.threads.messages(&dest);
                if msgs.is_empty() {
                    println!("  (nothing yet)");
                }
                for m in msgs {
                    println!(
                        "  {} {} {}",
                        if m.self_authored { "->" } else { "<-" },
                        match m.status {
                            MessageStatus::Received => " ",
                            MessageStatus::Acked => "✓",
                            _ => "·",
                        },
                        m.body
                    );
                }
            }
            other => println!("  unknown command {other} — /help"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spore::communicator::Communicator;

    fn env(typ: u8, body: &[u8], sign: bool) -> Vec<u8> {
        let mut e = Envelope::new(typ, ZERO_DEST, 1_700_000_000, body.to_vec());
        e.flags |= spore::fl::FLOOD;
        if sign {
            let sk = ed25519_dalek::SigningKey::from_bytes(&[7u8; 32]);
            e.sign(&sk);
        }
        e.wire()
    }

    #[test]
    fn an_announce_is_not_a_message() {
        // It arrives at the delivery sink like anything else. The first version
        // of this printed every one as a line of mojibake and filed it in the
        // conversation — found by running the demo rather than by reading.
        let mut c = Communicator::new();
        let from = on_delivered(&mut c, &env(spore::ty::ANNOUNCE, b"\x01\x02\x03", true));
        assert!(from.is_some(), "it is still attributable");
        assert_eq!(c.threads.conversations().len(), 0, "but it is not a conversation");
    }

    #[test]
    fn an_unsigned_envelope_is_counted_and_never_filed() {
        // The rule the thread store exists to enforce, reaching it through the
        // console: an address the envelope cannot prove must not open a
        // conversation, or anyone in range can write into one.
        let mut c = Communicator::new();
        assert_eq!(on_delivered(&mut c, &env(spore::ty::DATA, b"trust me", false)), None);
        assert_eq!(c.threads.conversations().len(), 0);
        assert_eq!(c.threads.unauthenticated_count(), 1, "counted, not silently dropped");
    }

    #[test]
    fn a_signed_message_opens_a_conversation() {
        let mut c = Communicator::new();
        let from = on_delivered(&mut c, &env(spore::ty::DATA, b"the dam holds", true));
        assert!(from.is_some());
        let rows = c.threads.conversations();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].last_body, "the dam holds");
    }

    #[test]
    fn ciphertext_is_reported_as_bytes_rather_than_printed_as_mojibake() {
        // A sealed payload and a file chunk are both opaque here. Rendering them
        // through `from_utf8_lossy` produces replacement characters that look
        // like corruption rather than like privacy.
        let mut c = Communicator::new();
        on_delivered(&mut c, &env(spore::ty::DATA, &[0xff, 0xfe, 0x00, 0x01], true));
        let body = &c.threads.conversations()[0].last_body;
        assert!(body.starts_with("<4 bytes"), "got {body:?}");
        assert!(!body.contains('\u{fffd}'), "no replacement characters");
    }

    #[test]
    fn a_frame_that_does_not_decode_is_reported_rather_than_filed() {
        let mut c = Communicator::new();
        assert_eq!(on_delivered(&mut c, b"not an envelope at all"), None);
        assert_eq!(c.threads.conversations().len(), 0);
    }

    #[test]
    fn an_address_is_sixteen_hex_digits_and_nothing_else() {
        assert_eq!(unhex8("0102030405060708"), Some([1, 2, 3, 4, 5, 6, 7, 8]));
        assert_eq!(unhex8("0102030405060708aa"), None, "too long");
        assert_eq!(unhex8("010203040506070"), None, "too short");
        assert_eq!(unhex8("0102030405060g08"), None, "not hex");
        assert_eq!(unhex8(""), None);
    }
}
