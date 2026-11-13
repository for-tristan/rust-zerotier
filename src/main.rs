mod eth;
mod state;
mod tap;

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
            eprintln!("[rzt] vswitch on port {port} — not yet implemented");
        }
        Commands::Vport {
            server_ip,
            server_port,
        } => {
            eprintln!("[rzt] vport -> {server_ip}:{server_port} — not yet implemented");
        }
        Commands::Gui => {
            eprintln!("[rzt] gui — not yet implemented");
        }
    }
}
