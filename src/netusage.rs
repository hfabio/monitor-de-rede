//! Uso da rede: banda total (todas as plataformas) e por aplicativo (Windows).
//!
//! No Windows, a banda por aplicativo vem das estatísticas por conexão TCP
//! (GetPerTcpConnectionEStats). Ligar essa coleta exige administrador; sem
//! isso mostramos só o número de conexões de cada aplicativo. Tráfego UDP
//! (parte das chamadas de vídeo e jogos) não aparece por aplicativo, só no total.

use crate::monitor::now_secs;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use sysinfo::Networks;

const HISTORY_SECS: f64 = 300.0;

#[derive(Clone, Debug)]
pub struct AppUsage {
    pub name: String,
    pub down_bps: f64,
    pub up_bps: f64,
    pub conns: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PerApp {
    /// Bytes por aplicativo disponíveis.
    Full,
    /// Só número de conexões (falta permissão de administrador).
    ConnectionsOnly,
    Unsupported,
}

pub struct Usage {
    pub down_bps: f64,
    pub up_bps: f64,
    /// (t, down, up) dos últimos 5 minutos.
    pub history: VecDeque<(f64, f64, f64)>,
    pub apps: Vec<AppUsage>,
    pub per_app: PerApp,
}

pub fn spawn() -> Arc<Mutex<Usage>> {
    let usage = Arc::new(Mutex::new(Usage {
        down_bps: 0.0,
        up_bps: 0.0,
        history: VecDeque::new(),
        apps: Vec::new(),
        per_app: if cfg!(windows) { PerApp::ConnectionsOnly } else { PerApp::Unsupported },
    }));
    let out = usage.clone();
    std::thread::spawn(move || run(usage));
    out
}

/// Interfaces que não são tráfego real para a internet (evita contar em dobro).
fn ignored_iface(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    ["loopback", "vethernet", "virtualbox", "vmware", "isatap", "teredo", "bridge", "docker", "veth", "utun", "awdl", "llw", "anpi", "ap1"]
        .iter()
        .any(|p| n.contains(p))
        || n == "lo"
        || n.starts_with("lo0")
}

fn run(usage: Arc<Mutex<Usage>>) {
    let Ok(mut networks) = Networks::new_with_refreshed_list() else { return };
    #[cfg(windows)]
    let mut apps = win::AppTracker::new();
    let mut last = std::time::Instant::now();

    loop {
        std::thread::sleep(Duration::from_secs(1));
        networks.refresh(true);
        let dt = last.elapsed().as_secs_f64().max(0.001);
        last = std::time::Instant::now();

        let (mut rx, mut tx) = (0u64, 0u64);
        for (name, data) in networks.iter() {
            if !ignored_iface(&name.to_string_lossy()) {
                rx += data.received();
                tx += data.transmitted();
            }
        }
        let down = rx as f64 * 8.0 / dt;
        let up = tx as f64 * 8.0 / dt;

        #[cfg(windows)]
        let (app_list, mode) = apps.sample(dt);
        #[cfg(not(windows))]
        let (app_list, mode) = (Vec::new(), PerApp::Unsupported);

        let t = now_secs();
        let mut u = usage.lock().unwrap();
        u.down_bps = down;
        u.up_bps = up;
        u.history.push_back((t, down, up));
        while u.history.front().is_some_and(|h| h.0 < t - HISTORY_SECS) {
            u.history.pop_front();
        }
        u.apps = app_list;
        u.per_app = mode;
    }
}

/// Nome amigável para executáveis conhecidos.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn friendly_name(exe: &str) -> String {
    let stem = exe.trim_end_matches(".exe").trim_end_matches(".EXE");
    let known = [
        ("chrome", "Google Chrome"),
        ("msedge", "Microsoft Edge"),
        ("firefox", "Firefox"),
        ("opera", "Opera"),
        ("brave", "Brave"),
        ("ms-teams", "Microsoft Teams"),
        ("teams", "Microsoft Teams"),
        ("zoom", "Zoom"),
        ("whatsapp", "WhatsApp"),
        ("onedrive", "OneDrive"),
        ("dropbox", "Dropbox"),
        ("steam", "Steam"),
        ("steamwebhelper", "Steam"),
        ("epicgameslauncher", "Epic Games"),
        ("discord", "Discord"),
        ("spotify", "Spotify"),
        ("svchost", "Serviços do Windows"),
        ("system", "Sistema"),
        ("wuauserv", "Windows Update"),
        ("monitor-rede", "Monitor de Rede"),
        ("monitorrede", "Monitor de Rede"),
    ];
    let lower = stem.to_ascii_lowercase();
    known
        .iter()
        .find(|(k, _)| lower == *k)
        .map(|(_, v)| v.to_string())
        .unwrap_or_else(|| stem.to_string())
}

