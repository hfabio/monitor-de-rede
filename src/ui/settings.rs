//! Aba Configurações: endereços, sugestões e parâmetros do teste.

use super::*;
use crate::config::{Target, AWS_SP, SUGGESTIONS};

impl App {
    pub(super) fn ui_settings(&mut self, ui: &mut egui::Ui) {
        let right_w = 300.0;
        let left_w = (ui.available_width() - right_w - 20.0).max(380.0);
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 20.0;
            let h = ui.available_height();
            ui.allocate_ui_with_layout(egui::vec2(left_w, h), egui::Layout::top_down(egui::Align::Min), |ui| {
                egui::ScrollArea::vertical().id_salt("enderecos").show(ui, |ui| {
                    ui.set_width(left_w);
                    self.targets_editor(ui, left_w);
                });
            });
            ui.allocate_ui_with_layout(egui::vec2(right_w, h), egui::Layout::top_down(egui::Align::Min), |ui| self.test_panel(ui, right_w));
        });
    }

    fn targets_editor(&mut self, ui: &mut egui::Ui, width: f32) {
        ui.label(RichText::new("Endereços monitorados").size(14.0).strong());
        ui.add_space(2.0);
        let name_w = (width - 40.0 - 44.0 - 24.0) * 0.42;
        let host_w = (width - 40.0 - 44.0 - 24.0) * 0.58;

        let mut remove = None;
        egui::Grid::new("alvos").spacing([8.0, 8.0]).show(ui, |ui| {
            for h in ["ATIVO", "NOME", "IP OU DOMÍNIO", ""] {
                ui.label(RichText::new(h).size(11.0).color(DIM));
            }
            ui.end_row();
            for (i, t) in self.draft.targets.iter_mut().enumerate() {
                ui.checkbox(&mut t.enabled, "").on_hover_text("Ativo");
                ui.add_sized([name_w, 30.0], egui::TextEdit::singleline(&mut t.name).margin(egui::vec2(8.0, 6.0)));
                ui.add_sized([host_w, 30.0], egui::TextEdit::singleline(&mut t.host).font(egui::TextStyle::Monospace).margin(egui::vec2(8.0, 6.0)))
                    .on_hover_text("IP, domínio, \"gateway\" (roteador automático), https://endereço ou host:porta");
                let b = egui::Button::new(RichText::new("×").size(16.0).color(RED)).fill(Color32::TRANSPARENT).stroke(egui::Stroke::new(1.0, BORDER2));
                if ui.add(b).on_hover_text("Remover").clicked() {
                    remove = Some(i);
                }
                ui.end_row();
            }
        });
        if let Some(i) = remove {
            self.draft.targets.remove(i);
        }

        ui.add_space(6.0);
        ui.separator();
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut self.new_name).hint_text("Nome (ex.: Valorant SP)").desired_width(name_w + 30.0).margin(egui::vec2(8.0, 6.0)));
            ui.add(egui::TextEdit::singleline(&mut self.new_host).hint_text("IP ou domínio").desired_width(host_w - 60.0).margin(egui::vec2(8.0, 6.0)));
            let ok = !self.new_host.trim().is_empty();
            if ui.add_enabled(ok, primary_button("+ Adicionar")).clicked() {
                let host = self.new_host.trim().to_string();
                let name = if self.new_name.trim().is_empty() { host.clone() } else { self.new_name.trim().to_string() };
                self.draft.targets.push(Target::new(&name, &host));
                self.new_name.clear();
                self.new_host.clear();
            }
        });

        ui.add_space(8.0);
        ui.label(RichText::new("Sugestões — clique para adicionar").size(12.0).color(DIM));
        for group in ["Jogos", "Apps"] {
            ui.horizontal_wrapped(|ui| {
                ui.allocate_ui(egui::vec2(52.0, 20.0), |ui| ui.label(RichText::new(group.to_uppercase()).size(11.0).color(DIM)));
                if group == "Jogos" {
                    egui::Frame::new()
                        .fill(Color32::from_rgb(0x17, 0x32, 0x1f))
                        .stroke(egui::Stroke::new(1.0, Color32::from_rgb(0x2c, 0x6b, 0x3e)))
                        .corner_radius(10.0)
                        .inner_margin(egui::Margin::symmetric(8, 2))
                        .show(ui, |ui| ui.label(RichText::new("servidores BR").size(11.0).color(Color32::from_rgb(0xa9, 0xd9, 0xb5))));
                }
                for (_, name, host) in SUGGESTIONS.iter().filter(|s| s.0 == group) {
                    let exists = self.draft.targets.iter().any(|t| t.name == *name);
                    let b = egui::Button::new(RichText::new(format!("+ {name}")).size(12.0)).corner_radius(14.0);
                    if ui.add_enabled(!exists, b).on_hover_text(*host).clicked() {
                        self.draft.targets.push(Target::new(name, host));
                    }
                }
            });
            if group == "Jogos" {
                ui.horizontal_wrapped(|ui| {
                    ui.add_space(58.0);
                    ui.label(RichText::new("Jogos →").size(11.0).color(DIM));
                    ui.label(RichText::new(AWS_SP.trim_start_matches("https://")).size(11.0).monospace().color(MUTED));
                    ui.label(RichText::new("· medido por HTTPS (AWS São Paulo)").size(11.0).color(DIM));
                });
            }
        }
    }

    fn test_panel(&mut self, ui: &mut egui::Ui, width: f32) {
        card(ui, BORDER, |ui| {
            ui.set_width(width - 30.0);
            ui.label(RichText::new("Teste").size(14.0).strong());
            ui.add_space(4.0);
            let d = &mut self.draft;
            let row = |ui: &mut egui::Ui, label: &str, add: &mut dyn FnMut(&mut egui::Ui)| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(label).size(13.0));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| add(ui));
                });
            };
            row(ui, "Intervalo por alvo (ms)", &mut |ui| {
                ui.add(egui::DragValue::new(&mut d.interval_ms).range(200..=60_000).speed(50));
            });
            row(ui, "Timeout (ms)", &mut |ui| {
                ui.add(egui::DragValue::new(&mut d.timeout_ms).range(100..=10_000).speed(50));
            });
            row(ui, "Pico acima de (ms)", &mut |ui| {
                ui.add(egui::DragValue::new(&mut d.spike_ms).range(10..=5_000).speed(5));
            });
            row(ui, "Perda acima de (% em 1 min)", &mut |ui| {
                ui.add(egui::DragValue::new(&mut d.loss_alert_pct).range(0.5..=100.0).speed(0.5).fixed_decimals(1));
            });
            ui.checkbox(&mut d.history_enabled, "Guardar histórico (SQLite)");
            ui.add_enabled_ui(d.history_enabled, |ui| {
                row(ui, "Manter histórico por (dias)", &mut |ui| {
                    ui.add(egui::DragValue::new(&mut d.retention_days).range(1..=3650));
                });
            });
            let mut on = self.autostart;
            if ui.checkbox(&mut on, crate::autostart::label()).changed() {
                match crate::autostart::set(on) {
                    Ok(()) => self.autostart = on,
                    Err(e) => self.message = Some((format!("Não consegui alterar a inicialização: {e}"), false)),
                }
            }

            ui.add_space(6.0);
            let dirty = self.draft != self.cfg;
            ui.horizontal(|ui| {
                let save = primary_button("Salvar e aplicar").min_size(egui::vec2(width - 150.0, 38.0));
                if ui.add_enabled(dirty, save).clicked() {
                    self.apply_settings();
                }
                if ui.add_enabled(dirty, egui::Button::new("Descartar").min_size(egui::vec2(0.0, 38.0))).clicked() {
                    self.draft = self.cfg.clone();
                }
            });
            if dirty {
                ui.label(RichText::new("Alterações não salvas").size(12.0).color(AMBER));
            }
            if let Some((m, ok)) = &self.message {
                ui.label(RichText::new(m).size(12.0).color(if *ok { GREEN } else { RED }));
            }
            ui.add_space(6.0);
            if ui.add(egui::Button::new(RichText::new("Abrir pasta de dados").size(12.0).color(MUTED)).frame(false)).clicked() {
                open_folder(&crate::config::app_dir());
            }
            if ui.add(egui::Button::new(RichText::new("Restaurar endereços padrão").size(12.0).color(MUTED)).frame(false)).clicked() {
                self.draft.targets = crate::config::Config::default().targets;
            }
        });
    }
}
