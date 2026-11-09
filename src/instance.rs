//! Uma cópia só do app por vez, e no Windows instalado abrir sempre como
//! administrador, sem a pergunta do UAC.
//!
//! A cópia que está rodando escuta em 127.0.0.1:47811. Uma nova cópia que
//! encontra a primeira pede para ela mostrar a janela e fecha.
//!
//! No Windows, o instalador cria a tarefa agendada "MonitorRede", que roda
//! com privilégios mais altos. Quando o app é aberto sem ser administrador
//! (atalho, iniciar com o Windows) e essa tarefa existe, ele dispara a tarefa
//! e fecha: o Windows abre a cópia de administrador sem perguntar nada. Uma
//! cópia de administrador também substitui uma comum que já esteja aberta.

use eframe::egui;
use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::Mutex;
use std::time::{Duration, Instant};

const PORT: u16 = 47811;
const HELLO: &str = "monitor-rede";

static LISTENER: Mutex<Option<TcpListener>> = Mutex::new(None);

/// Resposta da cópia que já está rodando.
struct Other {
    elevated: bool,
}

/// Fala com a cópia que já está rodando. `None` = não há nenhuma.
fn send(cmd: &str) -> Option<Other> {
    let addr = SocketAddr::from(([127, 0, 0, 1], PORT));
    let mut s = TcpStream::connect_timeout(&addr, Duration::from_millis(500)).ok()?;
    s.set_read_timeout(Some(Duration::from_secs(2))).ok()?;
    if cmd == "show" {
        win::allow_foreground();
    }
    writeln!(s, "{cmd}").ok()?;
    let mut line = String::new();
    BufReader::new(s).read_line(&mut line).ok()?;
    let mut parts = line.split_whitespace();
    (parts.next() == Some(HELLO)).then(|| Other { elevated: parts.next() == Some("admin") })
}

/// Decide no início se este processo segue abrindo (`true`) ou fecha porque
/// outra cópia assumiu (`false`). `show`: mostrar a janela (falso com `--bandeja`).
pub fn start(show: bool) -> bool {
    if std::env::var_os("MONITOR_REDE_SHOTS").is_some() {
        return true;
    }
    let me_admin = win::is_elevated();
    let cmd = if show { "show" } else { "ping" };

    match send("ping") {
        // Já existe uma cópia igual ou "melhor": entrega para ela.
        Some(other) if other.elevated || !me_admin => {
            if show {
                send("show");
            }
            return false;
        }
        // Somos administrador e a outra não: ela fecha e nós assumimos.
        Some(_) => {
            send("quit");
        }
        None => {}
    }

    // Instalado no Windows e sem administrador: abre pela tarefa agendada.
    if !me_admin && win::task_exists() && win::run_task() {
        let deadline = Instant::now() + Duration::from_secs(15);
        while Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(300));
            if send(cmd).is_some() {
                return false;
            }
        }
        // A tarefa não abriu o app: segue sem administrador.
    }

    // Assume o posto (tenta por alguns segundos, se a anterior ainda está fechando).
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match TcpListener::bind(("127.0.0.1", PORT)) {
            Ok(l) => {
                *LISTENER.lock().unwrap() = Some(l);
                return true;
            }
            Err(_) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(200)),
            // Porta ocupada por outro programa: segue sem esse controle.
            Err(_) => return true,
        }
    }
}

/// Atende os pedidos de outras cópias (chamado quando a interface existe).
pub fn serve(ctx: egui::Context) {
    let Some(listener) = LISTENER.lock().unwrap().take() else { return };
    let me = if win::is_elevated() { "admin" } else { "user" };
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
            let mut line = String::new();
            let Ok(mut s) = stream.try_clone() else { continue };
            if BufReader::new(stream).read_line(&mut line).is_err() {
                continue;
            }
            let _ = writeln!(s, "{HELLO} {me}");
            let _ = s.flush();
            match line.trim() {
                "show" => crate::tray::wake(&ctx),
                "quit" => std::process::exit(0),
                _ => {}
            }
        }
    });
}

/// Argumentos usados pelo instalador do Windows. `Some(código de saída)` se
/// o argumento foi tratado.
pub fn installer_command() -> Option<i32> {
    let arg = std::env::args().nth(1)?;
    let r = match arg.as_str() {
        "--instalar-tarefa" => win::install_task(),
        "--remover-tarefa" => win::remove_task(),
        _ => return None,
    };
    Some(if r { 0 } else { 1 })
}

