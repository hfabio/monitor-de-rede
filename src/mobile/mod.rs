//! Interface do Android: quatro telas com barra de navegação embaixo.
//!
//! - Início: veredito da casa (este celular + PC da casa) e últimos avisos.
//! - Este celular: medições feitas pelo próprio celular, só com o app aberto.
//! - PC da casa: status lido do painel do app de desktop pela rede local.
//! - Ajustes: conectar ao PC e opções de medição.

mod pc;

use crate::assess::{self, Level, GOOD_MS};
use crate::config::{app_dir, Config, Target, SUGGESTIONS};
use crate::monitor::{compute_stats, now_secs, EventKind, Monitor, Stats};
use chrono::{Local, TimeZone};
use eframe::egui::{self, Color32, RichText, Stroke};
use pc::{PcState, Scan};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::{Arc, Mutex};
use std::time::Duration;

// ------------------------------------------------------------------ tema

const BG: Color32 = Color32::from_rgb(0x14, 0x17, 0x1c);
const BAR: Color32 = Color32::from_rgb(0x0e, 0x10, 0x14);
const PANEL: Color32 = Color32::from_rgb(0x18, 0x1c, 0x22);
const ROW: Color32 = Color32::from_rgb(0x1f, 0x24, 0x2c);
const INPUT: Color32 = Color32::from_rgb(0x0f, 0x12, 0x16);
const BORDER: Color32 = Color32::from_rgb(0x26, 0x2b, 0x33);
const BORDER2: Color32 = Color32::from_rgb(0x33, 0x3a, 0x44);
const TEXT: Color32 = Color32::from_rgb(0xe6, 0xe9, 0xee);
const SOFT: Color32 = Color32::from_rgb(0xc9, 0xce, 0xd6);
const MUTED: Color32 = Color32::from_rgb(0xa9, 0xb0, 0xbb);
const DIM: Color32 = Color32::from_rgb(0x8b, 0x93, 0xa0);
const ACCENT: Color32 = Color32::from_rgb(0x7c, 0xc4, 0xff);
const PILL: Color32 = Color32::from_rgb(0x1f, 0x3a, 0x52);
const GREEN: Color32 = Color32::from_rgb(0x4a, 0xde, 0x80);
const AMBER: Color32 = Color32::from_rgb(0xf5, 0xa5, 0x24);
const RED: Color32 = Color32::from_rgb(0xf8, 0x71, 0x71);
const PINK: Color32 = Color32::from_rgb(0xf4, 0x72, 0xb6);
const PURPLE: Color32 = Color32::from_rgb(0xc0, 0x84, 0xfc);

const SERIES: [Color32; 6] = [ACCENT, AMBER, PURPLE, GREEN, PINK, Color32::from_rgb(0x2d, 0xd4, 0xbf)];

fn level_color(l: Level) -> Color32 {
    match l {
        Level::Waiting => DIM,
        Level::Good => GREEN,
        Level::Slow => AMBER,
        Level::Bad => RED,
    }
}

/// (fundo, borda) dos cartões de veredito.
fn level_fill(l: Level) -> (Color32, Color32) {
    match l {
        Level::Waiting => (PANEL, BORDER),
        Level::Good => (Color32::from_rgb(0x17, 0x32, 0x1f), Color32::from_rgb(0x2c, 0x5a, 0x3a)),
        Level::Slow => (Color32::from_rgb(0x2a, 0x24, 0x14), Color32::from_rgb(0x5a, 0x4a, 0x1c)),
        Level::Bad => (Color32::from_rgb(0x3a, 0x1d, 0x1d), Color32::from_rgb(0x7f, 0x2d, 0x2d)),
    }
}

/// Borda dos cartões de endereço.
fn level_border(l: Level) -> Color32 {
    match l {
        Level::Slow => Color32::from_rgb(0x5a, 0x4a, 0x1c),
        Level::Bad => Color32::from_rgb(0x7f, 0x2d, 0x2d),
        _ => BORDER,
    }
}

fn level_from_str(s: &str) -> Level {
    match s {
        "good" => Level::Good,
        "slow" => Level::Slow,
        "bad" => Level::Bad,
        _ => Level::Waiting,
    }
}

fn event_color(kind: &str) -> Color32 {
    match kind {
        "queda" => RED,
        "voltou" => GREEN,
        "pico" => AMBER,
        "perda" => PINK,
        _ => PURPLE,
    }
}

fn apply_theme(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    v.panel_fill = BG;
    v.window_fill = PANEL;
    v.extreme_bg_color = INPUT;
    v.override_text_color = Some(TEXT);
    v.selection.bg_fill = ACCENT.gamma_multiply(0.45);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    for w in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
        w.corner_radius = 10.0.into();
    }
    v.widgets.inactive.weak_bg_fill = ROW;
    v.widgets.inactive.bg_fill = ROW;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER2);
    ctx.set_theme(egui::Theme::Dark);
    ctx.set_visuals_of(egui::Theme::Dark, v);
    ctx.style_mut_of(egui::Theme::Dark, |s| {
        // Alvos de toque maiores.
        s.spacing.item_spacing = egui::vec2(10.0, 10.0);
        s.spacing.button_padding = egui::vec2(14.0, 10.0);
        s.spacing.interact_size = egui::vec2(44.0, 36.0);
        s.spacing.icon_width = 22.0;
        s.spacing.icon_width_inner = 12.0;
        s.text_styles.insert(egui::TextStyle::Body, egui::FontId::proportional(15.0));
        s.text_styles.insert(egui::TextStyle::Button, egui::FontId::proportional(15.0));
    });
}

// ------------------------------------------------------------------ estado

#[derive(Clone, Copy, PartialEq)]
enum Screen {
    Home,
    Phone,
    Pc,
    Settings,
}

