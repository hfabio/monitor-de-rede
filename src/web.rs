//! Painel no celular: servidor HTTP mínimo, só leitura, para abrir o status
//! do PC no navegador de outro aparelho da rede de casa.
//!
//! Uma thread só, uma conexão por vez. Rotas: `/` (página), `/api/status`
//! e `/api/events?hours=N`. Nada aqui altera configuração ou dispara testes.

use crate::monitor::{now_secs, Shared};
use crate::netusage::Usage;
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

const PAGE: &str = include_str!("web.html");

type StatusFn = Box<dyn Fn() -> Value + Send>;

/// Servidor em execução. Para (e libera a porta) ao ser descartado.
pub struct Server {
    port: u16,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Server {
    /// Abre a porta na hora (para o erro aparecer nas Configurações) e
    /// atende numa thread própria.
    pub fn start(port: u16, status: StatusFn) -> std::io::Result<Self> {
        let listener = TcpListener::bind(("0.0.0.0", port))?;
        listener.set_nonblocking(true)?;
        let port = listener.local_addr()?.port();
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let thread = std::thread::spawn(move || {
            while !flag.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let _ = handle(stream, &status);
                    }
                    Err(_) => std::thread::sleep(Duration::from_millis(100)),
                }
            }
        });
        Ok(Self { port, stop, thread: Some(thread) })
    }

    pub fn port(&self) -> u16 {
        self.port
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Status do app mais uma série curta de latência por alvo (sparklines).
pub fn status(shared: &Mutex<Shared>, usage: &Mutex<Usage>) -> Value {
    let mut v = crate::status::build(shared, usage);
    let now = now_secs();
    let series: Vec<Value> = {
        let sh = shared.lock().unwrap();
        sh.targets
            .iter()
            .map(|t| {
                let pts: Vec<Value> = t
                    .samples
                    .iter()
                    .filter(|s| s.t >= now - 300.0)
                    .map(|s| json!([((s.t - now) * 10.0).round() / 10.0, s.rtt.map(|r| (r * 10.0).round() / 10.0)]))
                    .collect();
                Value::from(pts)
            })
            .collect()
    };
    if let Some(targets) = v["targets"].as_array_mut() {
        for (t, s) in targets.iter_mut().zip(series) {
            t["series"] = s;
        }
    }
    v
}

/// IPv4 do PC na rede local. Conectar um socket UDP não envia pacotes; só
/// faz o sistema escolher a interface de saída.
pub fn lan_ip() -> Option<Ipv4Addr> {
    let s = UdpSocket::bind("0.0.0.0:0").ok()?;
    s.connect("8.8.8.8:80").ok()?;
    match s.local_addr().ok()?.ip() {
        std::net::IpAddr::V4(ip) if !ip.is_unspecified() && !ip.is_loopback() => Some(ip),
        _ => None,
    }
}

fn handle(mut stream: TcpStream, status: &StatusFn) -> std::io::Result<()> {
    // No Windows o socket aceito herda o modo não bloqueante.
    stream.set_nonblocking(false)?;
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;

    // Lê só o cabeçalho (GET não tem corpo).
    let mut buf = Vec::new();
    let mut chunk = [0u8; 1024];
    while !buf.windows(4).any(|w| w == b"\r\n\r\n") && buf.len() < 8192 {
        let n = stream.read(&mut chunk)?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
    }
    let head = String::from_utf8_lossy(&buf);
    let mut parts = head.lines().next().unwrap_or("").split_whitespace();
    let (method, target) = (parts.next().unwrap_or(""), parts.next().unwrap_or("/"));
    let (path, query) = target.split_once('?').unwrap_or((target, ""));

    let (code, ctype, body) = if method != "GET" {
        ("405 Method Not Allowed", "text/plain; charset=utf-8", "Só leitura.".as_bytes().to_vec())
    } else {
        match path {
            "/" | "/index.html" => ("200 OK", "text/html; charset=utf-8", PAGE.as_bytes().to_vec()),
            "/api/status" => ("200 OK", "application/json", serde_json::to_vec(&status()).unwrap_or_default()),
            "/api/events" => ("200 OK", "application/json", serde_json::to_vec(&events(query)).unwrap_or_default()),
            _ => ("404 Not Found", "text/plain; charset=utf-8", "Não encontrado.".as_bytes().to_vec()),
        }
    };
    let header = format!(
        "HTTP/1.1 {code}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(header.as_bytes())?;
    stream.write_all(&body)?;
    stream.flush()
}

/// Eventos do histórico das últimas `hours` horas (padrão 24, máximo 30 dias).
fn events(query: &str) -> Value {
    let hours = query
        .split('&')
        .find_map(|kv| kv.strip_prefix("hours="))
        .and_then(|h| h.parse::<f64>().ok())
        .filter(|h| h.is_finite())
        .unwrap_or(24.0)
        .clamp(1.0, 720.0);
    let list: Vec<Value> = crate::history::load_events(now_secs() - hours * 3600.0).iter().take(500).map(crate::status::event_json).collect();
    json!({ "hours": hours, "events": list })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn get(port: u16, path: &str) -> String {
        let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
        write!(s, "GET {path} HTTP/1.1\r\nHost: x\r\n\r\n").unwrap();
        let mut out = String::new();
        s.read_to_string(&mut out).unwrap();
        out
    }

    #[test]
    fn routes_and_busy_port() {
        let srv = Server::start(0, Box::new(|| json!({"overall": {"title": "ok"}}))).unwrap();
        let port = srv.port();
        let page = get(port, "/");
        assert!(page.starts_with("HTTP/1.1 200") && page.contains("Sem conexão com o PC"));
        let api = get(port, "/api/status");
        assert!(api.starts_with("HTTP/1.1 200") && api.ends_with(r#"{"overall":{"title":"ok"}}"#));
        assert!(get(port, "/config").starts_with("HTTP/1.1 404"));

        // Porta ocupada: erro, sem pânico.
        assert!(Server::start(port, Box::new(|| json!({}))).is_err());

        // Ao parar, a porta é liberada.
        drop(srv);
        assert!(Server::start(port, Box::new(|| json!({}))).is_ok());
    }
}
