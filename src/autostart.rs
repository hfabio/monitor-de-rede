//! "Iniciar com o sistema" em Windows (registro), macOS (LaunchAgent) e Linux (.desktop).

pub fn label() -> &'static str {
    if cfg!(windows) {
        "Iniciar com o Windows"
    } else if cfg!(target_os = "macos") {
        "Abrir ao iniciar sessão no Mac"
    } else {
        "Iniciar ao entrar na sessão"
    }
}

fn exe() -> Result<String, String> {
    std::env::current_exe().map(|p| p.display().to_string()).map_err(|e| e.to_string())
}

#[cfg(windows)]
mod imp {
    const KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
    const NAME: &str = "MonitorRede";

    fn reg(args: &[&str]) -> std::io::Result<std::process::Output> {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        std::process::Command::new("reg").args(args).creation_flags(CREATE_NO_WINDOW).output()
    }

    pub fn is_enabled() -> bool {
        reg(&["query", KEY, "/v", NAME]).map(|o| o.status.success()).unwrap_or(false)
    }

    pub fn set(on: bool) -> Result<(), String> {
        let out = if on {
            let val = format!("\"{}\"", super::exe()?);
            reg(&["add", KEY, "/v", NAME, "/t", "REG_SZ", "/d", &val, "/f"])
        } else {
            reg(&["delete", KEY, "/v", NAME, "/f"])
        }
        .map_err(|e| e.to_string())?;
        if out.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
        }
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use std::path::PathBuf;

    fn path() -> Option<PathBuf> {
        dirs::home_dir().map(|h| h.join("Library/LaunchAgents/com.hfabio.monitor-rede.plist"))
    }

    pub fn is_enabled() -> bool {
        path().is_some_and(|p| p.exists())
    }

    pub fn set(on: bool) -> Result<(), String> {
        let p = path().ok_or("pasta do usuário não encontrada")?;
        if !on {
            let _ = std::fs::remove_file(&p);
            return Ok(());
        }
        let plist = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>Label</key><string>com.hfabio.monitor-rede</string>
<key>ProgramArguments</key><array><string>{}</string></array>
<key>RunAtLoad</key><true/>
</dict></plist>
"#,
            super::exe()?
        );
        std::fs::create_dir_all(p.parent().unwrap()).map_err(|e| e.to_string())?;
        std::fs::write(&p, plist).map_err(|e| e.to_string())
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
mod imp {
    use std::path::PathBuf;

    fn path() -> Option<PathBuf> {
        dirs::config_dir().map(|c| c.join("autostart/monitor-rede.desktop"))
    }

    pub fn is_enabled() -> bool {
        path().is_some_and(|p| p.exists())
    }

    pub fn set(on: bool) -> Result<(), String> {
        let p = path().ok_or("pasta de configuração não encontrada")?;
        if !on {
            let _ = std::fs::remove_file(&p);
            return Ok(());
        }
        let desktop = format!("[Desktop Entry]\nType=Application\nName=Monitor de Rede\nExec=\"{}\"\nX-GNOME-Autostart-enabled=true\n", super::exe()?);
        std::fs::create_dir_all(p.parent().unwrap()).map_err(|e| e.to_string())?;
        std::fs::write(&p, desktop).map_err(|e| e.to_string())
    }
}

pub use imp::{is_enabled, set};