/// Opções só do celular, em `celular.json` (a lista de endereços e o
/// intervalo ficam no `config.json`, como no desktop).
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct PhoneSettings {
    /// PC da casa escolhido, "ip:porta".
    pc: Option<String>,
}

impl PhoneSettings {
    fn path() -> std::path::PathBuf {
        app_dir().join("celular.json")
    }

    fn load() -> Self {
        std::fs::read_to_string(Self::path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
    }

    fn save(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(app_dir())?;
        std::fs::write(Self::path(), serde_json::to_string_pretty(self).expect("serializa"))
    }
}

/// Um endereço medido pelo celular, copiado para desenhar sem segurar o lock.
struct Row {
    name: String,
    local: bool,
    level: Level,
    label: &'static str,
    recent: Stats,
    /// Média das 5 últimas respostas.
    current: Option<f64>,
    latency: Vec<[f64; 2]>,
    in_outage: bool,
    config_error: bool,
}

/// Um aviso da tela Início (do celular ou do PC).
struct Notice {
    t: f64,
    color: Color32,
    text: String,
}

pub struct App {
    cfg: Config,
    draft: Config,
    monitor: Monitor,
    phone: PhoneSettings,
    pc: Arc<Mutex<PcState>>,
    scan: Arc<Mutex<Scan>>,
    screen: Screen,
    adding: bool,
    new_name: String,
    new_host: String,
    confirm_remove: Option<usize>,
    manual: String,
    message: Option<(String, bool)>,
    updater: Arc<crate::update::Updater>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        apply_theme(&cc.egui_ctx);
        let cfg = Config::load();
        let monitor = Monitor::new(&cfg);
        let phone = PhoneSettings::load();
        let pc = Arc::new(Mutex::new(PcState::default()));
        pc.lock().unwrap().set_addr(phone.pc.clone());
        pc::spawn(pc.clone(), cc.egui_ctx.clone());
        let repaint = cc.egui_ctx.clone();
        // No Android só avisa; o APK novo é baixado pelo navegador.
        let updater = crate::update::spawn(false, move || repaint.request_repaint());
        Self {
            draft: cfg.clone(),
            cfg,
            monitor,
            manual: phone.pc.clone().unwrap_or_default(),
            phone,
            pc,
            scan: Arc::new(Mutex::new(Scan::default())),
            screen: Screen::Home,
            adding: false,
            new_name: String::new(),
            new_host: String::new(),
            confirm_remove: None,
            message: None,
            updater,
        }
    }

    fn rows(&self) -> Vec<Row> {
        let now = now_secs();
        let sh = self.monitor.shared.lock().unwrap();
        let spike = sh.spike_ms;
        sh.targets
            .iter()
            .map(|t| {
                let recent = compute_stats(t.samples.iter().filter(|s| s.t >= now - 60.0), spike);
                let (level, label) = assess::target_status(&recent, t.error.is_some(), t.outage_since.is_some());
                let replies: Vec<f64> = t.samples.iter().rev().filter_map(|s| s.rtt).take(5).collect();
                let current = (!replies.is_empty()).then(|| replies.iter().sum::<f64>() / replies.len() as f64);
                let latency = t.samples.iter().filter(|s| s.t >= now - 300.0).filter_map(|s| s.rtt.map(|r| [s.t - now, r])).collect();
                Row {
                    name: t.name.clone(),
                    local: t.local,
                    level,
                    label,
                    current,
                    latency,
                    in_outage: t.outage_since.is_some(),
                    config_error: t.error.is_some() && !recent.has_replies(),
                    recent,
                }
            })
            .collect()
    }

    fn save_targets(&mut self) {
        self.draft.targets = self.cfg.targets.clone();
        self.monitor.apply(&self.cfg);
        if let Err(e) = self.cfg.save() {
            self.message = Some((format!("Não consegui salvar: {e}"), false));
        }
    }

    fn set_pc(&mut self, addr: Option<String>) {
        self.phone.pc = addr.clone();
        self.manual = addr.clone().unwrap_or_default();
        self.pc.lock().unwrap().set_addr(addr);
        self.message = Some(match self.phone.save() {
            Ok(()) => ("PC salvo.".into(), true),
            Err(e) => (format!("Não consegui salvar: {e}"), false),
        });
    }
}

fn phone_overall(rows: &[Row]) -> (Level, &'static str, String) {
    let views: Vec<assess::View> = rows
        .iter()
        .map(|r| assess::View {
            name: &r.name,
            local: r.local,
            level: r.level,
            has_replies: r.recent.has_replies(),
            in_outage: r.in_outage,
            config_error: r.config_error,
        })
        .collect();
    assess::overall(&views)
}

fn rank(l: Level) -> u8 {
    match l {
        Level::Waiting => 0,
        Level::Good => 1,
        Level::Slow => 2,
        Level::Bad => 3,
    }
}

fn hhmm(t: f64) -> String {
    Local.timestamp_opt(t as i64, 0).single().map(|d| d.format("%H:%M").to_string()).unwrap_or_default()
}

fn ms_text(v: f64) -> String {
    format!("{v:.0}")
}

