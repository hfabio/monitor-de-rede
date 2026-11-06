//! Tela Início: status em linguagem simples e um cartão por endereço.

use super::*;

/// Resumo geral em linguagem comum: (nível, título, frase).
pub fn overall(rows: &[Row]) -> (Level, &'static str, String) {
    let views: Vec<crate::assess::View> = rows
        .iter()
        .map(|r| crate::assess::View {
            name: &r.name,
            local: r.local,
            level: r.status().0,
            has_replies: r.recent.has_replies(),
            in_outage: r.in_outage,
            config_error: r.error.is_some() && !r.recent.has_replies(),
        })
        .collect();
    crate::assess::overall(&views)
}

impl App {
    pub(super) fn ui_home(&mut self, ui: &mut egui::Ui) {
        let (rows, _) = self.snapshot();
        let (level, title, subtitle) = overall(&rows);

        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.vertical_centered(|ui| {
                let free = ui.available_height();
                let content = 260.0 + rows.len().div_ceil(2) as f32 * 82.0;
                ui.add_space(((free - content) / 2.0).clamp(8.0, 80.0));

                status_icon(ui, level);
                ui.add_space(14.0);
                ui.label(RichText::new(title).size(34.0).strong());
                ui.label(RichText::new(subtitle).size(17.0).color(MUTED));
                ui.add_space(18.0);

                let width = ui.available_width().min(760.0);
                let gap = 14.0;
                let card_w = (width - gap) / 2.0;
                for pair in rows.chunks(2) {
                    ui.allocate_ui_with_layout(egui::vec2(width, 72.0), egui::Layout::left_to_right(egui::Align::Min), |ui| {
                        ui.spacing_mut().item_spacing.x = gap;
                        for r in pair {
                            target_card(ui, r, card_w);
                        }
                    });
                    ui.add_space(4.0);
                }

                ui.add_space(10.0);
                ui.label(
                    RichText::new("Verde: até 80 ms · Amarelo: 80 ms a 1 s · Vermelho: acima de 1 s, sem resposta ou mais de 5% de perda")
                        .size(13.0)
                        .color(DIM),
                );
                ui.add_space(10.0);
                let btn = egui::Button::new(RichText::new("Ver detalhes técnicos").color(ACCENT).size(14.0))
                    .fill(Color32::TRANSPARENT)
                    .stroke(egui::Stroke::new(1.0, BORDER2))
                    .min_size(egui::vec2(0.0, 44.0));
                if ui.add(btn).clicked() {
                    self.view = View::Tab(Tab::Monitor);
                }
            });
        });
    }
}

fn status_icon(ui: &mut egui::Ui, level: Level) {
    let c = level.color();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(104.0, 104.0), egui::Sense::hover());
    let p = ui.painter();
    let center = rect.center();
    p.circle(center, 50.0, c.gamma_multiply(0.18), egui::Stroke::new(3.0, c));
    let s = egui::Stroke::new(5.0, c);
    let at = |x: f32, y: f32| center + egui::vec2(x, y);
    match level {
        Level::Good => {
            p.line_segment([at(-20.0, 2.0), at(-6.0, 16.0)], s);
            p.line_segment([at(-6.0, 16.0), at(22.0, -14.0)], s);
        }
        Level::Slow => {
            p.line_segment([at(0.0, -22.0), at(0.0, 6.0)], s);
            p.circle_filled(at(0.0, 20.0), 3.5, c);
        }
        Level::Bad => {
            p.line_segment([at(-16.0, -16.0), at(16.0, 16.0)], s);
            p.line_segment([at(16.0, -16.0), at(-16.0, 16.0)], s);
        }
        Level::Waiting => {
            for i in 0..3 {
                p.circle_filled(at(-16.0 + i as f32 * 16.0, 0.0), 4.0, c);
            }
        }
    }
}

fn target_card(ui: &mut egui::Ui, r: &Row, width: f32) {
    let (level, label) = r.status();
    let c = level.color();
    let stroke = match level {
        Level::Slow => Color32::from_rgb(0x5a, 0x4a, 0x1c),
        Level::Bad => Color32::from_rgb(0x7f, 0x2d, 0x2d),
        _ => BORDER,
    };
    egui::Frame::new()
        .fill(PANEL)
        .stroke(egui::Stroke::new(1.0, stroke))
        .corner_radius(12.0)
        .inner_margin(16)
        .show(ui, |ui| {
            ui.set_width(width - 34.0);
            ui.horizontal(|ui| {
                dot(ui, c, 14.0);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    ui.label(RichText::new(&r.name).size(15.0).strong());
                    ui.label(RichText::new(label).size(13.0).color(c));
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 3.0;
                    match r.current {
                        Some(v) if level != Level::Bad || r.recent.has_replies() => {
                            ui.label(RichText::new(" ms").size(12.0).color(DIM));
                            ui.label(RichText::new(format!("{v:.0}")).size(20.0).strong().monospace());
                        }
                        _ => {
                            ui.label(RichText::new("—").size(20.0).color(DIM));
                        }
                    };
                });
            });
        });
}
