//! Painel no celular: servidor HTTP mínimo, só leitura, para abrir o status
//! do PC no navegador de outro aparelho da rede de casa.
//!
//! Rotas: `/` (página), `/api/status` e `/api/events?hours=N`, só leitura.
//! Com o MCP pela rede ligado, também `POST /mcp` (MCP "Streamable HTTP"),
//! protegido por token, para assistentes de IA em outros computadores.

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

pub type StatusFn = Arc<dyn Fn() -> Value + Send + Sync>;

/// Servidor em execução. Para (e libera a porta) ao ser descartado.
pub struct Server {
    port: u16,
    mcp_token: Option<String>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Server {
    /// Abre a porta na hora (para o erro aparecer nas Configurações) e
    /// atende numa thread própria.
    /// `mcp_token`: com `Some`, atende `POST /mcp` para quem mandar
    /// `Authorization: Bearer <token>`.
    pub fn start(port: u16, status: StatusFn, mcp_token: Option<String>) -> std::io::Result<Self> {
        let listener = TcpListener::bind(("0.0.0.0", port))?;
        listener.set_nonblocking(true)?;
        let port = listener.local_addr()?.port();
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let token: Arc<Option<String>> = Arc::new(mcp_token.clone());
        let thread = std::thread::spawn(move || {
            while !flag.load(Ordering::Relaxed) {
                match listener.accept() {
                    // Uma thread por conexão: um probe_host demorado não trava o painel.
                    Ok((stream, _)) => {
                        let (status, token) = (status.clone(), token.clone());
                        std::thread::spawn(move || {
                            let _ = handle(stream, &status, token.as_deref());
                        });
                    }
                    Err(_) => std::thread::sleep(Duration::from_millis(100)),
                }
            }
        });
        Ok(Self { port, mcp_token, stop, thread: Some(thread) })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn mcp_token(&self) -> Option<&str> {
        self.mcp_token.as_deref()
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

fn handle(mut stream: TcpStream, status: &StatusFn, mcp_token: Option<&str>) -> std::io::Result<()> {
    // No Windows o socket aceito herda o modo não bloqueante.
    stream.set_nonblocking(false)?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;

    // Cabeçalho.
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    let head_end = loop {
        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break i + 4;
        }
        if buf.len() > 16 * 1024 {
            return Ok(());
        }
        let n = stream.read(&mut chunk)?;
        if n == 0 {
            return Ok(());
        }
        buf.extend_from_slice(&chunk[..n]);
    };
    let head = String::from_utf8_lossy(&buf[..head_end]).to_string();
    let mut lines = head.lines();
    let mut parts = lines.next().unwrap_or("").split_whitespace();
    let (method, target) = (parts.next().unwrap_or(""), parts.next().unwrap_or("/"));
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let header = |name: &str| {
        head.lines().skip(1).find_map(|l| {
            let (k, v) = l.split_once(':')?;
            k.trim().eq_ignore_ascii_case(name).then(|| v.trim().to_string())
        })
    };

    // Corpo (só no POST /mcp), até 1 MB.
    let len: usize = header("content-length").and_then(|v| v.parse().ok()).unwrap_or(0);
    let mut body = buf[head_end..].to_vec();
    if len > 1024 * 1024 {
        return respond(&mut stream, "413 Payload Too Large", "text/plain; charset=utf-8", b"Grande demais.");
    }
    while body.len() < len {
        let n = stream.read(&mut chunk)?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..n]);
    }

    if path == "/mcp" {
        let Some(token) = mcp_token else {
            return respond(&mut stream, "404 Not Found", "text/plain; charset=utf-8", "MCP pela rede desligado no app.".as_bytes());
        };
        let auth = header("authorization").unwrap_or_default();
        if auth.strip_prefix("Bearer ").map(str::trim) != Some(token) {
            return respond(&mut stream, "401 Unauthorized", "text/plain; charset=utf-8", "Token inválido.".as_bytes());
        }
        if method != "POST" {
            // Sem fluxo SSE: o servidor só responde a POSTs.
            return respond(&mut stream, "405 Method Not Allowed", "text/plain; charset=utf-8", b"Use POST.");
        }
        return match mcp(&body) {
            Some(v) => respond(&mut stream, "200 OK", "application/json", &serde_json::to_vec(&v).unwrap_or_default()),
            None => respond(&mut stream, "202 Accepted", "text/plain; charset=utf-8", b""),
        };
    }

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
    respond(&mut stream, code, ctype, &body)
}

fn respond(stream: &mut TcpStream, code: &str, ctype: &str, body: &[u8]) -> std::io::Result<()> {
    let header = format!(
        "HTTP/1.1 {code}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(header.as_bytes())?;
    stream.write_all(body)?;
    stream.flush()
}

/// Uma mensagem JSON-RPC (ou um lote) do MCP. `None` = só notificações (202).
fn mcp(body: &[u8]) -> Option<Value> {
    match serde_json::from_slice::<Value>(body) {
        Ok(Value::Array(msgs)) => {
            let replies: Vec<Value> = msgs.iter().filter_map(crate::mcp::handle).collect();
            (!replies.is_empty()).then(|| Value::from(replies))
        }
        Ok(msg) => crate::mcp::handle(&msg),
        Err(e) => Some(json!({ "jsonrpc": "2.0", "id": null, "error": { "code": -32700, "message": format!("JSON inválido: {e}") } })),
    }
}

/// Token aleatório para o MCP pela rede (32 caracteres hexadecimais).
pub fn new_token() -> String {
    use std::hash::{BuildHasher, Hasher};
    (0..2)
        .map(|i| {
            // RandomState usa sementes aleatórias do sistema a cada instância.
            let mut h = std::collections::hash_map::RandomState::new().build_hasher();
            h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos() + i);
            format!("{:016x}", h.finish())
        })
        .collect()
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

    fn post(port: u16, path: &str, auth: &str, body: &str) -> String {
        let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
        write!(s, "POST {path} HTTP/1.1\r\nHost: x\r\nAuthorization: {auth}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}", body.len()).unwrap();
        let mut out = String::new();
        s.read_to_string(&mut out).unwrap();
        out
    }

    fn status_fn(v: Value) -> StatusFn {
        Arc::new(move || v.clone())
    }

    #[test]
    fn mcp_over_http() {
        let srv = Server::start(0, status_fn(json!({})), Some("abc".into())).unwrap();
        let port = srv.port();
        let init = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"t","version":"1"}}}"#;
        assert!(post(port, "/mcp", "Bearer errado", init).starts_with("HTTP/1.1 401"));
        let ok = post(port, "/mcp", "Bearer abc", init);
        assert!(ok.starts_with("HTTP/1.1 200") && ok.contains("\"protocolVersion\":\"2025-06-18\""));
        let note = post(port, "/mcp", "Bearer abc", r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#);
        assert!(note.starts_with("HTTP/1.1 202"));
        let tools = post(port, "/mcp", "Bearer abc", r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#);
        assert!(tools.contains("probe_host"));
        drop(srv);

        // Sem token: MCP desligado.
        let srv = Server::start(0, status_fn(json!({})), None).unwrap();
        assert!(post(srv.port(), "/mcp", "Bearer abc", init).starts_with("HTTP/1.1 404"));
        assert_ne!(new_token(), new_token());
        assert_eq!(new_token().len(), 32);
    }

    #[test]
    fn routes_and_busy_port() {
        let srv = Server::start(0, status_fn(json!({"overall": {"title": "ok"}})), None).unwrap();
        let port = srv.port();
        let page = get(port, "/");
        assert!(page.starts_with("HTTP/1.1 200") && page.contains("Sem conexão com o PC"));
        let api = get(port, "/api/status");
        assert!(api.starts_with("HTTP/1.1 200") && api.ends_with(r#"{"overall":{"title":"ok"}}"#));
        assert!(get(port, "/config").starts_with("HTTP/1.1 404"));

        // Porta ocupada: erro, sem pânico.
        assert!(Server::start(port, status_fn(json!({})), None).is_err());

        // Ao parar, a porta é liberada.
        drop(srv);
        assert!(Server::start(port, status_fn(json!({})), None).is_ok());
    }
}
