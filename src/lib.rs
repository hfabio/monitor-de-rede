//! Monitor de Rede. O executável de desktop (`main.rs`) usa a interface de
//! `ui/`; o APK Android carrega esta biblioteca e usa a interface de
//! `mobile/` (entrada em `android.rs`). Medição, histórico e avaliação são
//! os mesmos nos dois.

#[cfg(target_os = "android")]
mod android;
mod assess;
#[cfg(not(target_os = "android"))]
mod autostart;
mod config;
// No Android, as consultas usadas só pelo MCP ficam sem uso.
#[cfg_attr(target_os = "android", allow(dead_code))]
mod history;
#[cfg(not(target_os = "android"))]
mod mcp;
#[cfg(target_os = "android")]
mod mobile;
// O celular não usa a aba Eventos nem a tabela do desktop.
#[cfg_attr(target_os = "android", allow(dead_code))]
mod monitor;
#[cfg(not(target_os = "android"))]
mod netusage;
mod probe;
#[cfg(not(target_os = "android"))]
mod status;
#[cfg(not(target_os = "android"))]
mod tray;
#[cfg(not(target_os = "android"))]
mod ui;
#[cfg(not(target_os = "android"))]
mod web;

/// Ponto de entrada do executável de desktop.
#[cfg(not(target_os = "android"))]
pub fn desktop_main() -> eframe::Result {
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
