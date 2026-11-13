//! Shared, thread-safe application state.
//!
//! The networking core (VSwitch / VPort, running on worker threads) and the
//! GUI (rendering on the main thread) communicate exclusively through a single
//! [`SharedState`] — an `Arc<Mutex<AppState>>`. Every frame seen by the core is
//! logged here, the MAC table is maintained here, and the running counters are
//! bumped here. The GUI takes a short-lived snapshot each repaint.

use crate::eth::MacAddr;
use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// Maximum number of log lines retained for the GUI's live traffic view.
pub const LOG_CAP: usize = 500;

/// Which role this process is playing.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Switch,
    Port,
}

impl Mode {
    pub fn label(self) -> &'static str {
        match self {
            Mode::Switch => "Virtual Switch",
            Mode::Port => "Virtual Port",
        }
    }
}

/// One line in the live traffic log.
#[derive(Clone)]
pub struct LogEntry {
    /// Milliseconds since the process started — used for stable ordering.
    pub ts_ms: u128,
    /// `HH:MM:SS.mmm` wall-clock string for display.
    pub time_str: String,
    /// Whether the frame was received or sent by the local role.
    pub dir: Direction,
    /// Source MAC, formatted.
    pub src: String,
    /// Destination MAC, formatted.
    pub dst: String,
    /// Frame size in bytes.
    pub size: usize,
    /// Remote endpoint (the VPort address for a switch, the VSwitch address
    /// for a port).
    pub peer: String,
    /// Short human-readable note, e.g. "forwarded", "broadcast x3",
    /// "discarded".
    pub note: String,
}

/// Direction of a logged frame relative to the local role.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Direction {
    /// Received from the network or TAP device.
    In,
    /// Sent to the network or TAP device.
    Out,
}

impl Direction {
    pub fn label(self) -> &'static str {
        match self {
            Direction::In => "IN",
            Direction::Out => "OUT",
        }
    }
}

/// Monotonic counters for the stats panel.
#[derive(Clone, Copy, Default, Debug)]
pub struct Stats {
    /// Total frames received by the local role.
    pub frames_in: u64,
    /// Total frames sent by the local role.
    pub frames_out: u64,
    /// Unicast frames forwarded to a known peer (switch).
    pub forwarded: u64,
    /// Broadcast events triggered (switch).
    pub broadcast: u64,
    /// Frames discarded, e.g. unknown unicast (switch).
    pub discarded: u64,
    /// Recoverable errors (bad parse, send failure, ...).
    pub errors: u64,
}

/// The full shared state. Guarded by a `Mutex` behind [`SharedState`].
pub struct AppState {
    pub mode: Mode,
    pub running: bool,
    /// `Switch`: the local `0.0.0.0:port` it bound to. `Port`: the VSwitch
    /// address it dialled.
    pub endpoint: Option<String>,
    /// `Port`: the TAP interface name allocated by the kernel.
    pub ifname: Option<String>,
    pub started_at: Option<Instant>,
    pub mac_table: HashMap<MacAddr, SocketAddr>,
    pub log: VecDeque<LogEntry>,
    pub stats: Stats,
    pub last_error: Option<String>,
}

impl AppState {
    pub fn new(mode: Mode) -> Self {
        AppState {
            mode,
            running: false,
            endpoint: None,
            ifname: None,
            started_at: None,
            mac_table: HashMap::new(),
            log: VecDeque::with_capacity(LOG_CAP),
            stats: Stats::default(),
            last_error: None,
        }
    }

    /// Begin a run: record the bind/dial endpoint and mark running.
    pub fn start(&mut self, endpoint: impl Into<String>) {
        self.endpoint = Some(endpoint.into());
        self.started_at = Some(Instant::now());
        self.running = true;
        self.last_error = None;
    }

    /// Record a fatal error and stop.
    pub fn fail(&mut self, msg: impl Into<String>) {
        self.last_error = Some(msg.into());
        self.running = false;
        self.stats.errors += 1;
    }

    /// Elapsed seconds since [`start`](Self::start), or 0 if not started.
    pub fn uptime_secs(&self) -> u64 {
        self.started_at
            .map(|t| t.elapsed().as_secs())
            .unwrap_or(0)
    }