// ------------------------------------------------------------------ quadro

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.ctx().request_repaint_after(Duration::from_millis(500));
        let ctx = ui.ctx().clone();

        // Botão "voltar": das telas volta ao Início; no Início fecha o app.
        if ctx.input(|i| i.key_pressed(egui::Key::BrowserBack)) {
            if self.adding {
                self.adding = false;
            } else if self.screen == Screen::Home {
                crate::android::finish();
            } else {
                self.screen = Screen::Home;
            }
        }

        // Barras do sistema (status e navegação) ficam por cima da janela.
        let (top, bottom) = crate::android::insets(&ctx);
        if top > 0.0 {
            egui::Panel::top("barra_status").exact_size(top).show_separator_line(false).frame(egui::Frame::new().fill(BG)).show(ui, |_| {});
        }
        if bottom > 0.0 {
            egui::Panel::bottom("barra_sistema").exact_size(bottom).show_separator_line(false).frame(egui::Frame::new().fill(BAR)).show(ui, |_| {});
        }
        self.nav_bar(ui);

        let title = match self.screen {
            Screen::Home => "Monitor de Rede",
            Screen::Phone => "Este celular",
            Screen::Pc => "PC da casa",
            Screen::Settings => "Ajustes",
        };
        let frame = egui::Frame::new().fill(BG).inner_margin(egui::Margin { left: 20, right: 20, top: 0, bottom: 0 });
        egui::Panel::top("titulo").exact_size(56.0).show_separator_line(false).frame(frame).show(ui, |ui| {
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.label(RichText::new(title).size(20.0).strong());
            });
        });

        let frame = egui::Frame::new().fill(BG).inner_margin(egui::Margin { left: 16, right: 16, top: 4, bottom: 0 });
        egui::CentralPanel::default_margins().frame(frame).show(ui, |ui| {
            egui::ScrollArea::vertical().id_salt(self.screen as u8).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = 12.0;
                match self.screen {
                    Screen::Home => self.home(ui),
                    Screen::Phone => self.phone_screen(ui),
                    Screen::Pc => self.pc_screen(ui),
                    Screen::Settings => self.settings(ui),
                }
                ui.add_space(16.0);
            });
        });
    }
}

impl App {
    fn nav_bar(&mut self, ui: &mut egui::Ui) {
        let frame = egui::Frame::new().fill(BAR);
        egui::Panel::bottom("navegacao").exact_size(72.0).show_separator_line(false).frame(frame).show(ui, |ui| {
            let rect = ui.max_rect();
            ui.painter().hline(rect.x_range(), rect.top(), Stroke::new(1.0, BORDER));
            let w = rect.width() / 4.0;
            for (i, (screen, label)) in [(Screen::Home, "Início"), (Screen::Phone, "Este celular"), (Screen::Pc, "PC da casa"), (Screen::Settings, "Ajustes")]
                .into_iter()
                .enumerate()
            {
                let cell = egui::Rect::from_min_size(egui::pos2(rect.left() + w * i as f32, rect.top()), egui::vec2(w, rect.height()));
                let resp = ui.interact(cell, egui::Id::unique(("aba", i)), egui::Sense::click());
                if resp.clicked() {
                    self.screen = screen;
                    self.adding = false;
                    self.confirm_remove = None;
                }
                let on = self.screen == screen;
                let p = ui.painter();
                let pill = egui::Rect::from_center_size(egui::pos2(cell.center().x, cell.top() + 24.0), egui::vec2(56.0, 28.0));
                if on {
                    p.rect_filled(pill, 14.0, PILL);
                }
                let c = if on { Color32::WHITE } else { DIM };
                nav_icon(p, screen, pill.center(), c);
                p.text(egui::pos2(cell.center().x, cell.top() + 52.0), egui::Align2::CENTER_CENTER, label, egui::FontId::proportional(12.0), c);
            }
        });
    }

    // ------------------------------------------------------------- Início

