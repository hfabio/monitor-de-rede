//! Interface: tela Início (simples) e abas de detalhes técnicos.

mod events;
mod home;
mod monitor_tab;
mod settings;
mod shots;
mod usage;

use crate::config::Config;
use crate::monitor::{compute_stats, now_secs, Category, Monitor, Stats};
use crate::netusage::{self, Usage};
use eframe::egui::{self, Color32, RichText};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;

// ------------------------------------------------------------------ tema

pub const BG: Color32 = Color32::from_rgb(0x14, 0x17, 0x1c);
pub const TOP: Color32 = Color32::from_rgb(0x0e, 0x10, 0x14);
pub const PANEL: Color32 = Color32::from_rgb(0x18, 0x1c, 0x22);
pub const INPUT: Color32 = Color32::from_rgb(0x0f, 0x12, 0x16);
pub const BORDER: Color32 = Color32::from_rgb(0x26, 0x2b, 0x33);
pub const BORDER2: Color32 = Color32::from_rgb(0x33, 0x3a, 0x44);
pub const TEXT: Color32 = Color32::from_rgb(0xe6, 0xe9, 0xee);
pub const MUTED: Color32 = Color32::from_rgb(0xa9, 0xb0, 0xbb);
pub const DIM: Color32 = Color32::from_rgb(0x8b, 0x93, 0xa0);
pub const ACCENT: Color32 = Color32::from_rgb(0x7c, 0xc4, 0xff);
pub const GREEN: Color32 = Color32::from_rgb(0x4a, 0xde, 0x80);
pub const AMBER: Color32 = Color32::from_rgb(0xf5, 0xa5, 0x24);
pub const RED: Color32 = Color32::from_rgb(0xf8, 0x71, 0x71);
pub const PURPLE: Color32 = Color32::from_rgb(0xc0, 0x84, 0xfc);
pub const PINK: Color32 = Color32::from_rgb(0xf4, 0x72, 0xb6);

pub const SERIES: [Color32; 10] = [
    ACCENT,
    AMBER,
    PURPLE,
    GREEN,
    PINK,
    Color32::from_rgb(0x2d, 0xd4, 0xbf),
    Color32::from_rgb(0xfb, 0x92, 0x3c),
    Color32::from_rgb(0xa3, 0xe6, 0x35),
    Color32::from_rgb(0x81, 0x8c, 0xf8),
    Color32::from_rgb(0xe8, 0x79, 0xf9),
];

pub use crate::assess::{Level, BAD_LOSS_PCT, GOOD_MS};

fn apply_theme(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    v.panel_fill = BG;
    v.window_fill = PANEL;
    v.extreme_bg_color = INPUT;
    v.faint_bg_color = Color32::from_rgb(0x17, 0x1b, 0x21);
    v.override_text_color = Some(TEXT);
    v.selection.bg_fill = ACCENT.gamma_multiply(0.45);
    v.hyperlink_color = ACCENT;
    v.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, BORDER);
    for w in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
        w.corner_radius = 6.0.into();
    }
    v.widgets.inactive.weak_bg_fill = Color32::from_rgb(0x1f, 0x24, 0x2c);
    v.widgets.inactive.bg_fill = Color32::from_rgb(0x1f, 0x24, 0x2c);
    v.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, BORDER2);
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(0x26, 0x2b, 0x33);
    v.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, ACCENT.gamma_multiply(0.6));
    ctx.set_theme(egui::Theme::Dark);
    ctx.set_visuals_of(egui::Theme::Dark, v);
    ctx.style_mut_of(egui::Theme::Dark, |s| {
        s.spacing.item_spacing = egui::vec2(8.0, 8.0);
        s.spacing.button_padding = egui::vec2(12.0, 6.0);
    });
}

// ------------------------------------------------------------- estado

#[derive(Clone, Copy, PartialEq)]
enum View {
    Home,
    Tab(Tab),
}

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Monitor,
    Usage,
    Events,
    Settings,
}

#[derive(Clone, Copy, PartialEq)]
enum Chart {
    Latency,
    Jitter,
    Loss,
}

impl Level {
    pub fn color(self) -> Color32 {
        match self {
            Level::Waiting => DIM,
            Level::Good => GREEN,
            Level::Slow => AMBER,
            Level::Bad => RED,
        }
    }
}

