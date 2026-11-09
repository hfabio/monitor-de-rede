//! Atualização automática pelas Releases do GitHub.
//!
//! Uma thread consulta a última Release ao abrir e a cada 6 horas. Se houver
//! versão nova:
//! - Windows instalado: baixa o instalador e roda em modo silencioso (ele
//!   fecha o app, atualiza e abre de novo);
//! - Windows sem instalar e Linux: troca o executável e reabre;
//! - macOS: troca o "Monitor de Rede.app" e reabre;
//! - Android: avisa, com um botão que baixa o APK novo pelo navegador (o
//!   Android não deixa um app se atualizar sozinho sem a Play Store).

// No Android só a consulta é usada.
#![cfg_attr(target_os = "android", allow(dead_code, unused_imports))]

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

const API: &str = "https://api.github.com/repos/hfabio/monitor-de-rede/releases/latest";
pub const CURRENT: &str = env!("CARGO_PKG_VERSION");
const FIRST_CHECK: Duration = Duration::from_secs(15);
const EVERY: Duration = Duration::from_secs(6 * 3600);
/// Argumento da cópia que acabou de ser atualizada: ela assume o lugar da antiga.
pub const UPDATED_ARG: &str = "--atualizado";
/// Mesmo argumento de `tray::START_HIDDEN_ARG` (abrir direto na bandeja).
const HIDDEN_ARG: &str = "--bandeja";

#[derive(Clone, Debug)]
pub struct Release {
    pub version: String,
    /// Página da Release (novidades).
    pub page: String,
    /// Arquivo desta plataforma.
    pub asset: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub enum State {
    #[default]
    Idle,
    Checking,
    UpToDate,
    Available(Release),
    Installing(String),
    Failed(String),
}

impl State {
    /// Texto curto para a interface.
    pub fn describe(&self) -> String {
        match self {
            State::Idle => format!("Versão {CURRENT}"),
            State::Checking => "Procurando atualização…".into(),
            State::UpToDate => format!("Versão {CURRENT} · é a mais recente"),
            State::Available(r) => format!("Versão {} disponível (você tem a {CURRENT})", r.version),
            State::Installing(v) => format!("Baixando e instalando a versão {v}…"),
            State::Failed(e) => e.clone(),
        }
    }
}

pub struct Updater {
    state: Mutex<State>,
    /// Procura sozinho (ao abrir e a cada 6 h). Desligado: só no "Procurar agora".
    pub check: AtomicBool,
    pub auto: AtomicBool,
    wake: (Mutex<bool>, Condvar),
}

impl Updater {
    pub fn state(&self) -> State {
        self.state.lock().unwrap().clone()
    }

    fn set(&self, s: State) {
        *self.state.lock().unwrap() = s;
    }

    /// Procura agora (botão "Procurar atualização").
    pub fn check_now(&self) {
        *self.wake.0.lock().unwrap() = true;
        self.wake.1.notify_all();
    }

    /// Espera `d` ou um "Procurar agora". Devolve `true` se foi pedido.
    fn sleep(&self, d: Duration) -> bool {
        let guard = self.wake.0.lock().unwrap();
        let (mut g, _) = self.wake.1.wait_timeout_while(guard, d, |woken| !*woken).unwrap();
        std::mem::take(&mut *g)
    }
}

/// Inicia a verificação periódica. `repaint` avisa a interface que algo mudou.
pub fn spawn(check_on: bool, auto: bool, repaint: impl Fn() + Send + 'static) -> Arc<Updater> {
    let up = Arc::new(Updater {
        state: Mutex::new(State::Idle),
        check: AtomicBool::new(check_on),
        auto: AtomicBool::new(auto),
        wake: (Mutex::new(false), Condvar::new()),
    });
    let u = up.clone();
    std::thread::spawn(move || {
        let mut asked = u.sleep(FIRST_CHECK);
        loop {
            // Só consulta o GitHub se a opção estiver ligada ou se pediram agora.
            // Não interrompe uma instalação em andamento.
            if (asked || u.check.load(Ordering::Relaxed)) && !matches!(u.state(), State::Installing(_)) {
                u.set(State::Checking);
                repaint();
                match check() {
                    Ok(Some(r)) => {
                        u.set(State::Available(r.clone()));
                        repaint();
                        if cfg!(not(target_os = "android")) && u.auto.load(Ordering::Relaxed) && u.check.load(Ordering::Relaxed) {
                            install(&u, &r, false);
                        }
                    }
                    Ok(None) => u.set(State::UpToDate),
                    Err(e) => u.set(State::Failed(e)),
                }
                repaint();
            }
            asked = u.sleep(EVERY);
        }
    });
    up
}

fn agent() -> ureq::Agent {
    ureq::Agent::new_with_config(ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(600))).build())
}