    fn home(&mut self, ui: &mut egui::Ui) {
        let rows = self.rows();
        let (p_level, p_title, p_msg) = phone_overall(&rows);
        let (pc_addr, pc_online, pc_data) = {
            let s = self.pc.lock().unwrap();
            (s.addr.clone(), s.online(), s.data.clone())
        };
        let pc_view = pc_data.as_ref().filter(|_| pc_online);
        let pc_level = pc_view.map(|v| level_from_str(v["overall"]["status"].as_str().unwrap_or("")));

        // Veredito da casa: o pior entre o celular e o PC.
        let (level, title, msg) = match pc_level {
            Some(Level::Good) if p_level == Level::Good => (Level::Good, "Internet de casa boa".to_string(), "Celular e PC da casa sem problemas.".to_string()),
            Some(l) if rank(l) > rank(p_level) => {
                let v = pc_view.unwrap();
                (l, format!("PC da casa: {}", v["overall"]["title"].as_str().unwrap_or("")), v["overall"]["message"].as_str().unwrap_or("").to_string())
            }
            _ => (p_level, p_title.to_string(), p_msg),
        };
        let (fill, stroke) = level_fill(level);
        egui::Frame::new().fill(fill).stroke(Stroke::new(1.0, stroke)).corner_radius(20.0).inner_margin(20).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.vertical_centered(|ui| {
                ui.spacing_mut().item_spacing.y = 8.0;
                status_icon(ui, level);
                ui.label(RichText::new(title).size(22.0).strong());
                ui.label(RichText::new(msg).size(14.0).color(MUTED));
            });
        });

        // Dois cartões lado a lado.
        let gap = 12.0;
        let w = (ui.available_width() - gap) / 2.0;
        let worst_phone = rows.iter().filter(|r| r.recent.has_replies()).max_by(|a, b| a.recent.avg.total_cmp(&b.recent.avg));
        let loss = rows.iter().map(|r| r.recent.loss_pct()).fold(0.0, f64::max);
        let phone_card = (
            "Este celular".to_string(),
            worst_phone.map(|r| format!("{} ms", ms_text(r.recent.avg))).unwrap_or_else(|| "—".into()),
            format!("perda {}", pct(loss)),
            p_level,
        );
        let pc_card = match (&pc_addr, pc_view) {
            (None, _) => ("PC da casa".to_string(), "—".to_string(), "não conectado".to_string(), Level::Waiting),
            (Some(_), None) => ("PC da casa".to_string(), "—".to_string(), "sem resposta".to_string(), Level::Waiting),
            (Some(_), Some(v)) => {
                let targets = v["targets"].as_array().cloned().unwrap_or_default();
                let worst = targets.iter().filter(|t| t["last_1min"]["avg_ms"].is_number()).max_by(|a, b| {
                    a["last_1min"]["avg_ms"].as_f64().unwrap_or(0.0).total_cmp(&b["last_1min"]["avg_ms"].as_f64().unwrap_or(0.0))
                });
                let value = worst.and_then(|t| t["last_1min"]["avg_ms"].as_f64()).map(|v| format!("{} ms", ms_text(v))).unwrap_or_else(|| "—".into());
                let bad = targets.iter().max_by_key(|t| rank(level_from_str(t["status"].as_str().unwrap_or(""))));
                let text = match bad {
                    Some(t) if rank(level_from_str(t["status"].as_str().unwrap_or(""))) >= 2 => {
                        format!("{}: {}", t["name"].as_str().unwrap_or(""), t["status_label"].as_str().unwrap_or("").to_lowercase())
                    }
                    _ => "tudo normal".into(),
                };
                ("PC da casa".to_string(), value, text, pc_level.unwrap_or(Level::Waiting))
            }
        };
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for ((label, value, sub, lv), target) in [(phone_card, Screen::Phone), (pc_card, Screen::Pc)] {
                let resp = egui::Frame::new()
                    .fill(PANEL)
                    .stroke(Stroke::new(1.0, level_border(lv)))
                    .corner_radius(16.0)
                    .inner_margin(14)
                    .show(ui, |ui| {
                        ui.set_width(w - 30.0);
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 2.0;
                            ui.label(RichText::new(label).size(12.0).color(DIM));
                            ui.label(RichText::new(value).size(24.0).strong().monospace().color(level_color(lv)));
                            ui.add(egui::Label::new(RichText::new(sub).size(12.0).color(DIM)).truncate());
                        });
                    })
                    .response;
                if ui.interact(resp.rect, resp.id.with("toque"), egui::Sense::click()).clicked() {
                    self.screen = target;
                }
            }
        });

        // Últimos avisos, do celular e do PC.
        let mut notices: Vec<Notice> = self
            .monitor
            .shared
            .lock()
            .unwrap()
            .events
            .iter()
            .take(20)
            .map(|e| Notice {
                t: e.t,
                color: event_color(e.kind.db_name()),
                text: notice_text("Este celular", &e.name, e.kind.label(), &e.detail, e.kind == EventKind::OutageEnd),
            })
            .collect();
        if let Some(v) = pc_data.as_ref() {
            for e in v["recent_events"].as_array().into_iter().flatten() {
                let Some(t) = e["time"].as_str().and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok()) else { continue };
                let kind = e["type"].as_str().unwrap_or("");
                notices.push(Notice {
                    t: t.timestamp() as f64,
                    color: event_color(kind),
                    text: notice_text("PC da casa", e["target"].as_str().unwrap_or(""), e["label"].as_str().unwrap_or(""), e["detail"].as_str().unwrap_or(""), kind == "voltou"),
                });
            }
        }
        notices.sort_by(|a, b| b.t.total_cmp(&a.t));
        ui.label(RichText::new("Últimos avisos").size(13.0).color(DIM));
        card(ui, BORDER, |ui| {
            ui.spacing_mut().item_spacing.y = 8.0;
            if notices.is_empty() {
                ui.label(RichText::new("Nenhum aviso nas últimas 24 h. Isso é bom!").size(13.0).color(DIM));
            }
            for n in notices.iter().take(6) {
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    ui.add_space(0.0);
                    dot(ui, n.color, 9.0, 8.0);
                    ui.add(egui::Label::new(RichText::new(format!("{} {}", hhmm(n.t), n.text)).size(13.0)).wrap());
                });
            }
        });
        if let crate::update::State::Available(r) = self.updater.state() {
            card(ui, ACCENT, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new(format!("Versão {} disponível", r.version)).size(15.0).strong());
                if ui.add(primary_button("Baixar atualização")).clicked() {
                    crate::android::open_url(r.asset.as_deref().unwrap_or(&r.page));
                }
            });
        }
        ui.vertical_centered(|ui| {
            ui.label(RichText::new("Mede só com o app aberto, sem rodar em segundo plano.").size(12.0).color(DIM));
        });
    }

    // ------------------------------------------------------- Este celular

    fn phone_screen(&mut self, ui: &mut egui::Ui) {
        let rows = self.rows();
        card(ui, BORDER, |ui| {
            ui.label(RichText::new("Latência — últimos 5 min").size(12.0).color(DIM));
            latency_chart(ui, &rows);
        });

        let mut remove = None;
        for (i, r) in rows.iter().enumerate() {
            let c = level_color(r.level);
            card(ui, level_border(r.level), |ui| {
                ui.horizontal(|ui| {
                    dot(ui, c, 12.0, 0.0);
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        ui.add(egui::Label::new(RichText::new(&r.name).size(15.0).strong()).truncate());
                        ui.label(RichText::new(r.label).size(13.0).color(c));
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 3.0;
                        let x = egui::Button::new(RichText::new("×").size(18.0).color(DIM)).frame(false).min_size(egui::vec2(36.0, 36.0));
                        if ui.add(x).clicked() {
                            self.confirm_remove = Some(i);
                        }
                        ui.add_space(6.0);
                        match r.current {
                            Some(v) if r.level != Level::Bad || r.recent.has_replies() => {
                                ui.label(RichText::new(" ms").size(12.0).color(DIM));
                                ui.label(RichText::new(ms_text(v)).size(20.0).strong().monospace());
                            }
                            _ => {
                                ui.label(RichText::new("—").size(20.0).color(DIM));
                            }
                        }
                    });
                });
                if self.confirm_remove == Some(i) {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Remover este endereço?").size(13.0).color(MUTED));
                        if ui.add(egui::Button::new(RichText::new("Remover").color(RED))).clicked() {
                            remove = Some(i);
                        }
                        if ui.button("Cancelar").clicked() {
                            self.confirm_remove = None;
                        }
                    });
                }
            });
        }
        if rows.is_empty() {
            ui.label(RichText::new("Nenhum endereço. Adicione um abaixo.").color(DIM));
        }
        if let Some(i) = remove {
            // A tela mostra só os ativos; acha o mesmo na configuração pelo nome.
            let name = rows[i].name.clone();
            if let Some(pos) = self.cfg.targets.iter().position(|t| t.enabled && (t.name == name || t.host == name)) {
                self.cfg.targets.remove(pos);
                self.save_targets();
            }
            self.confirm_remove = None;
        }

        if self.adding {
            self.add_form(ui);
        } else {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                if ui.add(primary_button("+ Adicionar endereço")).clicked() {
                    self.adding = true;
                    self.new_name.clear();
                    self.new_host.clear();
                }
            });
        }
    }

    fn add_form(&mut self, ui: &mut egui::Ui) {
        card(ui, BORDER, |ui| {
            ui.label(RichText::new("Novo endereço").size(15.0).strong());
            let w = ui.available_width();
            ui.add(egui::TextEdit::singleline(&mut self.new_name).hint_text("Nome (ex.: Deco da sala)").desired_width(w).margin(egui::vec2(10.0, 10.0)));
            ui.add(
                egui::TextEdit::singleline(&mut self.new_host)
                    .hint_text("IP, domínio ou https://site")
                    .font(egui::TextStyle::Monospace)
                    .desired_width(w)
                    .margin(egui::vec2(10.0, 10.0)),
            );
            ui.horizontal(|ui| {
                let ok = !self.new_host.trim().is_empty();
                if ui.add_enabled(ok, primary_button("Salvar")).clicked() {
                    let host = self.new_host.trim().to_string();
                    let name = if self.new_name.trim().is_empty() { host.clone() } else { self.new_name.trim().to_string() };
                    self.cfg.targets.push(Target::new(&name, &host));
                    self.save_targets();
                    self.adding = false;
                }
                if ui.add(egui::Button::new("Cancelar").min_size(egui::vec2(0.0, 44.0))).clicked() {
                    self.adding = false;
                }
            });
            ui.label(RichText::new("Ou toque numa sugestão:").size(12.0).color(DIM));
            ui.horizontal_wrapped(|ui| {
                for (_, name, host) in SUGGESTIONS {
                    let exists = self.cfg.targets.iter().any(|t| t.name == *name);
                    let b = egui::Button::new(RichText::new(format!("+ {name}")).size(13.0)).corner_radius(16.0);
                    if ui.add_enabled(!exists, b).clicked() {
                        self.cfg.targets.push(Target::new(name, host));
                        self.save_targets();
                        self.adding = false;
                    }
                }
            });
        });
    }

    // ---------------------------------------------------------- PC da casa

    fn pc_screen(&mut self, ui: &mut egui::Ui) {
        let (addr, online, data, age) = {
            let s = self.pc.lock().unwrap();
            (s.addr.clone(), s.online(), s.data.clone(), s.last_ok.map(|t| t.elapsed().as_secs()))
        };
        let Some(addr) = addr else {
            card(ui, BORDER, |ui| {
                ui.vertical_centered(|ui| {
                    let (r, _) = ui.allocate_exact_size(egui::vec2(56.0, 44.0), egui::Sense::hover());
                    ui.painter().rect_filled(r, 12.0, PILL);
                    monitor_icon(ui.painter(), r.center(), TEXT);
                    ui.label(RichText::new("Nenhum PC conectado").size(17.0).strong());
                    ui.label(
                        RichText::new("No PC, abra o Monitor de Rede e ligue o \"Painel no celular\" em Configurações. Depois, aqui em Ajustes, toque em Procurar PCs.")
                            .size(13.0)
                            .color(MUTED),
                    );
                    ui.add_space(4.0);
                    if ui.add(primary_button("Ir para Ajustes")).clicked() {
                        self.screen = Screen::Settings;
                    }
                });
            });
            return;
        };

        // Cabeçalho: PC, endereço e se está respondendo.
        card(ui, BORDER, |ui| {
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(egui::vec2(40.0, 40.0), egui::Sense::hover());
                ui.painter().rect_filled(r, 10.0, PILL);
                monitor_icon(ui.painter(), r.center(), TEXT);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    // Nome do computador (versões do PC a partir da 0.5.0).
                    let name = data.as_ref().and_then(|d| d["computer"].as_str()).unwrap_or("PC da casa");
                    ui.label(RichText::new(name).strong());
                    let state = if online { "conectado" } else if data.is_none() && age.is_none() { "conectando…" } else { "sem resposta" };
                    ui.add(egui::Label::new(RichText::new(format!("{addr} · {state}")).size(12.0).monospace().color(DIM)).truncate());
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let (c, t) = if online { (GREEN, "online") } else { (RED, "offline") };
                    ui.label(RichText::new(t).size(12.0).color(c));
                    dot(ui, c, 8.0, 0.0);
                });
            });
        });

        let Some(v) = data else {
            card(ui, BORDER, |ui| {
                let text = if age.is_none() && !self.pc.lock().unwrap().offline {
                    "Conectando ao PC…".to_string()
                } else {
                    format!(
                        "Não consegui falar com o PC em {addr}. Confira se ele está ligado, com o Monitor de Rede aberto e o \"Painel no celular\" ativo, e se este celular está no Wi-Fi de casa."
                    )
                };
                ui.label(RichText::new(text).size(14.0).color(MUTED));
            });
            footer(ui, None);
            return;
        };
        if !online {
            ui.label(RichText::new("O PC parou de responder. Mostrando os últimos dados recebidos.").size(13.0).color(AMBER));
        }

        // Veredito do PC.
        let level = level_from_str(v["overall"]["status"].as_str().unwrap_or(""));
        let (fill, stroke) = level_fill(level);
        egui::Frame::new().fill(fill).stroke(Stroke::new(1.0, stroke)).corner_radius(16.0).inner_margin(egui::Margin::symmetric(16, 12)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 4.0;
            let c = if level == Level::Waiting { TEXT } else { level_color(level) };
            ui.label(RichText::new(v["overall"]["title"].as_str().unwrap_or("")).size(15.0).strong().color(c));
            ui.label(RichText::new(v["overall"]["message"].as_str().unwrap_or("")).size(13.0).color(SOFT));
        });

        for t in v["targets"].as_array().into_iter().flatten() {
            let level = level_from_str(t["status"].as_str().unwrap_or(""));
            let c = level_color(level);
            card(ui, level_border(level), |ui| {
                ui.horizontal(|ui| {
                    dot(ui, c, 12.0, 0.0);
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        ui.add(egui::Label::new(RichText::new(t["name"].as_str().unwrap_or("")).size(15.0).strong()).truncate());
                        ui.label(RichText::new(t["status_label"].as_str().unwrap_or("")).size(13.0).color(c));
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 3.0;
                        match t["last_1min"]["avg_ms"].as_f64() {
                            Some(ms) => {
                                ui.label(RichText::new(" ms").size(12.0).color(DIM));
                                ui.label(RichText::new(ms_text(ms)).size(20.0).strong().monospace());
                            }
                            None => {
                                ui.label(RichText::new("—").size(20.0).color(DIM));
                            }
                        }
                    });
                });
            });
        }

        // Uso da rede no PC.
        let u = &v["network_usage"];
        if u.is_object() {
            card(ui, BORDER, |ui| {
                ui.spacing_mut().item_spacing.y = 6.0;
                ui.label(RichText::new("Uso de rede no PC agora").size(12.0).color(DIM));
                let mbps = |x: &Value| x.as_f64().map(|v| format!("{v:.1}").replace('.', ",")).unwrap_or_else(|| "—".into());
                kv(ui, "Total (download / upload)", &format!("{} / {} Mbps", mbps(&u["download_mbps"]), mbps(&u["upload_mbps"])));
                for a in u["top_apps"].as_array().into_iter().flatten().take(5) {
                    let value = if a["download_mbps"].is_number() {
                        format!("{} / {} Mbps", mbps(&a["download_mbps"]), mbps(&a["upload_mbps"]))
                    } else {
                        format!("{} conexões", a["connections"].as_u64().unwrap_or(0))
                    };
                    kv(ui, a["app"].as_str().unwrap_or(""), &value);
                }
            });
        }
        footer(ui, age);
    }

    // -------------------------------------------------------------- Ajustes

    fn settings(&mut self, ui: &mut egui::Ui) {
        let port = self.cfg.web_port;
        let (pc_online, pc_addr) = {
            let s = self.pc.lock().unwrap();
            (s.online(), s.addr.clone())
        };
        let (scanning, scan_done, found, scan_error) = {
            let s = self.scan.lock().unwrap();
            (s.running, s.done, s.found.clone(), s.error.clone())
        };

        ui.label(RichText::new("Conectar ao PC (rede local)").size(13.0).color(DIM));
        let mut choose: Option<Option<String>> = None;
        card(ui, BORDER, |ui| {
            // PC salvo primeiro, depois os encontrados na busca.
            let mut list: Vec<(String, String)> = Vec::new();
            if let Some(a) = &pc_addr {
                list.push((a.clone(), String::new()));
            }
            for (a, title) in &found {
                if Some(a) != pc_addr.as_ref() {
                    list.push((a.clone(), title.clone()));
                }
            }
            if !list.is_empty() {
                ui.label(RichText::new(if found.is_empty() { "PC salvo:" } else { "PCs encontrados nesta rede:" }).size(13.0).color(SOFT));
            }
            for (a, title) in &list {
                egui::Frame::new().fill(ROW).corner_radius(12.0).inner_margin(egui::Margin::symmetric(12, 10)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 2.0;
                            let (ip, p) = a.rsplit_once(':').unwrap_or((a, ""));
                            ui.label(RichText::new(ip).size(14.0).strong().monospace());
                            let sub = if title.is_empty() { format!("porta {p}") } else { format!("porta {p} · {title}") };
                            ui.add(egui::Label::new(RichText::new(sub).size(12.0).color(DIM)).truncate());
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if Some(a) == pc_addr.as_ref() {
                                if pc_online {
                                    // Ícone desenhado: a fonte embutida no Android não tem "✓".
                                    let (r, _) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
                                    let s = Stroke::new(2.0, GREEN);
                                    ui.painter().line_segment([r.left_center(), r.center_bottom() - egui::vec2(1.0, 2.0)], s);
                                    ui.painter().line_segment([r.center_bottom() - egui::vec2(1.0, 2.0), r.right_top() + egui::vec2(0.0, 2.0)], s);
                                    ui.label(RichText::new("Conectado").size(13.0).color(GREEN));
                                } else {
                                    ui.label(RichText::new("Salvo · sem resposta").size(13.0).color(AMBER));
                                }
                            } else if ui.button("Conectar").clicked() {
                                choose = Some(Some(a.clone()));
                            }
                        });
                    });
                });
            }

            if scanning {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(RichText::new("Procurando PCs nesta rede…").size(13.0).color(MUTED));
                });
            } else {
                if scan_done && found.is_empty() {
                    let msg = scan_error.clone().unwrap_or_else(|| format!("Nenhum PC encontrado na porta {port}."));
                    ui.label(RichText::new(msg).size(13.0).color(AMBER));
                }
                if ui.add(egui::Button::new("Procurar PCs").min_size(egui::vec2(ui.available_width(), 44.0))).clicked() {
                    pc::start_scan(self.scan.clone(), port, ui.ctx().clone());
                }
            }

            ui.horizontal(|ui| {
                let w = ui.available_width() - 90.0;
                ui.add(
                    egui::TextEdit::singleline(&mut self.manual)
                        .hint_text("IP manual: 192.168.68.__")
                        .font(egui::TextStyle::Monospace)
                        .desired_width(w)
                        .margin(egui::vec2(10.0, 10.0)),
                );
                if ui.add(egui::Button::new("Salvar").min_size(egui::vec2(0.0, 40.0))).clicked() {
                    match pc::parse_addr(&self.manual, port) {
                        Some(a) => choose = Some(Some(a)),
                        None => self.message = Some(("Endereço inválido. Use IP ou IP:porta.".into(), false)),
                    }
                }
            });
            if pc_addr.is_some() && ui.add(egui::Button::new(RichText::new("Esquecer este PC").size(13.0).color(MUTED)).frame(false)).clicked() {
                choose = Some(None);
            }
            ui.label(
                RichText::new(format!("No PC: Configurações → \"Painel no celular\" (porta {port}). A comunicação fica só na rede de casa e o painel é só leitura."))
                    .size(12.0)
                    .color(DIM),
            );
        });
        if let Some(c) = choose {
            self.set_pc(c);
        }
        if let Some((m, ok)) = &self.message {
            ui.label(RichText::new(m).size(13.0).color(if *ok { GREEN } else { RED }));
        }

        ui.label(RichText::new("Medição neste celular").size(13.0).color(DIM));
        card(ui, BORDER, |ui| {
            let d = &mut self.draft;
            setting_row(ui, "Intervalo por endereço (ms)", |ui| {
                ui.add(egui::DragValue::new(&mut d.interval_ms).range(200..=60_000).speed(50));
            });
            setting_row(ui, "Tempo máximo de espera (ms)", |ui| {
                ui.add(egui::DragValue::new(&mut d.timeout_ms).range(100..=10_000).speed(50));
            });
            setting_row(ui, "Pico acima de (ms)", |ui| {
                ui.add(egui::DragValue::new(&mut d.spike_ms).range(10..=5_000).speed(5));
            });
            setting_row(ui, "Porta do painel no PC", |ui| {
                ui.add(egui::DragValue::new(&mut d.web_port).range(1..=65_535));
            });
            ui.checkbox(&mut d.history_enabled, "Guardar histórico no celular");
            let dirty = self.draft != self.cfg;
            ui.horizontal(|ui| {
                if ui.add_enabled(dirty, primary_button("Salvar")).clicked() {
                    self.cfg = self.draft.clone();
                    self.monitor.apply(&self.cfg);
                    self.message = Some(match self.cfg.save() {
                        Ok(()) => ("Configuração salva.".into(), true),
                        Err(e) => (format!("Não consegui salvar: {e}"), false),
                    });
                }
                if ui.add_enabled(dirty, egui::Button::new("Descartar").min_size(egui::vec2(0.0, 44.0))).clicked() {
                    self.draft = self.cfg.clone();
                }
            });
            if ui.add(egui::Button::new(RichText::new("Restaurar endereços padrão").size(13.0).color(MUTED)).frame(false)).clicked() {
                self.cfg.targets = Config::default().targets;
                self.save_targets();
            }
        });
        ui.label(RichText::new("Atualizações").size(13.0).color(DIM));
        card(ui, BORDER, |ui| {
            ui.set_width(ui.available_width());
            let state = self.updater.state();
            ui.label(RichText::new(state.describe()).size(14.0).color(if matches!(state, crate::update::State::Failed(_)) { RED } else { MUTED }));
            match &state {
                crate::update::State::Available(r) => {
                    if ui.add(primary_button("Baixar atualização")).clicked() {
                        crate::android::open_url(r.asset.as_deref().unwrap_or(&r.page));
                    }
                    ui.label(
                        RichText::new("Abre o download do APK novo. Depois, toque no arquivo baixado e em Atualizar.")
                            .size(12.0)
                            .color(DIM),
                    );
                }
                crate::update::State::Checking => {}
                _ => {
                    if ui.add(egui::Button::new("Procurar agora").min_size(egui::vec2(0.0, 44.0))).clicked() {
                        self.updater.check_now();
                    }
                }
            }
        });
        ui.label(
            RichText::new("O app mede só enquanto está aberto na tela. Ande pela casa com ele aberto para ver onde o Wi-Fi fica lento.")
                .size(12.0)
                .color(DIM),
        );
    }
}

