#![cfg_attr(windows, windows_subsystem = "windows")]

// O código do app fica em lib.rs, compartilhado com a versão Android.
#[cfg(not(target_os = "android"))]
fn main() -> eframe::Result {
    monitor_rede::desktop_main()
}

// No Android o app é a biblioteca (libmonitor_rede.so), aberta pela NativeActivity.
#[cfg(target_os = "android")]
fn main() {}
