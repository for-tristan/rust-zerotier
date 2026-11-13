//! Virtual port — faithful Rust port of `vport.c`.
//!
//! A client that allocates a Linux TAP device and relays Ethernet frames in
//! both directions between the kernel network stack (the TAP fd) and a remote
//! VSwitch (a UDP socket), using two threads just like the original:
//!
//! * **up** forwarder: TAP -> VSwitch (`read` the TAP, `send_to` the switch)
//! * **down** forwarder: VSwitch -> TAP (`recv_from` the switch, `write` the TAP)

use crate::eth::EtherHeader;
use crate::state::{self, Direction, Mode, SharedState};
use crate::tap;
use std::io;
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::os::fd::RawFd;
use std::thread;
use std::time::Duration;

/// Maximum Ethernet frame size relayed in either direction (matches the
/// original buffer size).
const MTU: usize = 1518;

/// Default TAP interface name hint (the kernel may pick a different name; the
/// actual name is read back and reported).
const TAP_NAME: &str = "rzt0";

/// Run the virtual port: allocate a TAP device, open a UDP socket to the
/// VSwitch, and spawn the up/down relay threads. Returns when both threads
/// have stopped (either because the shared state was told to stop, or a fatal
/// error occurred).
pub fn run(server_ip: &str, server_port: u16, state: SharedState) -> io::Result<()> {
    // 1. resolve the VSwitch address
    let server_addr: SocketAddr = (server_ip, server_port)
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "unresolved server address"))?;

    // 2. allocate the TAP device
    let (tapfd, ifname) = tap::tap_alloc(TAP_NAME)?;
    set_nonblocking(tapfd)?;

    // 3. create the client UDP socket and aim it at the VSwitch
    let sock = UdpSocket::bind("0.0.0.0:0")?;
    sock.set_read_timeout(Some(Duration::from_millis(100)))?;
    sock.connect(server_addr)?;

    {
        let mut s = state.lock().unwrap();
        s.ifname = Some(ifname.clone());
        s.start(format!("{server_addr} (tap {ifname})"));
    }
    eprintln!(
        "[rzt] vport tap={ifname} <-> {server_addr}  (bring the interface up with: ip link set {ifname} up && ip addr add 10.1.1.101/24 dev {ifname})"
    );

    // 4. spawn the up (TAP -> VSwitch) and down (VSwitch -> TAP) forwarders
    let up_state = state.clone();
    let up_sock = sock.try_clone()?;
    let up = thread::Builder::new()
        .name("vport-up".into())
        .spawn(move || forward_tap_to_vswitch(tapfd, up_sock, server_addr, up_state))?;

    let down = thread::Builder::new()
        .name("vport-down".into())
        .spawn(move || forward_vswitch_to_tap(sock, tapfd, state))?;

    // 5. wait for both to finish, then close the TAP device
    let _ = up.join();
    let _ = down.join();
    unsafe { libc::close(tapfd) };

    eprintln!("[rzt] vport stopped");
    Ok(())
}

/// Up forwarder: read frames from the TAP device and send them to the VSwitch.
/// Uses `poll` with a short timeout so the loop notices a stop request.
fn forward_tap_to_vswitch(tapfd: RawFd, sock: UdpSocket, server: SocketAddr, state: SharedState) {
    let mut buf = [0u8; MTU];
    let mut fds = [libc::pollfd {
        fd: tapfd,
        events: libc::POLLIN,
        revents: 0,
    }];

    loop {
        if !state.lock().unwrap().running {
            break;
        }
        // poll the TAP fd for readability, 100ms timeout
        let ret = unsafe { libc::poll(fds.as_mut_ptr(), 1, 100) };
        if ret == 0 {
            continue; // timed out, re-check running
        }
        if ret < 0 {
            let e = io::Error::last_os_error();
            if e.raw_os_error() == Some(libc::EINTR) {
                continue;
            }
            state.lock().unwrap().fail(format!("tap poll: {e}"));
            break;
        }
        if fds[0].revents & libc::POLLIN == 0 {
            continue;
        }

        let n = unsafe { libc::read(tapfd, buf.as_mut_ptr() as *mut _, buf.len()) };
        if n > 0 {
            let data = &buf[..n as usize];
            let _ = sock.send_to(data, server);
            log_up(&state, data, server);
        } else if n == 0 {
            continue;
        } else {
            let e = io::Error::last_os_error();
            if e.kind() == io::ErrorKind::WouldBlock || e.raw_os_error() == Some(libc::EINTR) {
                continue;
            }
            state.lock().unwrap().fail(format!("tap read: {e}"));
            break;
        }
    }
}