// ------------------------------------------------------------ utilitários

fn notice_text(source: &str, name: &str, label: &str, detail: &str, back: bool) -> String {
    let what = if back { "voltou".to_string() } else { label.to_lowercase() };
    if detail.is_empty() || back {
        format!("{source} · {name}: {what}")
    } else {
        format!("{source} · {name}: {what} ({detail})")
    }
}

fn pct(v: f64) -> String {
    let s = if v == 0.0 { "0".to_string() } else { format!("{v:.1}").replace('.', ",") };
    format!("{s}%")
}

fn card<R>(ui: &mut egui::Ui, stroke: Color32, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::new()
        .fill(PANEL)
        .stroke(Stroke::new(1.0, stroke))
        .corner_radius(16.0)
        .inner_margin(egui::Margin::symmetric(16, 14))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

fn primary_button(text: &str) -> egui::Button<'_> {
    egui::Button::new(RichText::new(text).strong().color(BAR))
        .fill(ACCENT)
        .stroke(Stroke::NONE)
        .corner_radius(16.0)
        .min_size(egui::vec2(0.0, 48.0))
}

fn setting_row(ui: &mut egui::Ui, label: &str, add: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).size(14.0));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), add);
    });
}

fn kv(ui: &mut egui::Ui, k: &str, v: &str) {
    ui.horizontal(|ui| {
        ui.add(egui::Label::new(RichText::new(k).size(13.0)).truncate());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new(v).size(13.0).monospace());
        });
    });
}

