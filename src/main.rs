//! SPORE reference node + self-contained demo.
//!
//!   spore                      # in-memory mesh simulation, no state touched
//!   spore --help               # what follows, and where state lives
//!   spore <config.yaml>        # a node with every bridge the file names
//!   spore <bridge-spec>        # a node with exactly one bridge
//!
//! A **bridge spec** is one entry from the config's `bridges:` list, given on
//! the command line — `spore broadcast`, `spore audio`,
//! `spore meshtastic-serial:/dev/ttyUSB0`. Every hardware procedure in
//! `docs/HARDWARE.md` is written that way, and until this existed none of them
//! ran: the binary took its first argument as a file path, so `spore broadcast`
//! answered "cannot read config `broadcast`". A checklist naming a command that
//! does not exist cannot be followed (#303).
//!
//! The simulation drives the exact `Node::on_rx` router used in production; a
//! bridge only moves envelope bytes in and out of the node.

mod cli;

#[cfg(not(target_arch = "wasm32"))]
const USAGE: &str = "\
spore — a store-and-forward node

  spore                    in-memory mesh simulation (no files touched)
  spore <config.yaml>      a node with every bridge the file names
  spore <bridge-spec>      a node with exactly one bridge
  spore --help             this

Bridge specs are config `bridges:` entries, given directly:

  spore broadcast                      UDP broadcast on the LAN
  spore udp:7373                       UDP on a port
  spore tcp[:HOST:PORT]                KISS over TCP (listen, or connect)
  spore folder:DIR                     a shared folder of *.spore files
  spore audio                          the audio modem
  spore meshtastic                     a Meshtastic node over WiFi-UDP
  spore meshtastic-serial:/dev/ttyUSB0 a Meshtastic node over USB
  spore http:7373                      an HTTP bag (push/inv/want)

State lives in $SPORE_HOME, else $XDG_DATA_HOME/spore, else
~/.local/share/spore (%APPDATA%\\spore on Windows):

  seed       this node's identity. Key material — treat a backup of it as such.
  prekeys    what it can still decrypt after a rotation.
  store/     what it is carrying, including for other people.

Identity and store survive a restart. Delete the home directory to become a
different node.
";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        cli::sim::sim(); // no config -> the in-memory demo
        return;
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let arg = &args[0];
        if arg == "--help" || arg == "-h" || arg == "help" {
            print!("{USAGE}");
            return;
        }
        // A readable file is a config. Anything else is tried as a single
        // bridge spec, so the commands the documentation has always printed
        // finally do what they say.
        match std::fs::read_to_string(arg) {
            Ok(text) => match cli::config::parse_config(&text) {
                Ok(cfg) => cli::run::run_config(cfg),
                Err(e) => eprintln!("config error: {e}"),
            },
            Err(file_err) => match cli::config::one_bridge(&args.join(" ")) {
                Ok(cfg) => cli::run::run_config(cfg),
                // Both readings failed, so say both: a typo in a path and an
                // unknown bridge name look identical from one message, and the
                // reader is the one who knows which they meant.
                Err(spec_err) => {
                    eprintln!("`{arg}` is neither a config file nor a bridge.");
                    eprintln!("  as a file:   {file_err}");
                    eprintln!("  as a bridge: {spec_err}");
                    eprintln!("\nTry `spore --help`.");
                    std::process::exit(2);
                }
            },
        }
    }
}
