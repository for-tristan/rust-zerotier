//! Virtual switch — faithful Rust port of `vswitch.py`.
//!
//! A UDP server that learns source-MAC -> VPort-endpoint mappings and
//! forwards Ethernet frames: unicast to a known destination, broadcast to
//! every known VPort except the source, or discard for unknown unicast.

use crate::eth::EtherHeader;
use crate::state::{self, Direction, Mode, SharedState};
use std::io;
use std::net::{SocketAddr, UdpSocket};

/// Maximum Ethernet frame size the switch will accept (matches the original).
const MTU: usize = 1518;

/// Run the virtual switch: bind `0.0.0.0:port` and service VPorts until the
/// shared state is told to stop (or a fatal error occurs).
pub fn run(port: u16, state: SharedState) -> io::Result<()> {
    let bind_addr = format!("0.0.0.0:{port}");
    let sock = UdpSocket::bind(&bind_addr)?;
    // Short read timeout so the loop notices a stop request from the GUI
    // instead of blocking forever in recv_from.
    sock.set_read_timeout(Some(std::time::Duration::from_millis(100)))?;

    {
        let mut s = state.lock().unwrap();
        s.start(format!("0.0.0.0:{port}"));
    }
    eprintln!("[rzt] vswitch listening on {bind_addr}");

    let mut buf = [0u8; MTU];
    loop {
        if !state.lock().unwrap().running {
            break;
        }
        let (n, peer) = match sock.recv_from(&mut buf) {
            Ok(v) => v,
            Err(ref e)
                if e.kind() == io::ErrorKind::WouldBlock
                    || e.kind() == io::ErrorKind::TimedOut =>
            {
                continue;
            }
            Err(e) => {
                state.lock().unwrap().fail(format!("recv_from: {e}"));
                return Err(e);
            }
        };
        handle_frame(&sock, &buf[..n], peer, &state);
    }

    eprintln!("[rzt] vswitch stopped");
    Ok(())
}

/// Process one received Ethernet frame: learn the source, then forward,
/// broadcast or discard — mirroring `vswitch.py` step by step.
fn handle_frame(sock: &UdpSocket, data: &[u8], peer: SocketAddr, state: &SharedState) {
    let header = match EtherHeader::parse(data) {
        Some(h) => h,
        None => {
            let mut s = state.lock().unwrap();
            s.stats.errors += 1;
            s.log(state::make_log(
                Direction::In,
                "?",
                "?",
                data.len(),
                peer,
                "short frame (no ethernet header)",
            ));
            return;
        }
    };

    let src = header.src;
    let dst = header.dst;
    let size = data.len();

    let mut s = state.lock().unwrap();
    s.stats.frames_in += 1;
    let echo = s.cli_echo;

    // 3. learn the source MAC -> endpoint mapping
    let learned = s.learn(src, peer);
    if learned {
        s.log(state::make_log(
            Direction::In,
            src,
            dst,
            size,
            peer,
            format!("learned {src} @ {peer}"),
        ));
        if echo {
            eprintln!("[vswitch] learned {src} @ {peer}");
        }
    }

    // 4. forward
    if let Some(target) = s.lookup(dst) {
        // unicast forward to the known destination
        match sock.send_to(data, target) {
            Ok(_) => {
                s.stats.frames_out += 1;
                s.stats.forwarded += 1;
                s.log(state::make_log(
                    Direction::In,
                    src,
                    dst,
                    size,
                    peer,
                    format!("forwarded -> {target}"),
                ));
                if echo {
                    eprintln!("[vswitch] {src} -> {dst} ({size}B) forwarded to {target}");
                }
            }
            Err(e) => {
                s.stats.errors += 1;
                s.log(state::make_log(
                    Direction::In,
                    src,
                    dst,
                    size,
                    peer,
                    format!("forward error: {e}"),
                ));
                if echo {
                    eprintln!("[vswitch] {src} -> {dst} ({size}B) forward error: {e}");
                }
            }
        }
    } else if dst.is_broadcast() {
        // broadcast to every known VPort except the source's
        let targets = s.broadcast_targets(src);
        let n = targets.len();
        let mut ok = 0usize;
        for t in &targets {
            if sock.send_to(data, t).is_ok() {
                ok += 1;
            }
        }
        s.stats.frames_out += ok as u64;
        s.stats.broadcast += 1;
        s.log(state::make_log(
            Direction::In,
            src,
            dst,
            size,
            peer,
            format!("broadcast x{n} ({ok} sent)"),
        ));
        if echo {
            eprintln!("[vswitch] {src} -> {dst} ({size}B) broadcast x{n} ({ok} sent)");
        }
    } else {
        // unknown unicast: discard, as in the original
        s.stats.discarded += 1;
        s.log(state::make_log(Direction::In, src, dst, size, peer, "discarded"));
        if echo {
            eprintln!("[vswitch] {src} -> {dst} ({size}B) discarded");
        }
    }
}

/// Convenience for the CLI: construct a Switch-mode shared state.
pub fn shared_state() -> SharedState {
    state::shared(Mode::Switch)
}