fn footer(ui: &mut egui::Ui, age: Option<u64>) {
    let text = match age {
        Some(s) => format!("Atualizado há {s} s · só funciona na mesma rede (Wi-Fi de casa)"),
        None => "Só funciona na mesma rede (Wi-Fi de casa)".into(),
    };
    ui.vertical_centered(|ui| ui.label(RichText::new(text).size(12.0).color(DIM)));
}

/// Bolinha colorida; `top` desloca para alinhar com a primeira linha de texto.
fn dot(ui: &mut egui::Ui, c: Color32, size: f32, top: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size + top), egui::Sense::hover());
    ui.painter().circle_filled(egui::pos2(rect.center().x, rect.top() + top + size / 2.0), size / 2.0, c);
}

fn status_icon(ui: &mut egui::Ui, level: Level) {
    let c = level_color(level);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(72.0, 72.0), egui::Sense::hover());
    let p = ui.painter();
    let center = rect.center();
    p.circle_stroke(center, 34.5, Stroke::new(3.0, c));
    let s = Stroke::new(4.0, c);
    let at = |x: f32, y: f32| center + egui::vec2(x, y);
    match level {
        Level::Good => {
            p.line_segment([at(-14.0, 1.0), at(-4.0, 11.0)], s);
            p.line_segment([at(-4.0, 11.0), at(15.0, -9.0)], s);
        }
        Level::Slow => {
            p.line_segment([at(0.0, -15.0), at(0.0, 4.0)], s);
            p.circle_filled(at(0.0, 14.0), 2.8, c);
        }
        Level::Bad => {
            p.line_segment([at(-11.0, -11.0), at(11.0, 11.0)], s);
            p.line_segment([at(11.0, -11.0), at(-11.0, 11.0)], s);
        }
        Level::Waiting => {
            for i in 0..3 {
                p.circle_filled(at(-12.0 + i as f32 * 12.0, 0.0), 3.5, c);
            }
        }
    }
}