pub struct App {
    cfg: Config,
    draft: Config,
    monitor: Monitor,
    usage: Arc<Mutex<Usage>>,
    view: View,
    window_secs: f64,
    chart: Chart,
    filters: HashSet<Category>,
    new_name: String,
    new_host: String,
    message: Option<(String, bool)>,
    autostart: bool,
    shots: Option<shots::Shots>,
    tray: Option<crate::tray::Tray>,
    tray_updated: std::time::Instant,
    /// Painel no celular (None = desligado ou porta ocupada).
    web: Option<crate::web::Server>,
    web_error: Option<String>,
    /// Cartão do MCP mostrando a configuração pela rede (em vez da local).
    mcp_show_lan: bool,
    updater: std::sync::Arc<crate::update::Updater>,
}

/// Cópia do estado de um alvo para desenhar sem segurar o lock.
pub struct Row {
    pub name: String,
    pub host: String,
    pub method: &'static str,
    pub ip: Option<String>,
    pub error: Option<String>,
    pub local: bool,
    pub in_outage: bool,
    /// Último minuto (tela Início).
    pub recent: Stats,
    /// Média das 5 últimas respostas (número grande dos cartões).
    pub current: Option<f64>,
    /// Janela escolhida no gráfico.
    pub win: Stats,
    pub latency: Vec<[f64; 2]>,
    pub jitter: Vec<[f64; 2]>,
    pub losses: Vec<[f64; 2]>,
    /// (início do bloco relativo a agora, % de perda).
    pub loss_buckets: Vec<(f64, f64)>,
}

