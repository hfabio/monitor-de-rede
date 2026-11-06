//! Medição de latência de um alvo.
//! - ICMP (ping) para IPs e domínios. No Windows usa IcmpSendEcho, sem
//!   precisar de administrador; em outros sistemas usa o comando `ping`.
//! - TCP para `https://…`, `http://…` e `host:porta`: mede o tempo do
//!   handshake, útil para servidores que bloqueiam ping.

use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

#[derive(Clone, Debug, PartialEq)]
pub enum Method {
    Icmp,
    Tcp(u16),
}

#[derive(Clone, Debug)]
pub struct Endpoint {
    /// Nome ou IP a resolver ("gateway" = roteador padrão).
    pub host: String,
    pub method: Method,
}

impl Endpoint {
    pub fn parse(input: &str) -> Self {
        let s = input.trim();
        let lower = s.to_ascii_lowercase();
        for (scheme, port) in [("https://", 443u16), ("http://", 80), ("tcp://", 0)] {
            if let Some(rest) = lower.strip_prefix(scheme) {
                let hostport = rest.split(['/', '?', '#']).next().unwrap_or("");
                let (host, port) = split_port(hostport).unwrap_or((hostport.to_string(), port));
                let port = if port == 0 { 443 } else { port };
                return Self { host, method: Method::Tcp(port) };
            }
        }
        if let Some((host, port)) = split_port(&lower) {
            return Self { host, method: Method::Tcp(port) };
        }
        Self { host: lower, method: Method::Icmp }
    }

    pub fn label(&self) -> &'static str {
        match self.method {
            Method::Icmp => "ping",
            Method::Tcp(443) => "HTTPS",
            Method::Tcp(_) => "TCP",
        }
    }
}

/// "host:porta" (IPv6 não é suportado).
fn split_port(s: &str) -> Option<(String, u16)> {
    let (h, p) = s.rsplit_once(':')?;
    if h.contains(':') || h.is_empty() {
        return None;
    }
    Some((h.to_string(), p.parse().ok()?))
}

pub fn resolve(host: &str) -> Result<Ipv4Addr, String> {
    if host == "gateway" {
        return default_gateway().ok_or_else(|| "roteador padrão não encontrado".into());
    }
    if let Ok(ip) = host.parse::<IpAddr>() {
        return match ip {
            IpAddr::V4(v4) => Ok(v4),
            IpAddr::V6(_) => Err("IPv6 não suportado".into()),
        };
    }
    (host, 0)
        .to_socket_addrs()
        .map_err(|e| format!("DNS falhou: {e}"))?
        .find_map(|a| match a.ip() {
            IpAddr::V4(v4) => Some(v4),
            _ => None,
        })
        .ok_or_else(|| "nenhum endereço IPv4".into())
}

pub struct Prober {
    icmp: Pinger,
}

impl Prober {
    pub fn new() -> Self {
        Self { icmp: Pinger::new() }
    }

    /// Tempo de resposta em ms, ou None se não houve resposta.
    pub fn probe(&mut self, method: &Method, ip: Ipv4Addr, timeout: Duration) -> Option<f64> {
        match method {
            Method::Icmp => self.icmp.ping(ip, timeout),
            Method::Tcp(port) => tcp_ping(ip, *port, timeout),
        }
    }
}

fn tcp_ping(ip: Ipv4Addr, port: u16, timeout: Duration) -> Option<f64> {
    let start = Instant::now();
    match TcpStream::connect_timeout(&SocketAddr::new(IpAddr::V4(ip), port), timeout) {
        Ok(_) => Some(start.elapsed().as_secs_f64() * 1000.0),
        // "Conexão recusada" também é uma resposta do servidor.
        Err(e) if e.kind() == std::io::ErrorKind::ConnectionRefused => Some(start.elapsed().as_secs_f64() * 1000.0),
        Err(_) => None,
    }
}

// ------------------------------------------------------------------ Windows

#[cfg(windows)]
use win::Pinger;