/// `a` é mais nova que `b`? (versões "x.y.z", com ou sem "v").
pub fn newer(a: &str, b: &str) -> bool {
    let parse = |v: &str| -> Vec<u64> { v.trim_start_matches('v').split('.').map(|p| p.parse().unwrap_or(0)).collect() };
    parse(a) > parse(b)
}

/// Nome do arquivo da Release para esta plataforma (e modo, no Windows).
fn asset_name() -> &'static str {
    if cfg!(target_os = "android") {
        "MonitorRede-android-arm64.apk"
    } else if cfg!(windows) {
        if windows_installed() {
            "MonitorRede-windows-x64-instalador.exe"
        } else {
            "MonitorRede-windows-x64.exe"
        }
    } else if cfg!(target_os = "macos") {
        "MonitorRede-macos-universal.zip"
    } else {
        "MonitorRede-linux-x64.tar.gz"
    }
}

/// Instalado pelo instalador: o desinstalador fica na mesma pasta do app.
fn windows_installed() -> bool {
    exe().and_then(|e| e.parent().map(|d| d.join("unins000.exe").exists())).unwrap_or(false)
}

fn exe() -> Option<PathBuf> {
    std::env::current_exe().ok()
}

/// Consulta a última Release. `None` = já está na última versão.
pub fn check() -> Result<Option<Release>, String> {
    let text = agent()
        .get(API)
        .header("User-Agent", format!("monitor-rede/{CURRENT}"))
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| format!("Não consegui consultar o GitHub: {e}"))?
        .body_mut()
        .read_to_string()
        .map_err(|e| e.to_string())?;
    let v: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let tag = v["tag_name"].as_str().ok_or("Resposta do GitHub sem versão")?;
    if !newer(tag, CURRENT) {
        return Ok(None);
    }
    let asset = v["assets"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|a| a["name"].as_str() == Some(asset_name()))
        .and_then(|a| a["browser_download_url"].as_str())
        .map(String::from);
    Ok(Some(Release {
        version: tag.trim_start_matches('v').to_string(),
        page: v["html_url"].as_str().unwrap_or("https://github.com/hfabio/monitor-de-rede/releases/latest").to_string(),
        asset,
    }))
}

/// Baixa e instala. Se der certo, o app reabre na versão nova e este
/// processo fecha. `show`: abrir a janela depois (falso = volta para a bandeja).
pub fn install(up: &Updater, r: &Release, show: bool) {
    if cfg!(target_os = "android") {
        return;
    }
    up.set(State::Installing(r.version.clone()));
    let result = r.asset.as_deref().ok_or_else(|| "Esta versão não tem arquivo para este sistema.".to_string()).and_then(|url| {
        let bytes = download(url)?;
        replace_and_restart(&bytes, show)
    });
    if let Err(e) = result {
        up.set(State::Failed(format!("Não consegui atualizar para a {}: {e}", r.version)));
    }
}

fn download(url: &str) -> Result<Vec<u8>, String> {
    agent()
        .get(url)
        .header("User-Agent", format!("monitor-rede/{CURRENT}"))
        .call()
        .map_err(|e| format!("download falhou: {e}"))?
        .body_mut()
        .with_config()
        .limit(200 * 1024 * 1024)
        .read_to_vec()
        .map_err(|e| format!("download falhou: {e}"))
}