impl Row {
    pub fn status(&self) -> (Level, &'static str) {
        crate::assess::target_status(&self.recent, self.error.is_some(), self.in_outage)
    }
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        apply_theme(&cc.egui_ctx);
        let cfg = Config::load();
        let monitor = Monitor::new(&cfg);
        let usage = netusage::spawn();
        crate::status::spawn_writer(monitor.shared.clone(), usage.clone());
        let tray = crate::tray::install(cc, monitor.shared.clone(), usage.clone());
        crate::instance::serve(cc.egui_ctx.clone());
        let repaint = cc.egui_ctx.clone();
        let updater = crate::update::spawn(cfg.check_updates, cfg.auto_update, move || repaint.request_repaint());
        // O app pode ter mudado de pasta (atualização, cópia para Aplicativos):
        // regrava o "iniciar com o sistema" com o caminho atual.
        if crate::autostart::is_enabled() {
            let _ = crate::autostart::set(true);
        }
        let mut app = Self {
            draft: cfg.clone(),
            cfg,
            monitor,
            usage,
            view: View::Home,
            window_secs: 300.0,
            chart: Chart::Latency,
            filters: [Category::Queda, Category::Pico, Category::Perda, Category::Dns].into_iter().collect(),
            new_name: String::new(),
            new_host: String::new(),
            message: None,
            autostart: crate::autostart::is_enabled(),
            shots: shots::Shots::from_env(),
            tray,
            tray_updated: std::time::Instant::now(),
            web: None,
            web_error: None,
            mcp_show_lan: false,
            updater,
        };
        app.restart_web();
        app
    }

    /// (Re)inicia o painel no celular (e o MCP pela rede) conforme a configuração.
    fn restart_web(&mut self) {
        if self.cfg.mcp_token.is_empty() {
            self.cfg.mcp_token = crate::web::new_token();
            self.draft.mcp_token = self.cfg.mcp_token.clone();
            let _ = self.cfg.save();
        }
        let token = (self.cfg.web_enabled && self.cfg.mcp_lan).then(|| self.cfg.mcp_token.clone());
        if self.web.as_ref().is_some_and(|w| self.cfg.web_enabled && w.port() == self.cfg.web_port && w.mcp_token() == token.as_deref()) {
            return;
        }
        self.web = None; // para o anterior e libera a porta
        self.web_error = None;
        if !self.cfg.web_enabled {
            return;
        }
        let (shared, usage) = (self.monitor.shared.clone(), self.usage.clone());
        let status: crate::web::StatusFn = std::sync::Arc::new(move || crate::web::status(&shared, &usage));
        match crate::web::Server::start(self.cfg.web_port, status, token) {
            Ok(s) => self.web = Some(s),
            Err(e) => self.web_error = Some(format!("Não consegui abrir a porta {}: {e}", self.cfg.web_port)),
        }
    }

    fn snapshot(&self) -> (Vec<Row>, f64) {
        let now = now_secs();
        let from = now - self.window_secs;
        let spike = self.cfg.spike_ms as f64;
        let bucket = (self.window_secs / 10.0).max(6.0);
        let sh = self.monitor.shared.lock().unwrap();
        let rows = sh
            .targets
            .iter()
            .map(|t| {
                let recent = compute_stats(t.samples.iter().filter(|s| s.t >= now - 60.0), spike);
                let replies: Vec<f64> = t.samples.iter().rev().filter_map(|s| s.rtt).take(5).collect();
                let current = (!replies.is_empty()).then(|| replies.iter().sum::<f64>() / replies.len() as f64);
                let win_iter = t.samples.iter().filter(|s| s.t >= from);
                let win = compute_stats(win_iter.clone(), spike);

                let (mut latency, mut jitter, mut losses) = (Vec::new(), Vec::new(), Vec::new());
                let mut prev = None;
                let n_buckets = (self.window_secs / bucket).ceil() as usize;
                let mut b = vec![(0usize, 0usize); n_buckets];
                for s in win_iter {
                    let x = s.t - now;
                    let bi = (((s.t - from) / bucket) as usize).min(n_buckets - 1);
                    b[bi].0 += 1;
                    match s.rtt {
                        Some(r) => {
                            latency.push([x, r]);
                            if let Some(p) = prev {
                                jitter.push([x, f64::abs(r - p)]);
                            }
                            prev = Some(r);
                        }
                        None => {
                            losses.push([x, 0.0]);
                            b[bi].1 += 1;
                        }
                    }
                }
                let loss_buckets = b
                    .iter()
                    .enumerate()
                    .filter(|(_, (n, _))| *n > 0)
                    .map(|(i, (n, l))| (-self.window_secs + i as f64 * bucket, *l as f64 * 100.0 / *n as f64))
                    .collect();

                Row {
                    name: t.name.clone(),
                    host: t.host.clone(),
                    method: t.method,
                    ip: t.resolved.map(|ip| ip.to_string()),
                    error: t.error.clone(),
                    local: t.local,
                    in_outage: t.outage_since.is_some(),
                    recent,
                    current,
                    win,
                    latency,
                    jitter,
                    losses,
                    loss_buckets,
                }
            })
            .collect();
        (rows, sh.started)
    }

    fn apply_settings(&mut self) {
        self.draft.targets.retain(|t| !t.host.trim().is_empty());
        for t in &mut self.draft.targets {
            t.host = t.host.trim().to_string();
            t.name = t.name.trim().to_string();
        }
        self.cfg = self.draft.clone();
        self.monitor.apply(&self.cfg);
        self.restart_web();
        self.updater.auto.store(self.cfg.auto_update, std::sync::atomic::Ordering::Relaxed);
        self.updater.check.store(self.cfg.check_updates, std::sync::atomic::Ordering::Relaxed);
        self.message = Some(match self.cfg.save() {
            Ok(()) => ("Configuração salva e aplicada.".into(), true),
            Err(e) => (format!("Aplicado, mas não consegui salvar: {e}"), false),
        });
    }

    /// Aviso de atualização no topo da janela.
    fn update_badge(&mut self, ui: &mut egui::Ui) {
        use crate::update::State;
        match self.updater.state() {
            State::Available(r) => {
                let text = format!("Atualizar para a versão {}", r.version);
                if ui.add(egui::Button::new(RichText::new(text).size(12.0).color(ACCENT)).frame(false)).clicked() {
                    self.start_update(r);
                }
            }
            State::Installing(v) => {
                ui.label(RichText::new(format!("Atualizando para a {v}…")).size(12.0).color(ACCENT));
            }
            _ => {}
        }
    }

    fn start_update(&mut self, r: crate::update::Release) {
        let up = self.updater.clone();
        std::thread::spawn(move || crate::update::install(&up, &r, true));
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        let frame = egui::Frame::new().fill(TOP).inner_margin(egui::Margin::symmetric(14, 8));
        egui::Panel::top("titulo").frame(frame).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Monitor de Rede").color(MUTED));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if let View::Tab(_) = self.view {
                        if ui.add(egui::Button::new(RichText::new("← Início").color(MUTED)).frame(false)).clicked() {
                            self.view = View::Home;
                        }
                        ui.add_space(12.0);
                    }
                    self.update_badge(ui);
                });
            });
        });

        let View::Tab(current) = self.view else { return };
        let frame = egui::Frame::new().fill(BG).inner_margin(egui::Margin { left: 16, right: 16, top: 10, bottom: 0 });
        egui::Panel::top("abas").frame(frame).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                for (tab, label) in [(Tab::Monitor, "Monitor"), (Tab::Usage, "Uso da rede"), (Tab::Events, "Eventos"), (Tab::Settings, "Configurações")] {
                    if tab_button(ui, label, current == tab).clicked() {
                        self.view = View::Tab(tab);
                    }
                }
            });
            let r = ui.max_rect();
            ui.painter().hline(r.x_range(), r.bottom() + 1.0, egui::Stroke::new(1.0, BORDER));
        });
    }
}

