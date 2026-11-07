//! Conexão com o PC da casa: lê o painel só leitura do app de desktop
//! (`/api/status`) pela rede local, a cada 2 s, e procura PCs na rede.
//!
//! HTTP/1.1 simples sobre TcpStream: o painel é http puro e só responde na
//! rede de casa.

use serde_json::Value;
use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream, ToSocketAddrs};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const POLL: Duration = Duration::from_secs(2);

#[derive(Default)]
pub struct PcState {
    /// "ip:porta" do PC escolhido (None = nenhum).
    pub addr: Option<String>,
    /// Última resposta do `/api/status`.
    pub data: Option<Value>,
    pub last_ok: Option<Instant>,
    /// A última tentativa falhou (PC desligado, app fechado, outra rede…).
    pub offline: bool,
    /// Muda a cada troca de endereço, para buscar na hora.
    generation: u64,
}

impl PcState {
    pub fn set_addr(&mut self, addr: Option<String>) {
        if self.addr != addr {
            self.addr = addr;
            self.data = None;
            self.last_ok = None;
            self.offline = false;
            self.generation += 1;
        }
    }

    pub fn online(&self) -> bool {
        self.data.is_some() && !self.offline
    }
}

/// Thread que consulta o PC enquanto o app está aberto.
pub fn spawn(state: Arc<Mutex<PcState>>, ctx: eframe::egui::Context) {
    std::thread::spawn(move || {
        let mut seen = u64::MAX;
        let mut last = Instant::now() - POLL;
        loop {
            let (addr, generation) = {
                let s = state.lock().unwrap();
                (s.addr.clone(), s.generation)
            };
            if generation != seen || last.elapsed() >= POLL {
                seen = generation;
                last = Instant::now();
                if let Some(addr) = addr {
                    let res = get_status(&addr);
                    let mut s = state.lock().unwrap();
                    if s.generation == generation {
                        match res {
                            Some(v) => {
                                s.data = Some(v);
                                s.last_ok = Some(Instant::now());
                                s.offline = false;
                            }
                            None => s.offline = true,
                        }
                    }
                    drop(s);
                    ctx.request_repaint();
                }
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    });
}

/// "192.168.68.11", "192.168.68.11:8080" ou "pc.local:8080" → endereço de socket.
pub fn parse_addr(input: &str, default_port: u16) -> Option<String> {
    let s = input.trim().trim_start_matches("http://").trim_end_matches('/');
    if s.is_empty() || s.contains(['/', ' ']) {
        return None;
    }
    let (host, port) = match s.rsplit_once(':') {
        Some((h, p)) => (h, p.parse::<u16>().ok()?),
        None => (s, default_port),
    };
    if host.is_empty() {
        return None;
    }
    Some(format!("{host}:{port}"))
}

/// GET /api/status. Só aceita a resposta se for o JSON do Monitor de Rede.
pub fn get_status(addr: &str) -> Option<Value> {
    let body = http_get(addr, "/api/status", Duration::from_millis(1500), Duration::from_secs(3))?;
    let v: Value = serde_json::from_slice(&body).ok()?;
    v.get("overall").is_some().then_some(v)
}

fn http_get(addr: &str, path: &str, connect: Duration, read: Duration) -> Option<Vec<u8>> {
    let sock = addr.to_socket_addrs().ok()?.find(|a| a.is_ipv4())?;
    let mut s = TcpStream::connect_timeout(&sock, connect).ok()?;
    s.set_read_timeout(Some(read)).ok()?;
    s.set_write_timeout(Some(read)).ok()?;
    write!(s, "GET {path} HTTP/1.1\r\nHost: {addr}\r\nAccept: application/json\r\nConnection: close\r\n\r\n").ok()?;
    let mut buf = Vec::new();
    s.take(4 * 1024 * 1024).read_to_end(&mut buf).ok()?;
    let split = buf.windows(4).position(|w| w == b"\r\n\r\n")?;
    let head = String::from_utf8_lossy(&buf[..split]);
    let status = head.lines().next()?;
    if !status.split_whitespace().nth(1).is_some_and(|c| c == "200") {
        return None;
    }
    Some(buf[split + 4..].to_vec())
}

// ------------------------------------------------------------------ busca

#[derive(Default)]
pub struct Scan {
    pub running: bool,
    /// (endereço "ip:porta", título do status no momento da busca).
    pub found: Vec<(String, String)>,
    pub done: bool,
    pub error: Option<String>,
}

/// Procura PCs com o painel ligado na rede do celular: a /24 do celular (ou
/// a /22 inteira numa rede de Deco, 192.168.68.0–71.255), na porta indicada.
pub fn start_scan(scan: Arc<Mutex<Scan>>, port: u16, ctx: eframe::egui::Context) {
    {
        let mut s = scan.lock().unwrap();
        if s.running {
            return;
        }
        *s = Scan { running: true, ..Default::default() };
    }
    std::thread::spawn(move || {
        let Some(me) = crate::android::local_ip() else {
            let mut s = scan.lock().unwrap();
            s.running = false;
            s.done = true;
            s.error = Some("Sem rede: conecte o celular ao Wi-Fi de casa.".into());
            ctx.request_repaint();
            return;
        };
        let o = me.octets();
        let thirds: Vec<u8> = if o[0] == 192 && o[1] == 168 && (68..=71).contains(&o[2]) { (68..=71).collect() } else { vec![o[2]] };
        let hosts: Vec<Ipv4Addr> = thirds
            .iter()
            .flat_map(|&c| (1..=254u8).map(move |d| Ipv4Addr::new(o[0], o[1], c, d)))
            .filter(|ip| *ip != me)
            .collect();

        // Conexões TCP em paralelo (300 ms cada), depois confirma pelo /api/status.
        let open = Arc::new(Mutex::new(Vec::new()));
        let threads = 48;
        let chunk = hosts.len().div_ceil(threads).max(1);
        let handles: Vec<_> = hosts
            .chunks(chunk)
            .map(|part| {
                let part = part.to_vec();
                let open = open.clone();
                std::thread::spawn(move || {
                    for ip in part {
                        let sa = SocketAddr::new(IpAddr::V4(ip), port);
                        if TcpStream::connect_timeout(&sa, Duration::from_millis(300)).is_ok() {
                            open.lock().unwrap().push(ip);
                        }
                    }
                })
            })
            .collect();
        for h in handles {
            let _ = h.join();
        }
        let mut candidates = open.lock().unwrap().clone();
        candidates.sort();
        let mut found = Vec::new();
        for ip in candidates {
            let addr = format!("{ip}:{port}");
            if let Some(v) = get_status(&addr) {
                found.push((addr, v["overall"]["title"].as_str().unwrap_or("").to_string()));
            }
        }
        let mut s = scan.lock().unwrap();
        s.running = false;
        s.done = true;
        s.found = found;
        ctx.request_repaint();
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses() {
        assert_eq!(parse_addr("192.168.68.11", 8080).as_deref(), Some("192.168.68.11:8080"));
        assert_eq!(parse_addr(" http://192.168.68.11:7878/ ", 8080).as_deref(), Some("192.168.68.11:7878"));
        assert_eq!(parse_addr("192.168.68.11:x", 8080), None);
        assert_eq!(parse_addr("", 8080), None);
    }
}