fn temp_file(name: &str, bytes: &[u8]) -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join(format!("monitor-rede-update-{}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let p = dir.join(name);
    std::fs::write(&p, bytes).map_err(|e| e.to_string())?;
    Ok(p)
}

#[cfg_attr(target_os = "android", allow(dead_code))]
fn relaunch_args(show: bool) -> Vec<&'static str> {
    let mut a = vec![UPDATED_ARG];
    if !show {
        a.push(HIDDEN_ARG);
    }
    a
}

#[cfg(windows)]
fn replace_and_restart(bytes: &[u8], show: bool) -> Result<(), String> {
    let exe = exe().ok_or("caminho do app desconhecido")?;
    if windows_installed() {
        // O instalador fecha o app, atualiza e (em modo silencioso) abre de novo.
        let setup = temp_file("MonitorRede-instalador.exe", bytes)?;
        let params = format!("/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /appargs={}", if show { "--abrir" } else { HIDDEN_ARG });
        if !shell_execute(&setup, &params) {
            return Err("o instalador não abriu".into());
        }
        std::thread::sleep(Duration::from_secs(60));
        return Err("o instalador não terminou".into());
    }
    // Sem instalar: o Windows deixa renomear o executável em uso.
    let old = exe.with_extension("exe.old");
    let _ = std::fs::remove_file(&old);
    let new = exe.with_extension("exe.new");
    std::fs::write(&new, bytes).map_err(|e| format!("sem permissão para gravar em {}: {e}", exe.parent().unwrap_or(&exe).display()))?;
    std::fs::rename(&exe, &old).map_err(|e| e.to_string())?;
    if let Err(e) = std::fs::rename(&new, &exe) {
        let _ = std::fs::rename(&old, &exe);
        return Err(e.to_string());
    }
    std::process::Command::new(&exe).args(relaunch_args(show)).spawn().map_err(|e| e.to_string())?;
    std::process::exit(0);
}

#[cfg(windows)]
fn shell_execute(file: &Path, params: &str) -> bool {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let w = |s: &str| s.encode_utf16().chain(Some(0)).collect::<Vec<u16>>();
    let (verb, f, p) = (w("open"), w(&file.to_string_lossy()), w(params));
    let r = unsafe { ShellExecuteW(std::ptr::null_mut(), verb.as_ptr(), f.as_ptr(), p.as_ptr(), std::ptr::null(), SW_SHOWNORMAL) };
    r as isize > 32
}

/// Pasta "Monitor de Rede.app" de onde o app está rodando.
#[cfg(target_os = "macos")]
fn bundle() -> Option<PathBuf> {
    let b = exe()?.parent()?.parent()?.parent()?.to_path_buf();
    (b.extension()? == "app").then_some(b)
}

#[cfg(target_os = "macos")]
fn run(cmd: &str, args: &[&std::ffi::OsStr]) -> Result<(), String> {
    let st = std::process::Command::new(cmd).args(args).status().map_err(|e| e.to_string())?;
    st.success().then_some(()).ok_or_else(|| format!("{cmd} falhou"))
}