#[cfg(windows)]
mod win {
    use super::*;
    use std::ffi::c_void;
    use windows_sys::Win32::Foundation::{HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::NetworkManagement::IpHelper::{IcmpCloseHandle, IcmpCreateFile, IcmpSendEcho, ICMP_ECHO_REPLY};

    pub struct Pinger {
        handle: HANDLE,
    }

    impl Pinger {
        pub fn new() -> Self {
            Self { handle: unsafe { IcmpCreateFile() } }
        }

        fn valid(&self) -> bool {
            self.handle != INVALID_HANDLE_VALUE && !self.handle.is_null()
        }

        pub fn ping(&mut self, ip: Ipv4Addr, timeout: Duration) -> Option<f64> {
            if !self.valid() {
                return None;
            }
            let payload = [0x61u8; 32];
            // u64 garante o alinhamento da struct de resposta.
            let mut reply = [0u64; 64];
            let start = Instant::now();
            let n = unsafe {
                IcmpSendEcho(
                    self.handle,
                    u32::from_ne_bytes(ip.octets()),
                    payload.as_ptr() as *const c_void,
                    payload.len() as u16,
                    std::ptr::null(),
                    reply.as_mut_ptr() as *mut c_void,
                    std::mem::size_of_val(&reply) as u32,
                    timeout.as_millis() as u32,
                )
            };
            let elapsed = start.elapsed().as_secs_f64() * 1000.0;
            if n == 0 {
                return None;
            }
            let r = unsafe { &*(reply.as_ptr() as *const ICMP_ECHO_REPLY) };
            // Status != 0: destino inalcançável, TTL expirado etc.
            (r.Status == 0).then_some(elapsed)
        }
    }

    impl Drop for Pinger {
        fn drop(&mut self) {
            if self.valid() {
                unsafe { IcmpCloseHandle(self.handle) };
            }
        }
    }

    pub fn default_gateway() -> Option<Ipv4Addr> {
        use windows_sys::Win32::NetworkManagement::IpHelper::{GetBestRoute, MIB_IPFORWARDROW};
        let mut row: MIB_IPFORWARDROW = unsafe { std::mem::zeroed() };
        let dest = u32::from_ne_bytes([8, 8, 8, 8]);
        let rc = unsafe { GetBestRoute(dest, 0, &mut row) };
        let hop = Ipv4Addr::from(row.dwForwardNextHop.to_ne_bytes());
        (rc == 0 && !hop.is_unspecified()).then_some(hop)
    }
}

#[cfg(windows)]
pub use win::default_gateway;

// ------------------------------------------------------------ macOS / Linux

#[cfg(not(windows))]
struct Pinger;

#[cfg(not(windows))]
impl Pinger {
    fn new() -> Self {
        Pinger
    }

    fn ping(&mut self, ip: Ipv4Addr, timeout: Duration) -> Option<f64> {
        let ms = timeout.as_millis().max(1);
        // macOS: -W em ms; Linux: -W em segundos.
        let wait = if cfg!(target_os = "macos") { ms.to_string() } else { ms.div_ceil(1000).to_string() };
        let out = std::process::Command::new("ping")
            .args(["-n", "-c", "1", "-W", &wait, &ip.to_string()])
            .output()
            .ok()?;
        let text = String::from_utf8_lossy(&out.stdout);
        let rest = &text[text.find("time=")? + 5..];
        let end = rest.find(|c: char| !(c.is_ascii_digit() || c == '.'))?;
        rest[..end].parse().ok()
    }
}

#[cfg(target_os = "macos")]
pub fn default_gateway() -> Option<Ipv4Addr> {
    let out = std::process::Command::new("route").args(["-n", "get", "default"]).output().ok()?;
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .find_map(|l| l.trim().strip_prefix("gateway:").and_then(|g| g.trim().parse().ok()))
}

#[cfg(all(unix, not(target_os = "macos")))]
pub fn default_gateway() -> Option<Ipv4Addr> {
    // /proc/net/route: destino 00000000 = rota padrão; gateway em hex little-endian.
    let text = std::fs::read_to_string("/proc/net/route").ok()?;
    text.lines().skip(1).find_map(|l| {
        let f: Vec<&str> = l.split_whitespace().collect();
        if f.len() > 2 && f[1] == "00000000" {
            let g = u32::from_str_radix(f[2], 16).ok()?;
            Some(Ipv4Addr::from(g.to_le_bytes()))
        } else {
            None
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_endpoints() {
        let e = Endpoint::parse("https://dynamodb.sa-east-1.amazonaws.com/ping");
        assert_eq!(e.host, "dynamodb.sa-east-1.amazonaws.com");
        assert_eq!(e.method, Method::Tcp(443));
        let e = Endpoint::parse("http://exemplo.com:8080/x");
        assert_eq!((e.host.as_str(), e.method), ("exemplo.com", Method::Tcp(8080)));
        let e = Endpoint::parse("1.2.3.4:27015");
        assert_eq!((e.host.as_str(), e.method), ("1.2.3.4", Method::Tcp(27015)));
        let e = Endpoint::parse(" 8.8.8.8 ");
        assert_eq!((e.host.as_str(), e.method), ("8.8.8.8", Method::Icmp));
        let e = Endpoint::parse("gateway");
        assert_eq!(e.method, Method::Icmp);
    }
}
