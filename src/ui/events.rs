//! Aba Eventos: filtros por tipo (últimas 24 h) e exportação para CSV.

use super::*;
use crate::monitor::{fmt_time, Event, EventKind};

const CATS: [(Category, &str, &str, Color32, Color32, Color32); 4] = [
    (Category::Queda, "queda", "quedas", Color32::from_rgb(0xfc, 0xa5, 0xa5), Color32::from_rgb(0x3a, 0x1d, 0x1d), Color32::from_rgb(0x7f, 0x2d, 0x2d)),
    (Category::Pico, "pico", "picos", Color32::from_rgb(0xfc, 0xd3, 0x4d), Color32::from_rgb(0x3a, 0x2e, 0x14), Color32::from_rgb(0x7a, 0x5a, 0x1c)),
    (Category::Perda, "perda de pacotes", "perdas de pacotes", Color32::from_rgb(0xf9, 0xa8, 0xd4), Color32::from_rgb(0x3a, 0x1a, 0x2e), Color32::from_rgb(0x7a, 0x2f, 0x5c)),
    (Category::Dns, "erro DNS", "erros DNS", Color32::from_rgb(0xd8, 0xb4, 0xfe), Color32::from_rgb(0x2a, 0x1f, 0x3a), Color32::from_rgb(0x5b, 0x3f, 0x80)),
];

fn kind_color(k: EventKind) -> Color32 {
    match k {
        EventKind::OutageStart => RED,
        EventKind::OutageEnd => GREEN,
        EventKind::Spike => AMBER,
        EventKind::Loss => PINK,
        EventKind::DnsError => PURPLE,
    }
}

impl App {
    pub(super) fn ui_events(&mut self, ui: &mut egui::Ui) {
        let events: Vec<Event> = self.monitor.shared.lock().unwrap().events.iter().cloned().collect();

        ui.horizontal(|ui| {
            ui.label(RichText::new("Mostrar:").size(12.0).color(DIM));
            for (cat, one, many, fg, bg, bd) in CATS {
                let n = events.iter().filter(|e| e.kind.category() == cat && e.kind != EventKind::OutageEnd).count();
                let on = self.filters.contains(&cat);
                let text = format!("{} {} {}", if on { "✓" } else { "○" }, n, if n == 1 { one } else { many });
                let b = egui::Button::new(RichText::new(text).size(13.0).color(if on { fg } else { DIM }))
                    .fill(if on { bg } else { Color32::TRANSPARENT })
                    .stroke(egui::Stroke::new(1.0, if on { bd } else { BORDER2 }))
                    .corner_radius(14.0);
                if ui.add(b).clicked() && !self.filters.remove(&cat) {
                    self.filters.insert(cat);
                }
            }
            egui::Frame::new().fill(Color32::from_rgb(0x1f, 0x24, 0x2c)).corner_radius(14.0).inner_margin(egui::Margin::symmetric(12, 6)).show(ui, |ui| {
                ui.label(RichText::new("últimas 24 h").size(13.0).color(MUTED));
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Exportar CSV").clicked() {
                    let shown: Vec<&Event> = events.iter().filter(|e| self.filters.contains(&e.kind.category())).collect();
                    self.message = Some(match export_csv(&shown) {
                        Ok(p) => {
                            if let Some(dir) = p.parent() {
                                open_folder(dir);
                            }
                            (format!("Exportado para {}", p.display()), true)
                        }
                        Err(e) => (format!("Não consegui exportar: {e}"), false),
                    });
                }
            });
        });
        if let Some((m, ok)) = &self.message {
            ui.label(RichText::new(m).size(12.0).color(if *ok { GREEN } else { RED }));
        }
        ui.add_space(4.0);

        let rows: Vec<&Event> = events.iter().filter(|e| self.filters.contains(&e.kind.category())).collect();
        if rows.is_empty() {
            ui.add_space(24.0);
            ui.vertical_centered(|ui| {
                let msg = if events.is_empty() { "Nenhum evento nas últimas 24 h. Isso é bom!" } else { "Nenhum evento com os filtros selecionados." };
                ui.label(RichText::new(msg).size(14.0).color(DIM));
            });
            return;
        }
        egui::ScrollArea::vertical().show(ui, |ui| {
            egui::Grid::new("eventos").striped(true).spacing([22.0, 8.0]).show(ui, |ui| {
                for h in ["HORÁRIO", "TIPO", "ALVO", "DETALHE"] {
                    ui.label(RichText::new(h).size(11.0).color(DIM));
                }
                ui.end_row();
                for e in rows.iter().take(2000) {
                    ui.label(RichText::new(fmt_time(e.t)).monospace());
                    ui.label(RichText::new(e.kind.label()).color(kind_color(e.kind)));
                    ui.label(&e.name).on_hover_text(&e.host);
                    ui.label(RichText::new(&e.detail).color(MUTED));
                    ui.end_row();
                }
            });
        });
    }
}

fn export_csv(events: &[&Event]) -> std::io::Result<std::path::PathBuf> {
    let dir = dirs::download_dir().or_else(dirs::document_dir).unwrap_or_else(crate::config::app_dir);
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("monitor-rede-eventos-{}.csv", chrono::Local::now().format("%Y%m%d-%H%M%S")));
    // ';' e BOM UTF-8 para abrir certo no Excel em português.
    let mut out = String::from("\u{feff}horario;tipo;alvo;host;detalhe\n");
    for e in events {
        let clean = |s: &str| s.replace([';', '\n'], " ");
        out.push_str(&format!("{};{};{};{};{}\n", fmt_time(e.t), e.kind.label(), clean(&e.name), clean(&e.host), clean(&e.detail)));
    }
    std::fs::write(&path, out)?;
    Ok(path)
}
