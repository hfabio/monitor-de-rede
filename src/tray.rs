//! Ícone na bandeja do sistema (Windows e macOS).
//!
//! Fechar a janela só a esconde: o monitoramento continua e o ícone muda de
//! cor conforme a rede (verde / amarelo / vermelho). Clique no ícone (ou
//! "Abrir") mostra a janela; "Encerrar app" no menu fecha de verdade.
//!
//! - Windows: o ícone vive numa thread própria com loop de mensagens, então
//!   continua respondendo e atualizando com a janela escondida.
//! - macOS: o sistema exige o ícone na thread principal; ele é atualizado
//!   pela própria interface.
//! - Linux: sem bandeja (exigiria GTK); fechar a janela encerra o app.

use crate::assess::Level;
use eframe::egui;

/// Argumento usado ao iniciar com o sistema: abre direto na bandeja.
pub const START_HIDDEN_ARG: &str = "--bandeja";

pub const SUPPORTED: bool = cfg!(any(windows, target_os = "macos"));

/// Mostra a janela de novo (pode ser chamado de qualquer thread).
pub fn wake(ctx: &egui::Context) {
    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    ctx.request_repaint();
}

/// Ícone 32x32: círculo na cor do status, com anel claro.
#[cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]
fn icon_rgba(level: Level) -> Vec<u8> {
    let (r, g, b) = match level {
        Level::Good => (0x4a, 0xde, 0x80),
        Level::Slow => (0xf5, 0xa5, 0x24),
        Level::Bad => (0xf8, 0x71, 0x71),
        Level::Waiting => (0x8b, 0x93, 0xa0),
    };
    let size = 32usize;
    let mut px = vec![0u8; size * size * 4];
    let c = (size as f32 - 1.0) / 2.0;
    for y in 0..size {
        for x in 0..size {
            let d = ((x as f32 - c).powi(2) + (y as f32 - c).powi(2)).sqrt();
            let i = (y * size + x) * 4;
            // Anel externo claro + miolo colorido, com borda suavizada.
            let (pr, pg, pb, a) = if d <= 11.5 {
                (r, g, b, 1.0)
            } else if d <= 14.5 {
                (0x14, 0x17, 0x1c, 1.0)
            } else if d <= 15.5 {
                (0x14, 0x17, 0x1c, 15.5 - d)
            } else {
                (0, 0, 0, 0.0)
            };
            px[i..i + 4].copy_from_slice(&[pr, pg, pb, (a * 255.0) as u8]);
        }
    }
    px
}

#[cfg(any(windows, target_os = "macos"))]
fn make_icon(level: Level) -> Option<tray_icon::Icon> {
    tray_icon::Icon::from_rgba(icon_rgba(level), 32, 32).ok()
}

/// Resumo atual para o ícone e a dica: (nível, texto).
#[cfg_attr(not(windows), allow(dead_code))]
fn current(shared: &std::sync::Mutex<crate::monitor::Shared>, usage: &std::sync::Mutex<crate::netusage::Usage>) -> (Level, String) {
    let v = crate::status::build(shared, usage);
    let level = match v["overall"]["status"].as_str() {
        Some("good") => Level::Good,
        Some("slow") => Level::Slow,
        Some("bad") => Level::Bad,
        _ => Level::Waiting,
    };
    (level, format!("Monitor de Rede — {}", v["overall"]["title"].as_str().unwrap_or("")))
}

// -------------------------------------------------------------------- Windows

