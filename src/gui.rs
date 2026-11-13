//! Minimalist black-and-white control panel (egui).
//!
//! A single window with:
//!   - a header showing role, endpoint, uptime and a running/stopped dot,
//!   - a left panel with the config form (mode / port / VSwitch address),
//!     the Start/Stop button, and the live peer (MAC) table,
//!   - a central panel with a stats strip and the scrolling live traffic log.
//!
//! The networking core (vswitch/vport) runs on a worker thread and talks to
//! the UI exclusively through the shared [`state::SharedState`].

use crate::eth::MacAddr;
use crate::fa;
use crate::state::{self, Direction, LogEntry, Mode, SharedState, Stats};
use crate::{vport, vswitch};
use eframe::egui;
use std::thread;
use std::time::Duration;

/// Launch the control-panel GUI.
pub fn run() -> eframe::Result<()> {
    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("rust-zerotier")
            .with_inner_size([920.0, 640.0]),
        ..Default::default()
    };
    eframe::run_native(
        "rust-zerotier",
        opts,
        Box::new(|cc| {
            fa::install(&cc.egui_ctx);
            theme(&cc.egui_ctx);
            Ok(Box::new(App::new()))
        }),
    )
}

/// A point-in-time copy of the shared state, taken once per frame so the UI
/// never holds the lock while rendering.
struct Snapshot {
    running: bool,
    mode: Mode,
    endpoint: Option<String>,
    ifname: Option<String>,
    uptime: u64,
    stats: Stats,
    peers: Vec<(String, String)>,
    log: Vec<LogEntry>,
    last_error: Option<String>,
}

struct App {
    state: SharedState,
    mode: Mode,
    port: String,
    server_ip: String,
    worker: Option<thread::JoinHandle<()>>,
}

impl App {
    fn new() -> Self {
        App {
            state: state::shared(Mode::Switch),
            mode: Mode::Switch,
            port: "9999".to_string(),
            server_ip: "127.0.0.1".to_string(),
            worker: None,
        }
    }

    fn snapshot(&self) -> Snapshot {
        let s = self.state.lock().unwrap();
        let start = s.log.len().saturating_sub(200);
        let log: Vec<LogEntry> = s.log.iter().skip(start).cloned().collect();
        let peers: Vec<(String, String)> = s
            .mac_table
            .iter()
            .map(|(mac, addr)| (mac.to_string(), addr.to_string()))
            .collect();
        Snapshot {
            running: s.running,
            mode: s.mode,
            endpoint: s.endpoint.clone(),
            ifname: s.ifname.clone(),
            uptime: s.uptime_secs(),
            stats: s.stats,
            peers,
            log,
            last_error: s.last_error.clone(),
        }
    }

    fn is_running(&self) -> bool {
        self.state.lock().unwrap().running
    }

    fn start(&mut self) {
        if self.is_running() {
            return;
        }
        // drop a finished handle from a previous run, if any
        if let Some(h) = self.worker.take() {
            if !h.is_finished() {
                self.worker = Some(h);
                return;
            }
        }
        let port: u16 = match self.port.parse() {
            Ok(p) if p != 0 => p,
            _ => {
                self.state.lock().unwrap().last_error = Some("invalid port".into());
                return;
            }
        };
        let state = self.state.clone();
        {
            let mut s = state.lock().unwrap();
            s.mac_table.clear();
            s.log.clear();
            s.stats = Stats::default();
            s.last_error = None;
            s.mode = self.mode;
        }
        let mode = self.mode;
        let handle = match mode {
            Mode::Switch => thread::Builder::new()
                .name("rzt-vswitch".into())
                .spawn(move || {
                    if let Err(e) = vswitch::run(port, state.clone()) {
                        state.lock().unwrap().fail(format!("{e}"));
                    }
                })
                .ok(),
            Mode::Port => {
                let ip = self.server_ip.clone();
                thread::Builder::new()
                    .name("rzt-vport".into())
                    .spawn(move || {
                        if let Err(e) = vport::run(&ip, port, state.clone()) {
                            state.lock().unwrap().fail(format!("{e}"));
                        }
                    })
                    .ok()
            }
        };
        self.worker = handle;
    }

    fn stop(&mut self) {
        self.state.lock().unwrap().running = false;
        if let Some(h) = self.worker.take() {
            let _ = h.join(); // the network loop exits within its read timeout
        }
    }
}