    /// Learn a source MAC -> endpoint mapping.
    /// Returns `true` if this is a new or changed entry (worth logging).
    pub fn learn(&mut self, src: MacAddr, endpoint: SocketAddr) -> bool {
        match self.mac_table.get(&src) {
            Some(existing) if *existing == endpoint => false,
            _ => {
                self.mac_table.insert(src, endpoint);
                true
            }
        }
    }

    /// Look up the endpoint for a destination MAC.
    pub fn lookup(&self, dst: MacAddr) -> Option<SocketAddr> {
        self.mac_table.get(&dst).copied()
    }

    /// Unique endpoints of every known MAC except the one belonging to `src`.
    /// Mirrors the original broadcast logic: forward to all VPorts except the
    /// source's VPort.
    pub fn broadcast_targets(&self, src: MacAddr) -> Vec<SocketAddr> {
        let mut seen: Vec<SocketAddr> = Vec::new();
        for (mac, addr) in &self.mac_table {
            if *mac != src && !seen.contains(addr) {
                seen.push(*addr);
            }
        }
        seen
    }

    /// Number of peers currently in the MAC table.
    pub fn peer_count(&self) -> usize {
        self.mac_table.len()
    }

    /// Append a log entry, evicting the oldest when the ring is full.
    pub fn log(&mut self, entry: LogEntry) {
        if self.log.len() >= LOG_CAP {
            self.log.pop_front();
        }
        self.log.push_back(entry);
    }
}

/// Thread-safe handle to the shared state, shared between the networking core
/// and the GUI.
pub type SharedState = Arc<Mutex<AppState>>;

/// Convenience constructor.
pub fn shared(mode: Mode) -> SharedState {
    Arc::new(Mutex::new(AppState::new(mode)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, SocketAddrV4};

    fn peer(p: u16) -> SocketAddr {
        SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(10, 0, 0, 1), p))
    }

    fn mac(b: u8) -> MacAddr {
        MacAddr([b, b, b, b, b, b])
    }

    #[test]
    fn learn_reports_new_and_changed() {
        let mut s = AppState::new(Mode::Switch);
        assert!(s.learn(mac(1), peer(100))); // new
        assert!(!s.learn(mac(1), peer(100))); // unchanged
        assert!(s.learn(mac(1), peer(200))); // changed
        assert_eq!(s.peer_count(), 1);
    }

    #[test]
    fn broadcast_targets_exclude_source_and_dedupe() {
        let mut s = AppState::new(Mode::Switch);
        // mac(1) and mac(2) share endpoint 100; mac(3) is on 300.
        s.learn(mac(1), peer(100));
        s.learn(mac(2), peer(100));
        s.learn(mac(3), peer(300));

        // Excluding mac(1): mac(2)->100 (added once), mac(3)->300. Deduped.
        let targets = s.broadcast_targets(mac(1));
        assert_eq!(targets.len(), 2);
        assert!(targets.contains(&peer(100)));
        assert!(targets.contains(&peer(300)));

        // Excluding mac(2): mac(1)->100 still provides endpoint 100, plus 300.
        let targets2 = s.broadcast_targets(mac(2));
        assert_eq!(targets2.len(), 2);
        assert!(targets2.contains(&peer(100)));
        assert!(targets2.contains(&peer(300)));

        // Excluding mac(3): only endpoint 100 (via mac(1) and mac(2), deduped).
        let targets3 = s.broadcast_targets(mac(3));
        assert_eq!(targets3, vec![peer(100)]);
    }

    #[test]
    fn log_ring_evicts_oldest_when_full() {
        let mut s = AppState::new(Mode::Switch);
        for i in 0..(LOG_CAP + 5) as u128 {
            s.log(LogEntry {
                ts_ms: i,
                time_str: String::new(),
                dir: Direction::In,
                src: String::new(),
                dst: String::new(),
                size: 0,
                peer: String::new(),
                note: format!("entry {i}"),
            });
        }
        assert_eq!(s.log.len(), LOG_CAP);
        assert_eq!(s.log.front().unwrap().note, "entry 5");
        assert_eq!(s.log.back().unwrap().note, format!("entry {}", LOG_CAP + 4));
    }

    #[test]
    fn start_sets_uptime_and_endpoint() {
        let mut s = AppState::new(Mode::Port);
        assert_eq!(s.uptime_secs(), 0);
        s.start("1.2.3.4:9999");
        assert!(s.running);
        assert_eq!(s.endpoint.as_deref(), Some("1.2.3.4:9999"));
    }
}
