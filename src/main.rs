//! rust-zerotier — an educational L2 VPN (virtual switch + virtual port),
//! a Rust port of build-your-own-zerotier with a minimalist control-panel GUI.

mod eth;
mod state;
mod tap;
mod vport;
mod vswitch;

use clap::{CommandFactory, Parser, Subcommand};

const PKG_VERSION: &str = env!("CARGO_PKG_VERSION");
const ABOUT: &str = "An educational L2 VPN — a virtual switch and virtual port that mimic \
ZeroTier, in Rust, with a minimalist control-panel GUI.";

const LONG_ABOUT: &str = "An educational L2 VPN — a virtual switch and virtual port that mimic \
ZeroTier, in Rust, with a minimalist control-panel GUI.\n\n\
A faithful Rust port of the C + Python project build-your-own-zerotier:\n  \
- vswitch: a UDP server that learns MAC -> endpoint mappings and forwards frames\n  \
- vport:    a TAP device relayed to a remote vswitch over UDP\n  \
- gui:      a black-and-white control panel (live log, MAC table, stats)";

const AFTER_HELP: &str = "EXAMPLES:\n  \
rzt vswitch 9999                       Run the switch on UDP port 9999\n  \
sudo rzt vport 203.0.113.10 9999       Attach this host to a remote switch\n  \
rzt gui                                Launch the control panel\n\n\
The switch needs no privileges. The port needs root (or CAP_NET_ADMIN) to\n\
create a TAP device. Bring the interface up afterwards, e.g.:\n  \
ip link set rzt0 up && ip addr add 10.1.1.101/24 dev rzt0";

/// rust-zerotier command line.
#[derive(Parser)]
#[command(
    name = "rzt",
    version = PKG_VERSION,
    about = ABOUT,
    long_about = LONG_ABOUT,
    after_help = AFTER_HELP,
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run the virtual switch server (UDP, MAC learning, frame forwarding).
    #[command(
        long_about = "Run the virtual switch server.\n\n\
Binds 0.0.0.0:PORT and forwards Ethernet frames between connected VPorts while \
maintaining a source-MAC learning table: unicast to a known destination, \
broadcast to every VPort except the source, discard unknown unicast.\n\n\
Faithful port of vswitch.py."
    )]
    Vswitch {
        /// UDP port to listen on.
        port: u16,
    },
    /// Run the virtual port client (TAP device <-> VSwitch relay).
    #[command(
        long_about = "Run the virtual port client.\n\n\
Allocates a Linux TAP device (default name rzt0) and relays Ethernet frames in \
both directions between the kernel network stack and a remote vswitch over UDP, \
using two threads (one per direction).\n\n\
Faithful port of vport.c. Requires root or CAP_NET_ADMIN."
    )]
    Vport {
        /// VSwitch server IP address (or hostname).
        server_ip: String,
        /// VSwitch server UDP port.
        server_port: u16,
    },
    /// Launch the minimalist control-panel GUI.
    Gui,
}

fn main() {
    let cli = Cli::parse();
    // Print a short banner so the running role is obvious in the terminal.
    eprintln!("rzt {PKG_VERSION} — {ABOUT}");

    let code = run(cli.command);
    std::process::exit(code);
}

/// Dispatch a subcommand. Returns the process exit code.
fn run(command: Commands) -> i32 {
    match command {
        Commands::Vswitch { port } => {
            let state = vswitch::shared_state();
            state.lock().unwrap().cli_echo = true;
            if let Err(e) = vswitch::run(port, state) {
                eprintln!("[rzt] vswitch error: {e}");
                return 1;
            }
            0
        }
        Commands::Vport {
            server_ip,
            server_port,
        } => {
            let state = vport::shared_state();
            state.lock().unwrap().cli_echo = true;
            if let Err(e) = vport::run(&server_ip, server_port, state) {
                eprintln!("[rzt] vport error: {e}");
                return 1;
            }
            0
        }
        Commands::Gui => {
            eprintln!(
                "[rzt] the control-panel GUI is not yet available in this build; \
use `rzt vswitch <port>` or `rzt vport <ip> <port>`."
            );
            0
        }
    }
}

// Re-export the command builder so external callers (and future tests) can
// reach clap's generated metadata, e.g. for `--help` assertions.
#[allow(dead_code)]
pub fn build_command() -> clap::Command {
    Cli::command()
}