fn monitor_icon(p: &egui::Painter, c: egui::Pos2, color: Color32) {
    let s = Stroke::new(1.8, color);
    let screen = egui::Rect::from_center_size(c + egui::vec2(0.0, -2.0), egui::vec2(20.0, 13.0));
    p.rect_stroke(screen, 2.0, s, egui::StrokeKind::Middle);
    p.line_segment([c + egui::vec2(0.0, 4.5), c + egui::vec2(0.0, 8.0)], s);
    p.line_segment([c + egui::vec2(-5.0, 8.5), c + egui::vec2(5.0, 8.5)], s);
}

/// Ícones simples da barra de navegação, desenhados (sem depender de fontes).
fn nav_icon(p: &egui::Painter, screen: Screen, c: egui::Pos2, color: Color32) {
    let s = Stroke::new(1.8, color);
    let v = |x: f32, y: f32| c + egui::vec2(x, y);
    match screen {
        Screen::Home => {
            p.line_segment([v(-8.0, -1.0), v(0.0, -8.0)], s);
            p.line_segment([v(0.0, -8.0), v(8.0, -1.0)], s);
            p.rect_stroke(egui::Rect::from_min_max(v(-6.0, -2.0), v(6.0, 8.0)), 1.0, s, egui::StrokeKind::Middle);
        }
        Screen::Phone => {
            p.rect_stroke(egui::Rect::from_center_size(c, egui::vec2(11.0, 18.0)), 2.5, s, egui::StrokeKind::Middle);
            p.circle_filled(v(0.0, 6.0), 1.2, color);
        }
        Screen::Pc => monitor_icon(p, c, color),
        Screen::Settings => {
            for (y, x) in [(-5.0, 3.0), (0.0, -3.0), (5.0, 1.0)] {
                p.line_segment([v(-9.0, y), v(9.0, y)], s);
                p.circle(v(x, y), 2.4, BAR, s);
            }
        }
    }
}

