//! Aba Monitor: diagnóstico, gráfico (latência / jitter / perda) e tabela.

use super::*;
use crate::monitor::fmt_time;
use egui_plot::{Bar, BarChart, HLine, Legend, Line, LineStyle, MarkerShape, Plot, PlotPoints, Points};

/// Diagnóstico técnico para a faixa do topo: (nível, título, detalhe).
fn diagnosis(rows: &[Row], window: &str) -> (Level, String, String) {
    // Endereço que nunca resolveu é erro de configuração, não problema de rede.
    let measured: Vec<&Row> = rows.iter().filter(|r| r.win.sent >= 5 && !(r.error.is_some() && !r.win.has_replies())).collect();
    if measured.is_empty() {
        return (Level::Waiting, "Coletando dados…".into(), String::new());
    }
    let bad = |r: &&Row| r.in_outage || r.win.loss_pct() >= 2.0;
    let local: Vec<&Row> = measured.iter().copied().filter(|r| r.local).collect();
    let net: Vec<&Row> = measured.iter().copied().filter(|r| !r.local).collect();

    if !net.is_empty() && net.iter().all(|r| r.in_outage) {
        return if local.iter().any(bad) {
            (Level::Bad, "Sem conexão".into(), "A rede local também está falhando (cabo, Deco ou roteador).".into())
        } else {
            (Level::Bad, "Sem internet".into(), "A rede local responde; o problema está na saída para a internet.".into())
        };
    }
    if let Some(r) = local.iter().find(|r| bad(r)) {
        return (Level::Bad, "Problema na rede local".into(), format!("{} com {} de perda. Verifique cabo, Deco ou roteador.", r.name, pct(r.win.loss_pct())));
    }
    let net_loss = net.iter().map(|r| r.win.loss_pct()).fold(0.0, f64::max);
    if net_loss >= 2.0 {
        return (Level::Bad, "Instabilidade na internet".into(), format!("Até {} de perda; a rede local está ok (provável operadora).", pct(net_loss)));
    }
    let local_avg = local.iter().map(|r| r.win.avg).fold(0.0, f64::max);
    if local_avg > 30.0 {
        return (Level::Slow, "Rede local lenta".into(), format!("Média de até {local_avg:.0} ms; no cabo costuma ficar abaixo de 5 ms."));
    }
    let net_avg = net.iter().map(|r| r.win.avg).fold(0.0, f64::max);
    let net_jit = net.iter().map(|r| r.win.jitter).fold(0.0, f64::max);
    if net_avg > GOOD_MS || net_jit > 30.0 {
        return (Level::Slow, "Latência alta ou variável".into(), format!("Média até {net_avg:.0} ms e jitter até {net_jit:.0} ms. Pode atrapalhar vídeo e jogos."));
    }
    if measured.iter().any(|r| r.win.lost > 0) {
        return (Level::Slow, "Estável, com perdas ocasionais".into(), format!("Nos últimos {window}."));
    }
    (Level::Good, "Tudo estável".into(), format!("Rede local e internet sem perdas nos últimos {window}"))
}