impl Drop for App {
    fn drop(&mut self) {
        self.state.lock().unwrap().running = false;
        if let Some(h) = self.worker.take() {
            let _ = h.join();
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let snap = self.snapshot();
        if snap.running {
            // keep the log live without burning a full 60 fps
            ctx.request_repaint_after(Duration::from_millis(120));
        }

        egui::TopBottomPanel::top("header")
            .exact_height(34.0)
            .show(ctx, |ui| {
                self.header(ui, &snap);
            });
        egui::SidePanel::left("config")
            .resizable(false)
            .exact_width(312.0)
            .show(ctx, |ui| {
                self.config_panel(ui, &snap);
            });
        egui::CentralPanel::default().show(ctx, |ui| {
            self.central(ui, &snap);
        });
    }
}

// --- panel rendering --------------------------------------------------------

impl App {
    fn header(&self, ui: &mut egui::Ui, snap: &Snapshot) {
        ui.horizontal(|ui| {
            ui.add_space(6.0);
            ui.label(egui::RichText::new(format!("{} rust-zerotier", fa::SERVER)).strong());
            ui.label(
                egui::RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
                    .weak()
                    .color(egui::Color32::from_gray(130)),
            );
            ui.separator();
            ui.label(snap.mode.label());
            if let Some(ep) = &snap.endpoint {
                ui.separator();
                ui.label(egui::RichText::new(ep.clone()).monospace().color(egui::Color32::from_gray(90)));
            }
            if let Some(ifn) = &snap.ifname {
                ui.separator();
                ui.label(
                    egui::RichText::new(format!("tap {ifn}"))
                        .monospace()
                        .color(egui::Color32::from_gray(90)),
                );
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(egui::RichText::new(format!("up {}s", snap.uptime)).weak());
                ui.separator();
                ui.label(if snap.running { "RUNNING" } else { "STOPPED" });
                let (rect, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
                let c = rect.center();
                let p = ui.painter();
                if snap.running {
                    p.circle_filled(c, 4.0, egui::Color32::BLACK);
                } else {
                    p.circle_stroke(c, 3.5, egui::Stroke::new(1.5, egui::Color32::BLACK));
                }
            });
        });
    }

    fn config_panel(&mut self, ui: &mut egui::Ui, snap: &Snapshot) {
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new(format!("{} Configuration", fa::COG))
                .strong(),
        );
        ui.separator();

        ui.label("Role");
        ui.horizontal(|ui| {
            if ui
                .selectable_label(self.mode == Mode::Switch, format!("{} Switch", fa::SERVER))
                .clicked()
            {
                self.mode = Mode::Switch;
            }
            if ui
                .selectable_label(self.mode == Mode::Port, format!("{} Port", fa::NETWORK_WIRED))
                .clicked()
            {
                self.mode = Mode::Port;
            }
        });

        ui.add_space(2.0);
        ui.label("UDP port");
        ui.text_edit_singleline(&mut self.port);

        if self.mode == Mode::Port {
            ui.add_space(2.0);
            ui.label("VSwitch address (ip or host)");
            ui.text_edit_singleline(&mut self.server_ip);
        }

        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if snap.running {
                if secondary_button(ui, &format!("{} Stop", fa::STOP)).clicked() {
                    self.stop();
                }
            } else if primary_button(ui, &format!("{} Start", fa::PLAY)).clicked() {
                self.start();
            }
            if let Some(err) = &snap.last_error {
                ui.separator();
                ui.label(egui::RichText::new(err).color(egui::Color32::BLACK));
            }
        });

        ui.add_space(8.0);
        ui.separator();
        ui.label(
            egui::RichText::new(format!("{} Peers ({})", fa::TABLE, snap.peers.len())).strong(),
        );
        egui::ScrollArea::vertical()
            .max_height(ui.available_height())
            .show(ui, |ui| {
                egui::Grid::new("peers")
                    .num_columns(2)
                    .striped(false)
                    .min_col_width(96.0)
                    .show(ui, |ui| {
                        ui.label(egui::RichText::new("MAC").weak());
                        ui.label(egui::RichText::new("Endpoint").weak());
                        ui.end_row();
                        for (mac, ep) in &snap.peers {
                            ui.label(egui::RichText::new(mac.clone()).monospace());
                            ui.label(egui::RichText::new(ep.clone()).monospace());
                            ui.end_row();
                        }
                    });
            });
    }

    fn central(&self, ui: &mut egui::Ui, snap: &Snapshot) {
        // stats strip
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(format!("{} ", fa::CHART_BAR)).strong());
            stat(ui, "IN", snap.stats.frames_in);
            stat(ui, "OUT", snap.stats.frames_out);
            stat(ui, "FWD", snap.stats.forwarded);
            stat(ui, "BCAST", snap.stats.broadcast);
            stat(ui, "DROP", snap.stats.discarded);
            stat(ui, "ERR", snap.stats.errors);
            ui.separator();
            ui.label(format!("{} peers: {}", fa::PLUG, snap.peers.len()));
        });

        ui.separator();
        ui.label(egui::RichText::new(format!("{} Live traffic", fa::TERMINAL)).strong());
        ui.separator();

        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .stick_to_bottom(true)
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                if snap.log.is_empty() {
                    ui.label(
                        egui::RichText::new(
                            "No traffic yet. Start the switch or port to see frames here.",
                        )
                        .weak(),
                    );
                }
                for e in &snap.log {
                    let line = format!(
                        "{:<12} {:<3} {:<17} -> {:<17} {:>4}  {:<21} {}",
                        e.time_str,
                        e.dir.label(),
                        e.src,
                        e.dst,
                        format!("{}B", e.size),
                        e.peer,
                        e.note,
                    );
                    ui.label(egui::RichText::new(line).monospace());
                }
            });
    }
}