/// Copia o .app para `dest` (substituindo o que houver) e abre a cópia.
#[cfg(target_os = "macos")]
fn install_bundle(src: &Path, dest: &Path, args: &[&str]) -> Result<(), String> {
    use std::ffi::OsStr;
    let tmp = dest.with_extension("app.new");
    let old = dest.with_extension("app.old");
    let _ = std::fs::remove_dir_all(&tmp);
    let _ = std::fs::remove_dir_all(&old);
    run("ditto", &[src.as_os_str(), tmp.as_os_str()])?;
    let _ = run("xattr", &[OsStr::new("-dr"), OsStr::new("com.apple.quarantine"), tmp.as_os_str()]);
    if dest.exists() {
        std::fs::rename(dest, &old).map_err(|e| format!("sem permissão em {}: {e}", dest.display()))?;
    }
    if let Err(e) = std::fs::rename(&tmp, dest) {
        let _ = std::fs::rename(&old, dest);
        return Err(e.to_string());
    }
    let _ = std::fs::remove_dir_all(&old);
    let exe_name = exe().and_then(|e| e.file_name().map(|n| n.to_owned())).unwrap_or_else(|| "monitor-rede".into());
    std::process::Command::new(dest.join("Contents/MacOS").join(exe_name)).args(args).spawn().map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn replace_and_restart(bytes: &[u8], show: bool) -> Result<(), String> {
    use std::ffi::OsStr;
    let dest = bundle().ok_or("o app não está rodando de um .app")?;
    let zip = temp_file("MonitorRede.zip", bytes)?;
    let dir = zip.with_file_name("extraido");
    run("ditto", &[OsStr::new("-x"), OsStr::new("-k"), zip.as_os_str(), dir.as_os_str()])?;
    let src = dir.join("Monitor de Rede.app");
    install_bundle(&src, &dest, &relaunch_args(show))?;
    std::process::exit(0);
}

#[cfg(all(unix, not(target_os = "macos"), not(target_os = "android")))]
fn replace_and_restart(bytes: &[u8], show: bool) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let exe = exe().ok_or("caminho do app desconhecido")?;
    let tgz = temp_file("MonitorRede.tar.gz", bytes)?;
    let dir = tgz.parent().unwrap().to_path_buf();
    let st = std::process::Command::new("tar").arg("-xzf").arg(&tgz).arg("-C").arg(&dir).status().map_err(|e| e.to_string())?;
    if !st.success() {
        return Err("não consegui extrair o arquivo".into());
    }
    let new = exe.with_extension("new");
    std::fs::copy(dir.join("monitor-rede"), &new).map_err(|e| format!("sem permissão para gravar em {}: {e}", exe.display()))?;
    std::fs::set_permissions(&new, std::fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
    std::fs::rename(&new, &exe).map_err(|e| e.to_string())?;
    std::process::Command::new(&exe).args(relaunch_args(show)).spawn().map_err(|e| e.to_string())?;
    std::process::exit(0);
}

#[cfg(target_os = "android")]
fn replace_and_restart(_bytes: &[u8], _show: bool) -> Result<(), String> {
    Err("no Android, baixe o APK novo".into())
}

/// Limpa sobras de uma atualização anterior (Windows sem instalar).
pub fn cleanup() {
    #[cfg(windows)]
    if let Some(e) = exe() {
        let _ = std::fs::remove_file(e.with_extension("exe.old"));
    }
}

/// macOS: se o app foi aberto fora de Aplicativos (Downloads, imagem .dmg),
/// copia para /Applications (ou ~/Applications, sem permissão), abre a cópia
/// e devolve `true` (este processo deve fechar).
#[cfg(target_os = "macos")]
pub fn move_to_applications() -> bool {
    let Some(current) = bundle() else { return false };
    let home_apps = dirs::home_dir().map(|h| h.join("Applications"));
    let parent = current.parent().map(Path::to_path_buf);
    if parent.as_deref() == Some(Path::new("/Applications")) || parent == home_apps {
        return false;
    }
    let name = current.file_name().unwrap_or_else(|| std::ffi::OsStr::new("Monitor de Rede.app")).to_owned();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let mut targets = vec![PathBuf::from("/Applications")];
    targets.extend(home_apps.clone());
    // Para testes: outra pasta de destino.
    if let Some(dir) = std::env::var_os("MONITOR_REDE_APPS_DIR") {
        if parent.as_deref() == Some(Path::new(&dir)) {
            return false;
        }
        targets = vec![PathBuf::from(dir)];
    }
    for dir in targets {
        let _ = std::fs::create_dir_all(&dir);
        if install_bundle(&current, &dir.join(&name), &args).is_ok() {
            // Apaga o original se estiver numa pasta comum (não numa imagem
            // .dmg nem na cópia temporária do Gatekeeper).
            let s = current.to_string_lossy();
            if !s.starts_with("/Volumes/") && !s.contains("AppTranslocation") {
                let _ = std::fs::remove_dir_all(&current);
            }
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::newer;

    #[test]
    fn compares_versions() {
        assert!(newer("v0.6.0", "0.5.0"));
        assert!(newer("0.10.0", "0.9.9"));
        assert!(!newer("v0.5.0", "0.5.0"));
        assert!(!newer("0.4.9", "0.5.0"));
    }
}