fn latency_chart(ui: &mut egui::Ui, rows: &[Row]) {
    use egui_plot::{HLine, Line, LineStyle, Plot, PlotPoints};
    Plot::new("latencia_celular")
        .height(110.0)
        .show_axes([false, true])
        .show_grid(false)
        .show_background(false)
        .allow_drag(false)
        .allow_zoom(false)
        .allow_scroll(false)
        .allow_boxed_zoom(false)
        .allow_double_click_reset(false)
        .include_x(-300.0)
        .include_x(0.0)
        .include_y(0.0)
        .include_y(50.0)
        .y_axis_formatter(|m, _| format!("{:.0}", m.value))
        .show(ui, |pu| {
            pu.hline(HLine::new("80 ms", GOOD_MS).color(BORDER2).style(LineStyle::dashed_dense()));
            for (i, r) in rows.iter().enumerate() {
                pu.line(Line::new(r.name.clone(), PlotPoints::from(r.latency.clone())).color(SERIES[i % SERIES.len()]).width(2.0));
            }
        });
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        for (i, r) in rows.iter().enumerate() {
            let c = SERIES[i % SERIES.len()];
            let (rect, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
            ui.painter().hline(rect.x_range(), rect.center().y, Stroke::new(2.5, c));
            ui.label(RichText::new(&r.name).size(11.0).color(c));
            ui.add_space(6.0);
        }
    });
}