// --- small ui helpers -------------------------------------------------------

fn stat(ui: &mut egui::Ui, label: &str, value: u64) {
    ui.label(egui::RichText::new(label).weak());
    ui.label(egui::RichText::new(format!("{value}")).strong());
    ui.separator();
}

fn primary_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    ui.add(
        egui::Button::new(egui::RichText::new(text).color(egui::Color32::WHITE))
            .fill(egui::Color32::BLACK)
            .stroke(egui::Stroke::new(1.0, egui::Color32::BLACK)),
    )
}

fn secondary_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    ui.add(
        egui::Button::new(egui::RichText::new(text).color(egui::Color32::BLACK))
            .fill(egui::Color32::WHITE)
            .stroke(egui::Stroke::new(1.0, egui::Color32::BLACK)),
    )
}

/// Apply a strict black-on-white, thin-stroke visual theme.
fn theme(ctx: &egui::Context) {
    let black = egui::Color32::BLACK;
    let white = egui::Color32::WHITE;
    let mut vis = egui::Visuals::light();
    // Panels use the noninteractive widget fill, set below.
    vis.faint_bg_color = white;
    vis.extreme_bg_color = egui::Color32::from_gray(248);
    vis.window_fill = white;
    vis.window_stroke = egui::Stroke::new(1.0, black);
    vis.override_text_color = Some(black);

    {
        let w = &mut vis.widgets;
        w.noninteractive.bg_fill = white;
        w.noninteractive.bg_stroke = egui::Stroke::new(1.0, black);
        w.noninteractive.fg_stroke = egui::Stroke::new(1.0, black);
        w.inactive.bg_fill = white;
        w.inactive.bg_stroke = egui::Stroke::new(1.0, black);
        w.inactive.fg_stroke = egui::Stroke::new(1.0, black);
        w.hovered.bg_fill = egui::Color32::from_gray(235);
        w.hovered.bg_stroke = egui::Stroke::new(1.0, black);
        w.hovered.fg_stroke = egui::Stroke::new(1.0, black);
        w.active.bg_fill = black;
        w.active.bg_stroke = egui::Stroke::new(1.0, black);
        w.active.fg_stroke = egui::Stroke::new(1.0, white);
        w.open.bg_fill = white;
        w.open.bg_stroke = egui::Stroke::new(1.0, black);
        w.open.fg_stroke = egui::Stroke::new(1.0, black);
    }
    vis.selection.bg_fill = black;
    vis.selection.stroke = egui::Stroke::new(1.0, black);
    ctx.set_visuals(vis);

    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(6.0, 4.0);
    style.spacing.button_padding = egui::vec2(10.0, 4.0);
    style.spacing.text_edit_width = 200.0;
    ctx.set_style(style);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;

    fn peer(p: u16) -> SocketAddr {
        format!("10.0.0.1:{p}").parse().unwrap()
    }

    #[test]
    fn snapshot_reflects_injected_state() {
        let mut app = App::new();
        {
            let mut s = app.state.lock().unwrap();
            s.learn(MacAddr([1, 2, 3, 4, 5, 6]), peer(7));
            s.log(state::make_log(Direction::In, "aa:bb:cc:dd:ee:01", "ff:ff:ff:ff:ff:ff", 60, "10.0.0.1:7", "broadcast"));
            s.stats.frames_in = 1;
            s.stats.broadcast = 1;
        }
        let snap = app.snapshot();
        assert_eq!(snap.peers.len(), 1);
        assert_eq!(snap.peers[0].0, "01:02:03:04:05:06");
        assert_eq!(snap.log.len(), 1);
        assert_eq!(snap.stats.frames_in, 1);
        assert_eq!(snap.stats.broadcast, 1);
    }

    #[test]
    fn empty_snapshot_is_safe_to_render() {
        let app = App::new();
        let snap = app.snapshot();
        assert!(!snap.running);
        assert!(snap.peers.is_empty());
        assert!(snap.log.is_empty());
        assert!(snap.endpoint.is_none());
    }
}