#[cfg(windows)]
mod win {
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    const TASK: &str = "MonitorRede";
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    fn schtasks(args: &[&str]) -> bool {
        Command::new("schtasks").args(args).creation_flags(CREATE_NO_WINDOW).output().is_ok_and(|o| o.status.success())
    }

    pub fn is_elevated() -> bool {
        unsafe { windows_sys::Win32::UI::Shell::IsUserAnAdmin() != 0 }
    }

    /// Deixa a cópia que já roda trazer a janela para a frente.
    pub fn allow_foreground() {
        use windows_sys::Win32::UI::WindowsAndMessaging::{AllowSetForegroundWindow, ASFW_ANY};
        unsafe {
            AllowSetForegroundWindow(ASFW_ANY);
        }
    }

    pub fn task_exists() -> bool {
        schtasks(&["/query", "/tn", TASK])
    }

    pub fn run_task() -> bool {
        schtasks(&["/run", "/tn", TASK])
    }

    pub fn remove_task() -> bool {
        !task_exists() || schtasks(&["/delete", "/tn", TASK, "/f"])
    }

    fn xml_escape(s: &str) -> String {
        s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
    }

    /// Tarefa sem gatilho (só roda quando o app a dispara), para este
    /// usuário, com os privilégios mais altos e sem limite de tempo.
    pub fn task_xml(exe: &str, user: &str) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo>
    <Description>Abre o Monitor de Rede como administrador, sem a pergunta do UAC, para medir a banda de cada aplicativo.</Description>
  </RegistrationInfo>
  <Principals>
    <Principal id="Author">
      <UserId>{user}</UserId>
      <LogonType>InteractiveToken</LogonType>
      <RunLevel>HighestAvailable</RunLevel>
    </Principal>
  </Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <AllowHardTerminate>true</AllowHardTerminate>
    <StartWhenAvailable>false</StartWhenAvailable>
    <AllowStartOnDemand>true</AllowStartOnDemand>
    <Enabled>true</Enabled>
    <Hidden>false</Hidden>
    <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>
    <Priority>6</Priority>
  </Settings>
  <Actions Context="Author">
    <Exec>
      <Command>{exe}</Command>
      <Arguments>{arg}</Arguments>
    </Exec>
  </Actions>
</Task>
"#,
            exe = xml_escape(exe),
            user = xml_escape(user),
            arg = crate::tray::START_HIDDEN_ARG,
        )
    }

    pub fn install_task() -> bool {
        let Ok(exe) = std::env::current_exe() else { return false };
        let user = match (std::env::var("USERDOMAIN"), std::env::var("USERNAME")) {
            (Ok(d), Ok(u)) => format!("{d}\\{u}"),
            (_, Ok(u)) => u,
            _ => return false,
        };
        // O schtasks lê o XML em UTF-16 com BOM.
        let xml = task_xml(&exe.to_string_lossy(), &user);
        let mut bytes = vec![0xFF, 0xFE];
        bytes.extend(xml.encode_utf16().flat_map(u16::to_le_bytes));
        let path = std::env::temp_dir().join("monitor-rede-tarefa.xml");
        if std::fs::write(&path, bytes).is_err() {
            return false;
        }
        let ok = schtasks(&["/create", "/tn", TASK, "/xml", &path.to_string_lossy(), "/f"]);
        let _ = std::fs::remove_file(&path);
        ok
    }
}

#[cfg(not(windows))]
mod win {
    pub fn is_elevated() -> bool {
        false
    }
    pub fn allow_foreground() {}
    pub fn task_exists() -> bool {
        false
    }
    pub fn run_task() -> bool {
        false
    }
    pub fn install_task() -> bool {
        false
    }
    pub fn remove_task() -> bool {
        false
    }
}

#[cfg(all(test, windows))]
mod tests {
    #[test]
    fn task_xml_escapes() {
        let x = super::win::task_xml(r"C:\A & B\MonitorRede.exe", r"PC\Ana");
        assert!(x.contains(r"<Command>C:\A &amp; B\MonitorRede.exe</Command>") && x.contains("<RunLevel>HighestAvailable</RunLevel>"));
    }
}
