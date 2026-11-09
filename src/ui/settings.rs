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
                    ui.add_space(14.0);
                    self.mcp_card(ui);
                    ui.add_space(14.0);
                    self.web_card(ui);
                });
            });
            ui.allocate_ui_with_layout(egui::vec2(right_w, h), egui::Layout::top_down(egui::Align::Min), |ui| {
                egui::ScrollArea::vertical().id_salt("teste").show(ui, |ui| self.test_panel(ui, right_w));
            });
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
            ui.checkbox(&mut d.auto_update, "Atualizar automaticamente");
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
        ui.add_space(14.0);
        self.update_card(ui, width);
    }

    fn update_card(&mut self, ui: &mut egui::Ui, width: f32) {
        use crate::update::State;
        card(ui, BORDER, |ui| {
            ui.set_width(width - 30.0);
            ui.label(RichText::new("Atualizações").size(14.0).strong());
            let state = self.updater.state();
            let color = match state {
                State::Available(_) | State::Installing(_) => ACCENT,
                State::Failed(_) => RED,
                _ => MUTED,
            };
            ui.label(RichText::new(state.describe()).size(12.0).color(color));
            ui.horizontal(|ui| {
                match &state {
                    State::Available(r) => {
                        if ui.add(primary_button("Atualizar agora")).clicked() {
                            self.start_update(r.clone());
                        }
                        if ui.button("Novidades").clicked() {
                            ui.ctx().open_url(egui::OpenUrl::new_tab(&r.page));
                        }
                    }
                    State::Checking | State::Installing(_) => {}
                    _ => {
                        if ui.button("Procurar agora").clicked() {
                            self.updater.check_now();
                        }
                    }
                }
            });
            ui.label(
                RichText::new("Procura versões novas no GitHub ao abrir e a cada 6 horas. Com \"Atualizar automaticamente\", instala sozinho e reabre o app.")
                    .size(11.0)
                    .color(DIM),
            );
        });
    }

    fn mcp_card(&mut self, ui: &mut egui::Ui) {
        use crate::mcp::setup;
        card(ui, BORDER, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new("Integração com IA (MCP)").size(14.0).strong());
            ui.label(
                RichText::new("Deixa o Claude, Codex, Antigravity, Cursor e outros assistentes consultarem o status, os eventos e o histórico da rede, e testarem endereços.")
                    .size(12.0)
                    .color(MUTED),
            );
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.mcp_show_lan, false, "Neste computador");
                ui.selectable_value(&mut self.mcp_show_lan, true, "De outro computador da rede");
            });
            ui.add_space(2.0);

            let items: Vec<(&str, &str, String)> = if !self.mcp_show_lan {
                vec![
                    ("Claude Code", "rode no terminal", setup::claude_code()),
                    ("Claude Desktop · Antigravity · Cursor", "cole em mcpServers do arquivo de configuração", setup::json()),
                    ("Codex", "cole em ~/.codex/config.toml", setup::codex()),
                ]
            } else {
                ui.horizontal(|ui| {
                    ui.add_enabled(self.draft.web_enabled, egui::Checkbox::new(&mut self.draft.mcp_lan, "Permitir acesso pela rede de casa"));
                    if ui.button(RichText::new("Gerar novo token").size(12.0)).on_hover_text("O token antigo para de funcionar").clicked() {
                        self.draft.mcp_token = crate::web::new_token();
                    }
                });
                if !self.draft.web_enabled {
                    ui.label(RichText::new("Usa o mesmo servidor do Painel no celular: ative o painel abaixo.").size(11.0).color(AMBER));
                }
                if self.draft.mcp_lan != self.cfg.mcp_lan || self.draft.mcp_token != self.cfg.mcp_token {
                    ui.label(RichText::new("Clique em Salvar e aplicar para valer.").size(11.0).color(AMBER));
                }
                match self.web.as_ref().filter(|w| w.mcp_token().is_some()) {
                    Some(w) => {
                        let host = crate::web::lan_ip().map(|ip| ip.to_string()).unwrap_or_else(|| "localhost".into());
                        let url = format!("http://{host}:{}/mcp", w.port());
                        let token = w.mcp_token().unwrap_or_default().to_string();
                        ui.label(
                            RichText::new(format!(
                                "Use no outro computador (mesma rede). Cadastre um servidor por PC: este aparece como \"{}\". O token protege o acesso; não compartilhe fora de casa.",
                                setup::lan_name()
                            ))
                            .size(11.0)
                            .color(DIM),
                        );
                        vec![
                            ("Claude Code", "rode no terminal do outro computador", setup::lan::claude_code(&url, &token)),
                            ("Cursor · Antigravity e outros", "cole em mcpServers", setup::lan::json(&url, &token)),
                            ("Codex", "cole em ~/.codex/config.toml", setup::lan::codex(&url, &token)),
                        ]
                    }
                    None => {
                        ui.label(RichText::new("Acesso pela rede desligado.").size(12.0).color(MUTED));
                        Vec::new()
                    }
                }
            };

            let mut copied = None;
            for (name, how, text) in items {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(name).size(13.0).strong());
                    ui.label(RichText::new(how).size(12.0).color(DIM));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(RichText::new("Copiar").size(12.0)).clicked() {
                            ui.ctx().copy_text(text.clone());
                            copied = Some(name);
                        }
                    });
                });
                egui::Frame::new().fill(INPUT).corner_radius(6.0).inner_margin(8).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new(&text).size(11.0).monospace().color(MUTED));
                });
            }
            if let Some(n) = copied {
                self.message = Some((format!("Configuração para {n} copiada."), true));
            }
            let note = if self.mcp_show_lan {
                "Funciona enquanto o Monitor estiver aberto neste PC (também na bandeja)."
            } else {
                "O status ao vivo exige o Monitor aberto; histórico, eventos e testes funcionam mesmo com ele fechado."
            };
            ui.label(RichText::new(note).size(11.0).color(DIM));
        });
    }

    fn web_card(&mut self, ui: &mut egui::Ui) {
        card(ui, BORDER, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new("Painel no celular").size(14.0).strong());
            ui.label(
                RichText::new("Veja o status da rede no navegador do celular (Android ou iPhone) conectado ao mesmo Wi-Fi. Só leitura: pelo celular não dá para mudar nada.")
                    .size(12.0)
                    .color(MUTED),
            );
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.draft.web_enabled, "Ativar painel");
                ui.add_space(12.0);
                ui.label(RichText::new("Porta").size(13.0));
                ui.add_enabled(self.draft.web_enabled, egui::DragValue::new(&mut self.draft.web_port).range(1024..=65535));
            });
            if self.draft.web_enabled != self.cfg.web_enabled || self.draft.web_port != self.cfg.web_port {
                ui.label(RichText::new("Clique em Salvar e aplicar para valer.").size(11.0).color(AMBER));
            }

            if let Some(err) = self.web_error.clone() {
                ui.label(RichText::new(err).size(12.0).color(RED));
                ui.label(RichText::new("Outro programa deve estar usando essa porta. Troque a porta e salve.").size(11.0).color(DIM));
                if ui.button(RichText::new("Tentar de novo").size(12.0)).clicked() {
                    self.restart_web();
                }
            } else if let Some(port) = self.web.as_ref().map(|w| w.port()) {
                let url = match crate::web::lan_ip() {
                    Some(ip) => format!("http://{ip}:{port}"),
                    None => format!("http://localhost:{port}"),
                };
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Abra no celular").size(13.0).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(RichText::new("Copiar").size(12.0)).clicked() {
                            ui.ctx().copy_text(url.clone());
                            self.message = Some(("Endereço do painel copiado.".into(), true));
                        }
                    });
                });
                egui::Frame::new().fill(INPUT).corner_radius(6.0).inner_margin(8).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new(&url).size(14.0).monospace().color(ACCENT));
                });
                if cfg!(windows) {
                    ui.label(
                        RichText::new("Na primeira vez, o Firewall do Windows pode pedir permissão: clique em \"Permitir acesso\" para redes privadas. Se não abrir no celular, confira se a rede do PC está como privada.")
                            .size(11.0)
                            .color(DIM),
                    );
                }
            } else {
                ui.label(RichText::new("Painel desligado.").size(12.0).color(DIM));
            }
        });
    }
}
