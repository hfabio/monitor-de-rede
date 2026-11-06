#![cfg_attr(windows, windows_subsystem = "windows")]

mod autostart;
mod config;
mod history;
mod monitor;
mod netusage;
mod probe;
mod ui;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Monitor de Rede")
            .with_inner_size([1100.0, 720.0])
            .with_min_inner_size([820.0, 560.0]),
        ..Default::default()
    };
    eframe::run_native("Monitor de Rede", options, Box::new(|cc| Ok(Box::new(ui::App::new(cc)))))
}
