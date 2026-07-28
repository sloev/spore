//! SPORE reference node + self-contained demo.
//!
//!   cargo run                 # in-memory mesh simulation
//!   cargo run -- udp          # a real node on UDP :7373 with LAN broadcast
//!   cargo run -- http         # an HTTP "bag" bridge on :7373 (push/inv/want)
//!   cargo run -- folder DIR   # a shared-store bridge over a folder of *.spore
//!   cargo run -- tcp [HOST]   # a KISS-over-TCP stream bridge (listen, or connect)
//!   cargo run -- meshtastic   # bridge to a Meshtastic WiFi-UDP broadcast node
//!
//! The simulation drives the exact `Node::on_rx` router used in production; each
//! `-- <mode>` swaps in a different bridge. The router never changes — a bridge
//! only moves envelope bytes in and out of the node.

mod cli;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        cli::sim::sim(); // no config -> the in-memory demo
        return;
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let path = &args[0];
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("cannot read config `{path}`: {e}");
                eprintln!("usage: spore <config.yaml>   (or `spore` for the in-memory demo)");
                return;
            }
        };
        match cli::config::parse_config(&text) {
            Ok(cfg) => cli::run::run_config(cfg),
            Err(e) => eprintln!("config error: {e}"),
        }
    }
}