/// Down forwarder: receive frames from the VSwitch and write them to the TAP
/// device. The UDP socket's read timeout makes the loop stoppable.
fn forward_vswitch_to_tap(sock: UdpSocket, tapfd: RawFd, state: SharedState) {
    let mut buf = [0u8; MTU];
    loop {
        if !state.lock().unwrap().running {
            break;
        }
        let (n, peer) = match sock.recv_from(&mut buf) {
            Ok(v) => v,
            Err(ref e)
                if e.kind() == io::ErrorKind::WouldBlock || e.kind() == io::ErrorKind::TimedOut =>
            {
                continue;
            }
            Err(e) => {
                state.lock().unwrap().fail(format!("vswitch recv: {e}"));
                break;
            }
        };
        let data = &buf[..n];
        let written = unsafe { libc::write(tapfd, data.as_ptr() as *const _, data.len()) };
        log_down(&state, data, peer, written);
    }
}

/// Log a frame sent from the TAP up to the VSwitch.
fn log_up(state: &SharedState, data: &[u8], server: SocketAddr) {
    let (src, dst, size) = frame_summary(data);
    let mut s = state.lock().unwrap();
    s.stats.frames_out += 1;
    let echo = s.cli_echo;
    s.log(state::make_log(
        Direction::Out,
        &src,
        &dst,
        size,
        server,
        "tap -> vswitch",
    ));
    if echo {
        eprintln!("[vport] tap -> {server}: {src} -> {dst} ({size}B)");
    }
}

/// Log a frame received from the VSwitch and written to the TAP.
fn log_down(state: &SharedState, data: &[u8], peer: SocketAddr, written: isize) {
    let (src, dst, size) = frame_summary(data);
    let mut s = state.lock().unwrap();
    if written < 0 {
        let e = io::Error::last_os_error();
        s.stats.errors += 1;
        s.log(state::make_log(
            Direction::In,
            &src,
            &dst,
            size,
            peer,
            format!("tap write error: {e}"),
        ));
        if s.cli_echo {
            eprintln!("[vport] {peer} -> tap: write error: {e}");
        }
        return;
    }
    s.stats.frames_in += 1;
    s.log(state::make_log(
        Direction::In,
        &src,
        &dst,
        size,
        peer,
        "vswitch -> tap",
    ));
    if s.cli_echo {
        eprintln!("[vport] {peer} -> tap: {src} -> {dst} ({size}B)");
    }
}

/// (src, dst, size) for a frame, using "?" for an unparseable header.
fn frame_summary(data: &[u8]) -> (String, String, usize) {
    match EtherHeader::parse(data) {
        Some(h) => (h.src.to_string(), h.dst.to_string(), data.len()),
        None => ("?".to_string(), "?".to_string(), data.len()),
    }
}

/// Set a raw file descriptor to non-blocking mode.
fn set_nonblocking(fd: RawFd) -> io::Result<()> {
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 {
        return Err(io::Error::last_os_error());
    }
    let ret = unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) };
    if ret < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// Convenience for the CLI: construct a Port-mode shared state.
pub fn shared_state() -> SharedState {
    state::shared(Mode::Port)
}