fn tab_button(ui: &mut egui::Ui, label: &str, selected: bool) -> egui::Response {
    let text = RichText::new(label).size(14.0).color(if selected { Color32::WHITE } else { MUTED });
    let text = if selected { text.strong() } else { text };
    let btn = egui::Button::new(text)
        .fill(if selected { Color32::from_rgb(0x1f, 0x24, 0x2c) } else { Color32::TRANSPARENT })
        .stroke(egui::Stroke::NONE)
        .corner_radius(egui::CornerRadius { nw: 6, ne: 6, sw: 0, se: 0 })
        .min_size(egui::vec2(0.0, 38.0));
    let resp = ui.add(btn);
    if selected {
        let r = resp.rect;
        ui.painter().hline(r.x_range(), r.bottom() - 1.0, egui::Stroke::new(2.0, ACCENT));
    }
    resp
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.ctx().request_repaint_after(Duration::from_millis(500));
        let ctx = ui.ctx().clone();
        self.drive_shots(&ctx);

        // Com bandeja, fechar a janela só a esconde; o app segue monitorando.
        if self.tray.is_some() && ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }
        if self.tray.is_some() && self.tray_updated.elapsed() > Duration::from_secs(2) {
            self.tray_updated = std::time::Instant::now();
            let (rows, _) = self.snapshot();
            let (level, title, _) = home::overall(&rows);
            if let Some(t) = self.tray.as_mut() {
                t.update(level, &format!("Monitor de Rede — {title}"));
            }
        }
        self.top_bar(ui);
        let frame = egui::Frame::new().fill(BG).inner_margin(16);
        egui::CentralPanel::default_margins().frame(frame).show(ui, |ui| match self.view {
            View::Home => self.ui_home(ui),
            View::Tab(Tab::Monitor) => self.ui_monitor(ui),
            View::Tab(Tab::Usage) => self.ui_usage(ui),
            View::Tab(Tab::Events) => self.ui_events(ui),
            View::Tab(Tab::Settings) => self.ui_settings(ui),
        });
    }
}

// ----------------------------------------------------------- utilitários

pub fn dot(ui: &mut egui::Ui, c: Color32, size: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    ui.painter().circle_filled(rect.center(), size / 2.0, c);
}

pub fn ms(v: f64) -> String {
    if v < 10.0 {
        format!("{v:.1} ms").replace('.', ",")
    } else {
        format!("{v:.0} ms")
    }
}

pub fn pct(v: f64) -> String {
    format!("{v:.1}%").replace('.', ",")
}

pub fn fmt_ago(secs: f64) -> String {
    let s = secs.max(0.0).round() as i64;
    match s {
        0 => "agora".into(),
        1..=59 => format!("-{s}s"),
        _ if s % 60 == 0 => format!("-{}m", s / 60),
        _ => format!("-{}m{:02}", s / 60, s % 60),
    }
}

pub fn card<R>(ui: &mut egui::Ui, stroke: Color32, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::new()
        .fill(PANEL)
        .stroke(egui::Stroke::new(1.0, stroke))
        .corner_radius(10.0)
        .inner_margin(14)
        .show(ui, add)
        .inner
}

pub fn primary_button(text: &str) -> egui::Button<'_> {
    egui::Button::new(RichText::new(text).strong().color(TOP)).fill(ACCENT).stroke(egui::Stroke::NONE).min_size(egui::vec2(0.0, 34.0))
}

pub fn open_folder(path: &std::path::Path) {
    let _ = std::fs::create_dir_all(path);
    let cmd = if cfg!(windows) {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    let _ = std::process::Command::new(cmd).arg(path).spawn();
}
