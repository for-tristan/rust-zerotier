mod eth;
mod state;
mod tap;
mod vport;
mod vswitch;

use clap::{Parser, Subcommand};

/// rust-zerotier — an educational L2 VPN (virtual switch + virtual port).
#[derive(Parser)]
#[command(name = "rzt", version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run the virtual switch server (UDP, MAC learning, frame forwarding).
    Vswitch {
        /// UDP port to listen on.
        port: u16,
    },
    /// Run the virtual port client (TAP device <-> VSwitch relay).
    Vport {
        /// VSwitch server IP address.
        server_ip: String,
        /// VSwitch server UDP port.
        server_port: u16,
    },
    /// Launch the minimalist control-panel GUI.
    Gui,
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Commands::Vswitch { port } => {
            let state = vswitch::shared_state();
            state.lock().unwrap().cli_echo = true;
            if let Err(e) = vswitch::run(port, state) {
                eprintln!("[rzt] vswitch error: {e}");
                std::process::exit(1);
            }
        }
        Commands::Vport {
            server_ip,
            server_port,
        } => {
            let state = vport::shared_state();
            state.lock().unwrap().cli_echo = true;
            if let Err(e) = vport::run(&server_ip, server_port, state) {
                eprintln!("[rzt] vport error: {e}");
                std::process::exit(1);
            }
        }
        Commands::Gui => {
            eprintln!("[rzt] gui — not yet implemented");
        }
    }
}
