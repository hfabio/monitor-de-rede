#![cfg_attr(windows, windows_subsystem = "windows")]

mod assess;
mod autostart;
mod config;
mod history;
mod mcp;
mod monitor;
mod netusage;
mod probe;
mod status;
mod tray;
mod ui;

fn main() -> eframe::Result {
    // `monitor-rede mcp`: servidor MCP por stdio, sem janela.
    if std::env::args().nth(1).as_deref() == Some("mcp") {
        mcp::serve();
        return Ok(());
    }
    // Ao iniciar com o sistema, abre direto na bandeja (sem mostrar a janela).
    let start_hidden = tray::SUPPORTED && std::env::args().any(|a| a == tray::START_HIDDEN_ARG);
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_visible(!start_hidden)
            .with_title("Monitor de Rede")
            .with_inner_size([1100.0, 720.0])
            .with_min_inner_size([820.0, 560.0]),
        ..Default::default()
    };
    eframe::run_native("Monitor de Rede", options, Box::new(|cc| Ok(Box::new(ui::App::new(cc)))))
}