impl App {
    pub(super) fn ui_monitor(&mut self, ui: &mut egui::Ui) {
        let (rows, started) = self.snapshot();
        let windows = [("1 min", 60.0), ("5 min", 300.0), ("15 min", 900.0), ("1 h", 3600.0)];
        let win_label = windows.iter().find(|w| w.1 == self.window_secs).map(|w| w.0).unwrap_or("5 min");

        // Faixa de diagnóstico.
        let (level, title, detail) = diagnosis(&rows, win_label);
        let c = level.color();
        egui::Frame::new()
            .fill(c.gamma_multiply(0.14))
            .stroke(egui::Stroke::new(1.0, c.gamma_multiply(0.6)))
            .corner_radius(8.0)
            .inner_margin(egui::Margin::symmetric(16, 12))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    dot(ui, c, 12.0);
                    ui.label(RichText::new(title).size(15.0).strong());
                    ui.label(RichText::new(detail).size(13.0).color(MUTED));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(RichText::new(format!("Monitorando desde {}", &fmt_time(started)[..11])).size(12.0).color(MUTED));
                    });
                });
            });

        ui.add_space(6.0);
        ui.horizontal(|ui| {
            egui::Frame::new().fill(INPUT).stroke(egui::Stroke::new(1.0, BORDER)).corner_radius(8.0).inner_margin(3).show(ui, |ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                for (ch, label) in [(Chart::Latency, "Latência (ms)"), (Chart::Jitter, "Jitter (ms)"), (Chart::Loss, "Perda de pacotes (%)")] {
                    let on = self.chart == ch;
                    let b = egui::Button::new(RichText::new(label).size(13.0).color(if on { Color32::WHITE } else { MUTED }))
                        .fill(if on { BORDER } else { Color32::TRANSPARENT })
                        .stroke(egui::Stroke::NONE);
                    if ui.add(b).clicked() {
                        self.chart = ch;
                    }
                }
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                for (label, secs) in windows.iter().rev() {
                    let on = self.window_secs == *secs;
                    let b = egui::Button::new(RichText::new(*label).size(12.0).color(if on { TOP } else { MUTED }))
                        .fill(if on { ACCENT } else { Color32::TRANSPARENT })
                        .stroke(if on { egui::Stroke::NONE } else { egui::Stroke::new(1.0, BORDER2) });
                    if ui.add(b).clicked() {
                        self.window_secs = *secs;
                    }
                }
            });
        });

        self.plot(ui, &rows);
        ui.add_space(8.0);
        table(ui, &rows);
    }

    fn plot(&self, ui: &mut egui::Ui, rows: &[Row]) {
        let w = self.window_secs;
        let chart = self.chart;
        let unit = if chart == Chart::Loss { "%" } else { " ms" };
        let plot = Plot::new(("grafico", chart as u8))
            .legend(Legend::default().position(egui_plot::Corner::LeftTop))
            .height(240.0)
            .include_x(-w)
            .include_x(0.0)
            .include_y(0.0)
            .include_y(match chart {
                Chart::Latency => 50.0,
                Chart::Jitter => 20.0,
                Chart::Loss => 10.0,
            })
            .allow_scroll(false)
            .show_background(true)
            .grid_color(Color32::from_rgb(0x2a, 0x30, 0x39))
            .x_axis_formatter(|m, _| fmt_ago(-m.value))
            .y_axis_formatter(move |m, _| format!("{:.0}{unit}", m.value))
            .label_formatter(move |h| match h {
                egui_plot::HoverPosition::NearDataPoint { plot_name, position, .. } => {
                    Some(format!("{plot_name}\n{:.1}{unit}\n{}", position.y, fmt_ago(-position.x)).replace('.', ","))
                }
                _ => None,
            });

        egui::Frame::new().fill(INPUT).stroke(egui::Stroke::new(1.0, BORDER)).corner_radius(8.0).inner_margin(6).show(ui, |ui| {
            plot.show(ui, |pu| match chart {
                Chart::Latency => {
                    for (i, r) in rows.iter().enumerate() {
                        let c = SERIES[i % SERIES.len()];
                        pu.line(Line::new(r.name.clone(), PlotPoints::from(r.latency.clone())).color(c).width(1.6));
                    }
                    let losses: Vec<[f64; 2]> = rows.iter().filter(|r| r.error.is_none()).flat_map(|r| r.losses.iter().copied()).collect();
                    if !losses.is_empty() {
                        pu.points(Points::new("perda", PlotPoints::from(losses)).color(RED).shape(MarkerShape::Circle).radius(4.0).filled(true));
                    }
                }
                Chart::Jitter => {
                    for (i, r) in rows.iter().enumerate() {
                        let c = SERIES[i % SERIES.len()];
                        pu.line(Line::new(r.name.clone(), PlotPoints::from(r.jitter.clone())).color(c).width(1.6));
                    }
                }
                Chart::Loss => {
                    let bucket = (w / 10.0).max(6.0);
                    let n = rows.len().max(1) as f64;
                    let bw = bucket * 0.8 / n;
                    for (i, r) in rows.iter().enumerate() {
                        let c = SERIES[i % SERIES.len()];
                        let bars = r
                            .loss_buckets
                            .iter()
                            .map(|(x, p)| Bar::new(x + bucket * 0.1 + bw * (i as f64 + 0.5), *p).width(bw).fill(c))
                            .collect();
                        pu.bar_chart(BarChart::new(r.name.clone(), bars).color(c));
                    }
                    pu.hline(HLine::new("5% (vermelho)", BAD_LOSS_PCT).color(RED).style(LineStyle::dashed_dense()));
                }
            });
        });
    }
}

fn table(ui: &mut egui::Ui, rows: &[Row]) {
    egui::ScrollArea::vertical().show(ui, |ui| {
        egui::Grid::new("tabela").striped(true).spacing([18.0, 8.0]).min_col_width(40.0).show(ui, |ui| {
            for h in ["ALVO", "HOST", "ÚLTIMO", "MÉDIA", "MÍN / MÁX", "JITTER", "PERDA"] {
                ui.label(RichText::new(h).size(11.0).color(DIM));
            }
            ui.end_row();
            for (i, r) in rows.iter().enumerate() {
                let (level, _) = r.status();
                ui.horizontal(|ui| {
                    dot(ui, level.color(), 10.0);
                    ui.label(RichText::new(&r.name).color(SERIES[i % SERIES.len()]));
                    if r.local {
                        ui.label(RichText::new("LOCAL").size(10.0).color(DIM));
                    }
                    if r.method != "ping" {
                        ui.label(RichText::new(r.method).size(10.0).color(DIM));
                    }
                });
                let host = match (&r.ip, &r.error) {
                    (_, Some(_)) => RichText::new(format!("{} (não encontrado)", short(&r.host))).color(RED),
                    (Some(ip), None) if r.host == "gateway" => RichText::new(format!("{ip} (automático)")).color(MUTED),
                    _ => RichText::new(short(&r.host)).color(MUTED),
                };
                let resp = ui.label(host.monospace());
                if let Some(e) = &r.error {
                    resp.on_hover_text(e);
                }
                let s = &r.win;
                let mono = |t: String| RichText::new(t).monospace();
                match s.last {
                    Some(v) => ui.label(mono(ms(v))),
                    None if s.sent > 0 => ui.label(mono("perdido".into()).color(RED)),
                    None => ui.label(mono("—".into())),
                };
                let has = s.has_replies();
                ui.label(mono(if has { ms(s.avg) } else { "—".into() }));
                ui.label(mono(if has { format!("{:.0} / {:.0}", s.min, s.max) } else { "—".into() }));
                ui.label(mono(if has { format!("{:.0}", s.jitter) } else { "—".into() }));
                let lp = s.loss_pct();
                let lc = if lp > BAD_LOSS_PCT { RED } else if lp > 0.0 { AMBER } else { GREEN };
                ui.label(mono(pct(lp)).color(lc)).on_hover_text(format!("{} de {} perdidos", s.lost, s.sent));
                ui.end_row();
            }
        });
    });
}

/// Encurta endereços longos para caber na tabela.
fn short(host: &str) -> String {
    let h = host.trim_start_matches("https://").trim_start_matches("http://");
    if h.chars().count() > 38 {
        format!("{}…", h.chars().take(37).collect::<String>())
    } else {
        h.to_string()
    }
}