/// Abre o próprio programa como administrador (para liberar a banda por app).
#[cfg(windows)]
pub fn relaunch_as_admin() -> bool {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let Ok(exe) = std::env::current_exe() else { return false };
    let w = |s: &str| s.encode_utf16().chain(Some(0)).collect::<Vec<u16>>();
    let (verb, file) = (w("runas"), w(&exe.to_string_lossy()));
    let r = unsafe {
        ShellExecuteW(std::ptr::null_mut(), verb.as_ptr(), file.as_ptr(), std::ptr::null(), std::ptr::null(), SW_SHOWNORMAL)
    };
    r as isize > 32
}

#[cfg(windows)]
mod win {
    use super::{friendly_name, AppUsage, PerApp};
    use std::collections::HashMap;
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::NetworkManagement::IpHelper::*;
    use windows_sys::Win32::Networking::WinSock::{AF_INET, AF_INET6, IN6_ADDR, IN6_ADDR_0};
    use windows_sys::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION};

    const ESTABLISHED: u32 = 5;
    const ERROR_ACCESS_DENIED: u32 = 5;

    #[derive(Clone, Copy, PartialEq, Eq, Hash)]
    enum Key {
        V4 { la: u32, lp: u32, ra: u32, rp: u32 },
        V6 { la: [u8; 16], lp: u32, ra: [u8; 16], rp: u32 },
    }

    struct Conn {
        key: Key,
        pid: u32,
        scope: (u32, u32),
    }

    pub struct AppTracker {
        /// Último (bytes in, bytes out) visto por conexão.
        last: HashMap<Key, (u64, u64)>,
        names: HashMap<u32, String>,
        estats: Option<bool>,
    }

    impl AppTracker {
        pub fn new() -> Self {
            Self { last: HashMap::new(), names: HashMap::new(), estats: None }
        }

        pub fn sample(&mut self, dt: f64) -> (Vec<AppUsage>, PerApp) {
            let conns = tcp_connections();
            let mut per: HashMap<String, AppUsage> = HashMap::new();
            let mut seen = HashMap::new();

            for c in &conns {
                let name = self.name_of(c.pid);
                let e = per.entry(name.clone()).or_insert(AppUsage { name, down_bps: 0.0, up_bps: 0.0, conns: 0 });
                e.conns += 1;
                if self.estats == Some(false) {
                    continue;
                }
                let Some((bin, bout)) = self.read(c) else { continue };
                if let Some((pin, pout)) = self.last.get(&c.key) {
                    e.down_bps += bin.saturating_sub(*pin) as f64 * 8.0 / dt;
                    e.up_bps += bout.saturating_sub(*pout) as f64 * 8.0 / dt;
                }
                seen.insert(c.key, (bin, bout));
            }
            self.last = seen;
            if self.names.len() > 2000 {
                self.names.clear();
            }

            let mut list: Vec<AppUsage> = per.into_values().collect();
            list.sort_by(|a, b| (b.down_bps + b.up_bps).total_cmp(&(a.down_bps + a.up_bps)).then(b.conns.cmp(&a.conns)));
            let mode = if self.estats == Some(true) { PerApp::Full } else { PerApp::ConnectionsOnly };
            (list, mode)
        }

        /// Liga a coleta na conexão (se ainda não) e lê os contadores.
        fn read(&mut self, c: &Conn) -> Option<(u64, u64)> {
            let rw = TCP_ESTATS_DATA_RW_v0 { EnableCollection: true };
            let mut rod: TCP_ESTATS_DATA_ROD_v0 = unsafe { std::mem::zeroed() };
            let rw_ptr = &rw as *const _ as *const u8;
            let rw_size = std::mem::size_of_val(&rw) as u32;
            let rod_ptr = &mut rod as *mut _ as *mut u8;
            let rod_size = std::mem::size_of_val(&rod) as u32;
            let (set, get) = unsafe {
                match c.key {
                    Key::V4 { la, lp, ra, rp } => {
                        let mut row: MIB_TCPROW_LH = std::mem::zeroed();
                        row.Anonymous.dwState = ESTABLISHED;
                        row.dwLocalAddr = la;
                        row.dwLocalPort = lp;
                        row.dwRemoteAddr = ra;
                        row.dwRemotePort = rp;
                        let set = if self.last.contains_key(&c.key) {
                            0
                        } else {
                            SetPerTcpConnectionEStats(&row, TcpConnectionEstatsData, rw_ptr, 0, rw_size, 0)
                        };
                        let get = GetPerTcpConnectionEStats(&row, TcpConnectionEstatsData, std::ptr::null_mut(), 0, 0, std::ptr::null_mut(), 0, 0, rod_ptr, 0, rod_size);
                        (set, get)
                    }
                    Key::V6 { la, lp, ra, rp } => {
                        let row = MIB_TCP6ROW {
                            State: ESTABLISHED as MIB_TCP_STATE,
                            LocalAddr: IN6_ADDR { u: IN6_ADDR_0 { Byte: la } },
                            dwLocalScopeId: c.scope.0,
                            dwLocalPort: lp,
                            RemoteAddr: IN6_ADDR { u: IN6_ADDR_0 { Byte: ra } },
                            dwRemoteScopeId: c.scope.1,
                            dwRemotePort: rp,
                        };
                        let set = if self.last.contains_key(&c.key) {
                            0
                        } else {
                            SetPerTcp6ConnectionEStats(&row, TcpConnectionEstatsData, rw_ptr, 0, rw_size, 0)
                        };
                        let get = GetPerTcp6ConnectionEStats(&row, TcpConnectionEstatsData, std::ptr::null_mut(), 0, 0, std::ptr::null_mut(), 0, 0, rod_ptr, 0, rod_size);
                        (set, get)
                    }
                }
            };
            if set == ERROR_ACCESS_DENIED {
                self.estats = Some(false);
                return None;
            }
            if set == 0 && get == 0 {
                self.estats = Some(true);
                Some((rod.DataBytesIn, rod.DataBytesOut))
            } else {
                None
            }
        }

        fn name_of(&mut self, pid: u32) -> String {
            if let Some(n) = self.names.get(&pid) {
                return n.clone();
            }
            let n = match pid {
                0 => "Sistema".to_string(),
                4 => "Sistema".to_string(),
                _ => process_exe(pid).map(|e| friendly_name(&e)).unwrap_or_else(|| format!("Processo {pid}")),
            };
            self.names.insert(pid, n.clone());
            n
        }
    }

    fn process_exe(pid: u32) -> Option<String> {
        unsafe {
            let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if h.is_null() {
                return None;
            }
            let mut buf = [0u16; 520];
            let mut len = buf.len() as u32;
            let ok = QueryFullProcessImageNameW(h, 0, buf.as_mut_ptr(), &mut len);
            CloseHandle(h);
            if ok == 0 {
                return None;
            }
            let path = String::from_utf16_lossy(&buf[..len as usize]);
            path.rsplit(['\\', '/']).next().map(|s| s.to_string())
        }
    }

    /// Lê a tabela de conexões TCP (IPv4 e IPv6) com o PID dono.
    fn table(af: u16) -> Vec<u8> {
        let mut size = 0u32;
        unsafe { GetExtendedTcpTable(std::ptr::null_mut(), &mut size, 0, af as u32, TCP_TABLE_OWNER_PID_ALL, 0) };
        for _ in 0..3 {
            // Vec<u64> para alinhar as structs.
            let mut buf = vec![0u64; (size as usize).div_ceil(8) + 64];
            let mut sz = (buf.len() * 8) as u32;
            let rc = unsafe { GetExtendedTcpTable(buf.as_mut_ptr() as *mut _, &mut sz, 0, af as u32, TCP_TABLE_OWNER_PID_ALL, 0) };
            if rc == 0 {
                let bytes = unsafe { std::slice::from_raw_parts(buf.as_ptr() as *const u8, sz as usize) }.to_vec();
                return bytes;
            }
            size = sz;
        }
        Vec::new()
    }

    fn tcp_connections() -> Vec<Conn> {
        let mut out = Vec::new();
        let t4 = table(AF_INET);
        if t4.len() >= 4 {
            let n = u32::from_ne_bytes(t4[..4].try_into().unwrap()) as usize;
            let rows = unsafe {
                let tab = t4.as_ptr() as *const MIB_TCPTABLE_OWNER_PID;
                std::slice::from_raw_parts((*tab).table.as_ptr(), n)
            };
            for r in rows {
                let remote = std::net::Ipv4Addr::from(r.dwRemoteAddr.to_ne_bytes());
                if r.dwState == ESTABLISHED && !remote.is_loopback() {
                    out.push(Conn {
                        key: Key::V4 { la: r.dwLocalAddr, lp: r.dwLocalPort, ra: r.dwRemoteAddr, rp: r.dwRemotePort },
                        pid: r.dwOwningPid,
                        scope: (0, 0),
                    });
                }
            }
        }
        let t6 = table(AF_INET6);
        if t6.len() >= 4 {
            let n = u32::from_ne_bytes(t6[..4].try_into().unwrap()) as usize;
            let rows = unsafe {
                let tab = t6.as_ptr() as *const MIB_TCP6TABLE_OWNER_PID;
                std::slice::from_raw_parts((*tab).table.as_ptr(), n)
            };
            for r in rows {
                let remote = std::net::Ipv6Addr::from(r.ucRemoteAddr);
                if r.dwState == ESTABLISHED && !remote.is_loopback() {
                    out.push(Conn {
                        key: Key::V6 { la: r.ucLocalAddr, lp: r.dwLocalPort, ra: r.ucRemoteAddr, rp: r.dwRemotePort },
                        pid: r.dwOwningPid,
                        scope: (r.dwLocalScopeId, r.dwRemoteScopeId),
                    });
                }
            }
        }
        out
    }
}
