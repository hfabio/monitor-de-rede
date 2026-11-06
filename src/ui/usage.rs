//! Aba Uso da rede: banda total agora, gráfico de 5 min e uso por aplicativo.

use super::*;
use crate::netusage::PerApp;
use egui_plot::{Line, Plot, PlotPoints};

pub fn mbps(bps: f64) -> (String, &'static str) {
    if bps < 100_000.0 {
        (format!("{:.0}", bps / 1000.0), "kbps")
    } else {
        (format!("{:.1}", bps / 1_000_000.0).replace('.', ","), "Mbps")
    }
}

fn mbps_text(bps: f64) -> String {
    let (v, u) = mbps(bps);
    format!("{v} {u}")
}

impl App {
    pub(super) fn ui_usage(&mut self, ui: &mut egui::Ui) {
        let (down, up, hist, apps, mode) = {
            let u = self.usage.lock().unwrap();
            (u.down_bps, u.up_bps, u.history.iter().copied().collect::<Vec<_>>(), u.apps.clone(), u.per_app.clone())
        };

        let total_w = ui.available_width();
        let small = (total_w - 24.0) / 4.0;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            for (label, v) in [("Download agora", down), ("Upload agora", up)] {
                ui.allocate_ui_with_layout(egui::vec2(small, 80.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                    card(ui, BORDER, |ui| {
                        ui.set_width(small - 30.0);
                        ui.label(RichText::new(label).size(12.0).color(DIM));
                        let (n, u) = mbps(v);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(n).size(26.0).strong().monospace());
                            ui.label(RichText::new(u).size(14.0).color(DIM));
                        });
                    });
                });
            }
            ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), 80.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                card(ui, BORDER, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Últimos 5 min").size(12.0).color(DIM));
                        ui.label(RichText::new("━ download").size(11.0).color(ACCENT));
                        ui.label(RichText::new("━ upload").size(11.0).color(AMBER));
                    });
                    let now = crate::monitor::now_secs();
                    let d: Vec<[f64; 2]> = hist.iter().map(|h| [h.0 - now, h.1 / 1e6]).collect();
                    let u: Vec<[f64; 2]> = hist.iter().map(|h| [h.0 - now, h.2 / 1e6]).collect();
                    Plot::new("banda")
                        .height(44.0)
                        .show_axes(false)
                        .show_grid(false)
                        .show_background(false)
                        .allow_drag(false)
                        .allow_zoom(false)
                        .allow_scroll(false)
                        .include_x(-300.0)
                        .include_x(0.0)
                        .include_y(0.0)
                        .label_formatter(|h| match h {
                            egui_plot::HoverPosition::NearDataPoint { plot_name, position, .. } => {
                                Some(format!("{plot_name}: {:.1} Mbps", position.y).replace('.', ","))
                            }
                            _ => None,
                        })
                        .show(ui, |pu| {
                            pu.line(Line::new("download", PlotPoints::from(d)).color(ACCENT).width(1.6));
                            pu.line(Line::new("upload", PlotPoints::from(u)).color(AMBER).width(1.6));
                        });
                });
            });
        });

        ui.add_space(6.0);
        match mode {
            PerApp::Full => {}
            PerApp::ConnectionsOnly => {
                card(ui, Color32::from_rgb(0x5a, 0x4a, 0x1c), |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("Para ver quanto cada aplicativo baixa e envia, o Windows exige abrir o Monitor como administrador. Sem isso, mostramos só as conexões de cada um.")
                                .color(MUTED),
                        );
                        #[cfg(windows)]
                        if ui.button("Reabrir como administrador").clicked() && crate::netusage::relaunch_as_admin() {
                            std::process::exit(0);
                        }
                    });
                });
            }
            PerApp::Unsupported => {
                ui.label(RichText::new("O uso por aplicativo está disponível só no Windows. Acima, a banda total do computador.").color(MUTED));
                return;
            }
        }

        let full = mode == PerApp::Full;
        let max_total = apps.iter().map(|a| a.down_bps + a.up_bps).fold(0.0, f64::max).max(down + up).max(1.0);
        egui::ScrollArea::vertical().max_height(ui.available_height() - 30.0).show(ui, |ui| {
            egui::Grid::new("apps").striped(true).spacing([18.0, 8.0]).show(ui, |ui| {
                for h in ["APLICATIVO", "DOWNLOAD", "UPLOAD", "USO", "CONEXÕES"] {
                    ui.label(RichText::new(h).size(11.0).color(DIM));
                }
                ui.end_row();
                for a in apps.iter().take(30) {
                    ui.label(RichText::new(&a.name).strong());
                    let mono = |t: String| RichText::new(t).monospace();
                    ui.label(mono(if full { mbps_text(a.down_bps) } else { "—".into() }));
                    ui.label(mono(if full { mbps_text(a.up_bps) } else { "—".into() }));
                    let frac = if full { ((a.down_bps + a.up_bps) / max_total) as f32 } else { 0.0 };
                    bar(ui, frac, 240.0);
                    ui.label(mono(a.conns.to_string()));
                    ui.end_row();
                }
            });
        });
        ui.label(
            RichText::new("Abas do Chrome aparecem juntas numa linha só (separar por aba exigiria uma extensão do navegador). Tráfego UDP, usado em parte das chamadas e jogos, entra só no total.")
                .size(12.0)
                .color(DIM),
        );
    }
}

fn bar(ui: &mut egui::Ui, frac: f32, width: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 8.0), egui::Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 4.0, BORDER);
    if frac > 0.0 {
        let mut fill = rect;
        fill.set_width((width * frac.clamp(0.0, 1.0)).max(3.0));
        p.rect_filled(fill, 4.0, ACCENT);
    }
}
