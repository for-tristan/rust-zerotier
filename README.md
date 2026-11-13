# rust-zerotier

A Rust port of an educational L2 VPN: a virtual switch and a virtual port that
together mimic the behaviour of ZeroTier, with a minimalist black-and-white
control-panel GUI. Devices connected to the switch's ports appear to share one
local network, even when the ports are tunnels reaching across the Internet.

> Inspired by [`peiyuanix/build-your-own-zerotier`](https://github.com/peiyuanix/build-your-own-zerotier),
> an educational C + Python implementation of the same concept. The original
> idea and the switch/port model are by Liu Peiyuan.

This is a **re-implementation, not a fork**: no code is shared with the
original. It is written from scratch in Rust, ships a single binary with three
subcommands, and adds a GUI on top of the same faithful L2-relay logic.

## What it does

A physical network switch forwards Ethernet frames between its ports by MAC
address. This project virtualises that:

- **VSwitch** - a UDP server that plays the switch. It learns which source MAC
  lives behind which VPort endpoint, then forwards each frame: unicast to a
  known destination, broadcast to every VPort except the source, or discard for
  unknown unicast.
- **VPort** - a client that plays one switch port. It allocates a Linux TAP
  device on the local host and relays Ethernet frames in both directions between
  the kernel network stack and a remote VSwitch over UDP.
- **GUI** - a minimalist control panel that runs either role in a worker thread
  and shows a live traffic log, the MAC/peer table, and running counters.

Like the original, this is a teaching tool: there is **no authentication and no
cryptography**. Traffic between VSwitch and VPort is plaintext UDP. Do not use
it as a real VPN.

## Architecture

```
                VSwitch (UDP server, 0.0.0.0:PORT)
                ,--------- MAC table ---------.
                |  src MAC  ->  VPort endpoint |
                `------------------------------'
                   ^                |
            Ethernet frame    Ethernet frame
                   |                v
   host A          |                |              host B
  TAP rzt0     VPort(client)   VPort(client)     TAP rzt0
   [kernel] <--> [tap<->udp]   [tap<->udp] <--> [kernel]
```

A single `Arc<Mutex<AppState>>` is the only channel between the networking
core (running on a worker thread) and the GUI (rendering on the main thread):
every frame seen is logged there, the MAC table lives there, and the counters
are bumped there. The GUI snapshots it once per repaint.

## Build prerequisites

- A Rust toolchain (stable). Install via <https://rustup.rs>.
- A Linux system with TAP support (the `tun` kernel module and `/dev/net/tun`).
- The GUI links against a few X11 system libraries. On Debian/Ubuntu:

  ```sh
  sudo apt install -y pkg-config libxkbcommon-dev libxkbcommon-x11-dev
  ```

  Most desktops already pull these in; they are only needed to build the GUI
  binary. The `vswitch` and `vport` subcommands need nothing beyond the Rust
  toolchain.

  The Wayland backend is disabled in `Cargo.toml` to keep the build lean; the
  X11 backend works on X11 sessions and under XWayland. Re-enable the `wayland`
  feature of `eframe` if you want native Wayland.

## Build

```sh
cargo build            # debug binary at target/debug/rzt
cargo build --release  # optimised binary at target/release/rzt
cargo test             # unit tests
cargo clippy --all-targets  # lints (should be clean)
```

## Usage

The binary is `rzt`. Three subcommands:

```sh
# Run the switch on UDP port 9999 (no privileges needed).
rzt vswitch 9999

# Attach this host to a remote switch (needs root / CAP_NET_ADMIN for the TAP).
sudo rzt vport 203.0.113.10 9999

# Launch the control-panel GUI.
rzt gui
```

`rzt --help` and `rzt <subcommand> --help` print full descriptions and
examples.

### Bringing the TAP interface up

After starting a VPort, the `rzt0` TAP device exists but is down. Configure it
(adjust the address to suit your virtual subnet):

```sh
sudo ./setup.sh                 # defaults to rzt0, 10.1.1.101/24
sudo ./setup.sh rzt1 10.1.1.102 # or: interface name and address
```

Do the same on every host running a VPort, giving each a distinct address in
the same subnet. They should then ping each other as if on one LAN.

### A two-host example

On the switch host (any reachable address, here `203.0.113.10`):

```sh
rzt vswitch 9999
```

On host A and host B:

```sh
sudo rzt vport 203.0.113.10 9999
sudo ./setup.sh rzt0 10.1.1.101/24   # on host A
sudo ./setup.sh rzt0 10.1.1.102/24   # on host B
ping 10.1.1.102                       # from host A
```

The CLI prints one concise line per frame to stderr. Run `rzt gui` instead to
get the same information as a live panel.

## The GUI

`rzt gui` opens a black-and-white window with:

- a **header** showing the role, the bound/dialled endpoint, the TAP name,
  uptime, and a filled/hollow running dot;
- a **config panel** to pick Switch or Port, set the port and (for Port) the
  VSwitch address, and Start/Stop;
- a **peer table** of the current MAC-to-endpoint mappings;
- a **stats strip** (frames in/out, forwarded, broadcast, discarded, errors,
  peer count);
- a **live traffic log** that scrolls and sticks to the newest frame.

All icons are FontAwesome 6 Solid glyphs (the TTF is embedded in the binary);
no emoji are used. Start spawns the selected role on a worker thread sharing
the state with the UI; Stop signals it to exit and joins.

## Project layout

```
rust-zerotier/
  Cargo.toml         package, deps (clap, libc, eframe), release profile
  setup.sh            bring the rzt0 TAP interface up
  assets/
    fa-solid-900.ttf  FontAwesome 6 Solid, embedded via include_bytes!
  src/
    main.rs           CLI: clap subcommands + dispatch
    eth.rs            MAC address + 14-byte Ethernet header parsing
    tap.rs            Linux TAP device allocation (/dev/net/tun + TUNSETIFF)
    state.rs          thread-safe shared state (MAC table, log ring, stats)
    vswitch.rs        UDP server, MAC learning, forward/broadcast/discard
    vport.rs          TAP <-> UDP relay, two threads
    fa.rs             embedded FontAwesome font + icon codepoints
    gui.rs            egui control panel (black/white theme)
```

## Differences from the original

| | original (build-your-own-zerotier) | rust-zerotier |
| --- | --- | --- |
| Language | C + Python | Rust |
| Interface | CLI only | CLI + GUI |
| Binary | two programs (`vport`, `vswitch.py`) | one binary, three subcommands |
| MAC table | Python dict | `HashMap` behind `Arc<Mutex<_>>` |
| Threads | pthreads (two relay threads) | `std::thread` (two relay threads) |
| Networking | blocking sockets | blocking sockets (`std::net`) |
| Security | none (plaintext UDP) | none (plaintext UDP, faithful) |

The forwarding logic is a faithful port: learn the source MAC, forward unicast
to known destinations, broadcast to all VPorts except the source, discard
unknown unicast.

## License

MIT - see [LICENSE](LICENSE).