#[cfg(windows)]
mod imp {
    use super::*;
    use crate::monitor::Shared;
    use crate::netusage::Usage;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};
    use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
    use tray_icon::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, PeekMessageW, SetForegroundWindow, ShowWindow, TranslateMessage, MSG, PM_REMOVE, SW_SHOWNORMAL,
    };

    pub struct Tray;

    pub fn show_window(ctx: &egui::Context, hwnd: isize) {
        if hwnd != 0 {
            unsafe {
                ShowWindow(hwnd as _, SW_SHOWNORMAL);
                SetForegroundWindow(hwnd as _);
            }
        }
        super::wake(ctx);
    }

    pub fn install(cc: &eframe::CreationContext<'_>, shared: Arc<Mutex<Shared>>, usage: Arc<Mutex<Usage>>) -> Option<Tray> {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        let hwnd = match cc.window_handle().ok()?.as_raw() {
            RawWindowHandle::Win32(h) => h.hwnd.get(),
            _ => return None,
        };
        let ctx = cc.egui_ctx.clone();
        std::thread::spawn(move || run(hwnd, ctx, shared, usage));
        Some(Tray)
    }

    fn run(hwnd: isize, ctx: egui::Context, shared: Arc<Mutex<Shared>>, usage: Arc<Mutex<Usage>>) {
        let open = MenuItem::new("Abrir Monitor de Rede", true, None);
        let quit = MenuItem::new("Encerrar app", true, None);
        let menu = Menu::new();
        let _ = menu.append_items(&[&open, &PredefinedMenuItem::separator(), &quit]);
        let (mut level, tip) = current(&shared, &usage);
        let Ok(tray) = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .with_tooltip(&tip)
            .with_icon(make_icon(level).expect("ícone"))
            .build()
        else {
            return;
        };

        let mut last_update = Instant::now();
        loop {
            // O ícone do Windows precisa de um loop de mensagens nesta thread.
            unsafe {
                let mut msg: MSG = std::mem::zeroed();
                while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
            while let Ok(e) = TrayIconEvent::receiver().try_recv() {
                let left_click = matches!(e, TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. });
                if left_click || matches!(e, TrayIconEvent::DoubleClick { .. }) {
                    show_window(&ctx, hwnd);
                }
            }
            while let Ok(e) = MenuEvent::receiver().try_recv() {
                if e.id == *open.id() {
                    show_window(&ctx, hwnd);
                } else if e.id == *quit.id() {
                    std::process::exit(0);
                }
            }
            if last_update.elapsed() > Duration::from_secs(2) {
                last_update = Instant::now();
                let (lv, tip) = current(&shared, &usage);
                if lv != level {
                    level = lv;
                    let _ = tray.set_icon(make_icon(level));
                }
                let _ = tray.set_tooltip(Some(tip));
            }
            std::thread::sleep(Duration::from_millis(30));
        }
    }

    impl Tray {
        /// Atualizado pela própria thread do ícone.
        pub fn update(&mut self, _level: Level, _tip: &str) {}

    }
}

// ---------------------------------------------------------------------- macOS

#[cfg(target_os = "macos")]
mod imp {
    use super::*;
    use crate::monitor::Shared;
    use crate::netusage::Usage;
    use std::sync::{Arc, Mutex};
    use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
    use tray_icon::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

    pub struct Tray {
        icon: TrayIcon,
        level: Option<Level>,
        tip: String,
    }

    pub fn show_window(ctx: &egui::Context) {
        super::wake(ctx);
    }

    pub fn install(cc: &eframe::CreationContext<'_>, _shared: Arc<Mutex<Shared>>, _usage: Arc<Mutex<Usage>>) -> Option<Tray> {
        let open = MenuItem::new("Abrir Monitor de Rede", true, None);
        let quit = MenuItem::new("Encerrar app", true, None);
        let menu = Menu::new();
        menu.append_items(&[&open, &PredefinedMenuItem::separator(), &quit]).ok()?;
        let icon = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .with_tooltip("Monitor de Rede")
            .with_icon(make_icon(Level::Waiting)?)
            .build()
            .ok()?;

        let ctx = cc.egui_ctx.clone();
        TrayIconEvent::set_event_handler(Some(move |e| {
            if matches!(e, TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. }) {
                show_window(&ctx);
            }
        }));
        let ctx = cc.egui_ctx.clone();
        let (open_id, quit_id) = (open.id().clone(), quit.id().clone());
        MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
            if e.id == open_id {
                show_window(&ctx);
            } else if e.id == quit_id {
                std::process::exit(0);
            }
        }));
        Some(Tray { icon, level: None, tip: String::new() })
    }

    impl Tray {
        pub fn update(&mut self, level: Level, tip: &str) {
            if self.level != Some(level) {
                self.level = Some(level);
                let _ = self.icon.set_icon(make_icon(level));
            }
            if self.tip != tip {
                self.tip = tip.to_string();
                let _ = self.icon.set_tooltip(Some(tip));
            }
        }

    }
}

// ---------------------------------------------------------------------- Linux

#[cfg(not(any(windows, target_os = "macos")))]
mod imp {
    use super::*;
    use crate::monitor::Shared;
    use crate::netusage::Usage;
    use std::sync::{Arc, Mutex};

    pub struct Tray;

    pub fn install(_cc: &eframe::CreationContext<'_>, _shared: Arc<Mutex<Shared>>, _usage: Arc<Mutex<Usage>>) -> Option<Tray> {
        None
    }

    impl Tray {
        pub fn update(&mut self, _level: Level, _tip: &str) {}
    }
}

pub use imp::{install, Tray};
