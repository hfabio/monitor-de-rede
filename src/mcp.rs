//! Servidor MCP (Model Context Protocol) por stdio: `monitor-rede mcp`.
//!
//! Permite que assistentes de IA (Claude, Codex, Antigravity, Cursor…) saibam
//! o que está acontecendo na rede:
//! - status ao vivo (lido de `status.json`, gravado pelo app aberto);
//! - eventos e histórico (lidos do SQLite);
//! - testes na hora de qualquer endereço (funciona mesmo com o app fechado).
//!
//! Protocolo: JSON-RPC 2.0, uma mensagem por linha em stdin/stdout.

use crate::config::Config;
use crate::history;
use crate::monitor::{compute_stats, now_secs, Sample};
use crate::probe::{resolve, Endpoint, Prober};
use crate::status::{self, event_json, iso, stats_json};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::io::{BufRead, Write};
use std::time::{Duration, Instant};

const PROTOCOLS: [&str; 4] = ["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];
/// Status mais antigo que isso = app fechado.
const STALE_SECS: f64 = 15.0;

const INSTRUCTIONS: &str = "Monitor de Rede: mede continuamente latência, jitter e perda de pacotes até vários endereços \
(roteador, DNS, sites, servidores de jogos) e o uso de banda do computador. Comece por get_status para a situação atual; \
use get_events e get_history para investigar o passado e probe_host para testar um endereço agora. \
Endereços 'local' são a rede de casa (roteador, mesh); 'internet' são externos. Perda/latência só nos externos \
com os locais bons indica problema na operadora; problema nos locais indica Wi-Fi, cabo ou roteador.";

pub fn serve() {
    let stdin = std::io::stdin();
    let mut out = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(&line) {
            Ok(msg) => handle(&msg),
            Err(e) => Some(error(Value::Null, -32700, &format!("JSON inválido: {e}"))),
        };
        if let Some(r) = reply {
            let _ = writeln!(out, "{r}");
            let _ = out.flush();
        }
    }
}

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn ok(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

/// Responde a uma mensagem. Notificações (sem `id`) não têm resposta.
pub fn handle(msg: &Value) -> Option<Value> {
    let id = msg.get("id").cloned()?;
    let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
    let params = msg.get("params").cloned().unwrap_or(json!({}));
    Some(match method {
        "initialize" => {
            let asked = params.get("protocolVersion").and_then(Value::as_str).unwrap_or("");
            let version = if PROTOCOLS.contains(&asked) { asked } else { PROTOCOLS[0] };
            ok(
                id,
                json!({
                    "protocolVersion": version,
                    "capabilities": { "tools": {} },
                    "serverInfo": { "name": "monitor-rede", "title": format!("Monitor de Rede ({})", status::pc_name()), "version": env!("CARGO_PKG_VERSION") },
                    "instructions": format!("{INSTRUCTIONS} Este servidor mede a rede do computador \"{}\"; se houver outros servidores monitor-rede, cada um é um computador diferente.", status::pc_name()),
                }),
            )
        }
        "ping" => ok(id, json!({})),
        "tools/list" => ok(id, json!({ "tools": tools() })),
        "tools/call" => {
            let name = params.get("name").and_then(Value::as_str).unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            match call(name, &args) {
                Some(Ok(v)) => ok(id, json!({
                    "content": [{ "type": "text", "text": serde_json::to_string_pretty(&v).unwrap_or_default() }],
                    "structuredContent": v,
                })),
                Some(Err(e)) => ok(id, json!({ "content": [{ "type": "text", "text": e }], "isError": true })),
                None => error(id, -32602, &format!("ferramenta desconhecida: {name}")),
            }
        }
        _ => error(id, -32601, &format!("método não suportado: {method}")),
    })
}

fn tools() -> Value {
    let ro = json!({ "readOnlyHint": true, "openWorldHint": false });
    json!([
        {
            "name": "get_status",
            "title": "Status atual da rede",
            "description": "Current network health from the running Monitor de Rede app: overall verdict in plain Portuguese, \
per-target latency/jitter/loss for the last 1 and 5 minutes, current download/upload and top apps by bandwidth, and the \
10 most recent events. Returns app_running=false if the app is closed (use probe_host then).",
            "inputSchema": { "type": "object", "properties": {} },
            "annotations": ro,
        },
        {
            "name": "get_events",
            "title": "Eventos da rede",
            "description": "Recorded network events, newest first: outage (3+ consecutive probes without reply), recovered \
(with downtime), spike (latency above threshold), loss (packet loss above threshold within a minute) and dns (resolution \
errors). Read from the local history database.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "hours": { "type": "number", "description": "How far back to look. Default 24, max 720." },
                    "types": {
                        "type": "array",
                        "items": { "type": "string", "enum": ["queda", "voltou", "pico", "perda", "dns"] },
                        "description": "Filter by type (queda=outage, voltou=recovered, pico=spike, perda=loss, dns=DNS error)."
                    },
                    "target": { "type": "string", "description": "Filter by target name or address." },
                    "limit": { "type": "integer", "description": "Max events. Default 50, max 500." }
                }
            },
            "annotations": ro,
        },
        {
            "name": "get_history",
            "title": "Histórico por endereço",
            "description": "Latency/loss history from per-minute summaries: per-target totals for the period (loss %, average \
and worst latency, jitter, spikes), the worst minutes, and a time series of up to 60 points per target. Use it to answer \
'how was the connection this morning?' or to correlate with a complaint time.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "hours": { "type": "number", "description": "How far back. Default 1, max 720." },
                    "target": { "type": "string", "description": "Only this target (name or address)." }
                }
            },
            "annotations": ro,
        },
        {
            "name": "probe_host",
            "title": "Testar um endereço agora",
            "description": "Measure an address right now and return latency/jitter/loss stats. Accepts an IP, a domain (ICMP \
ping), 'gateway' (default router), 'https://host/...' or 'host:port' (TCP connect time, for servers that block ping). \
Works even if the app is closed.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "address": { "type": "string", "description": "IP, domain, gateway, https://host or host:port." },
                    "count": { "type": "integer", "description": "Number of probes. Default 10, max 60." },
                    "interval_ms": { "type": "integer", "description": "Interval between probes. Default 500, min 200." },
                    "timeout_ms": { "type": "integer", "description": "Per-probe timeout. Default 2000." }
                },
                "required": ["address"]
            },
            "annotations": { "readOnlyHint": true, "openWorldHint": true },
        },
        {
            "name": "list_targets",
            "title": "Endereços monitorados",
            "description": "Configured targets and test settings (interval, timeout, spike and loss thresholds, history retention).",
            "inputSchema": { "type": "object", "properties": {} },
            "annotations": ro,
        }
    ])
}

fn call(name: &str, args: &Value) -> Option<Result<Value, String>> {
    Some(match name {
        "get_status" => get_status(),
        "get_events" => get_events(args),
        "get_history" => get_history(args),
        "probe_host" => probe_host(args),
        "list_targets" => list_targets(),
        _ => return None,
    })
}

fn num(args: &Value, key: &str, default: f64, min: f64, max: f64) -> f64 {
    args.get(key).and_then(Value::as_f64).unwrap_or(default).clamp(min, max)
}

fn get_status() -> Result<Value, String> {
    let text = std::fs::read_to_string(status::path()).ok();
    let v: Option<Value> = text.and_then(|t| serde_json::from_str(&t).ok());
    let Some(mut v) = v else {
        return Ok(json!({
            "app_running": false,
            "message": "O Monitor de Rede não está aberto (nenhum status encontrado). Abra o app para monitoramento contínuo, ou use probe_host para testar agora.",
        }));
    };
    let age = now_secs() - v.get("updated_unix").and_then(Value::as_f64).unwrap_or(0.0);
    let running = age <= STALE_SECS;
    v["app_running"] = json!(running);
    if !running {
        v["message"] = json!(format!(
            "O Monitor de Rede parece fechado: o último status é de {} (há {:.0} min). Os dados abaixo são antigos; use probe_host para testar agora.",
            v.get("updated_at").and_then(Value::as_str).unwrap_or("?"),
            age / 60.0
        ));
    }
    Ok(v)
}

fn get_events(args: &Value) -> Result<Value, String> {
    let hours = num(args, "hours", 24.0, 0.01, 720.0);
    let limit = num(args, "limit", 50.0, 1.0, 500.0) as usize;
    let types: Vec<String> = args
        .get("types")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(|t| t.as_str().map(str::to_lowercase)).collect())
        .unwrap_or_default();
    let target = args.get("target").and_then(Value::as_str).map(str::to_lowercase);

    let all = history::load_events(now_secs() - hours * 3600.0);
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    let filtered: Vec<Value> = all
        .iter()
        .filter(|e| types.is_empty() || types.iter().any(|t| t == e.kind.db_name()))
        .filter(|e| target.as_ref().is_none_or(|t| e.name.to_lowercase() == *t || e.host.to_lowercase() == *t))
        .inspect(|e| *counts.entry(e.kind.db_name()).or_default() += 1)
        .take(limit)
        .map(event_json)
        .collect();
    Ok(json!({
        "period_hours": hours,
        "counts_by_type": counts,
        "events": filtered,
        "note": if all.is_empty() { "Nenhum evento no período (ou o histórico está desativado nas configurações)." } else { "" },
    }))
}

fn get_history(args: &Value) -> Result<Value, String> {
    let hours = num(args, "hours", 1.0, 0.02, 720.0);
    let target = args.get("target").and_then(Value::as_str);
    let since = (now_secs() - hours * 3600.0) as i64;
    let rows = history::load_minutes(since, target).map_err(|e| format!("Não consegui ler o histórico: {e}"))?;
    if rows.is_empty() {
        return Ok(json!({
            "period_hours": hours,
            "targets": [],
            "note": "Sem dados no período. O app precisa estar aberto com 'Guardar histórico' ligado; o primeiro resumo é gravado após 1 minuto.",
        }));
    }

    let minutes = (hours * 60.0).ceil() as i64;
    let bucket = (minutes as f64 / 60.0).ceil().max(1.0) as i64;
    let mut by_target: BTreeMap<String, Vec<&history::MinuteRow>> = BTreeMap::new();
    for r in &rows {
        by_target.entry(r.name.clone()).or_default().push(r);
    }

    let targets: Vec<Value> = by_target
        .iter()
        .map(|(name, rs)| {
            let sent: i64 = rs.iter().map(|r| r.sent).sum();
            let lost: i64 = rs.iter().map(|r| r.lost).sum();
            let replies: i64 = rs.iter().map(|r| r.sent - r.lost).sum();
            let avg = if replies > 0 { rs.iter().map(|r| r.avg * (r.sent - r.lost) as f64).sum::<f64>() / replies as f64 } else { 0.0 };
            let max = rs.iter().filter(|r| r.sent > r.lost).map(|r| r.max).fold(0.0, f64::max);
            let best = rs.iter().filter(|r| r.sent > r.lost).map(|r| r.min).fold(f64::MAX, f64::min);
            let jitter = rs.iter().map(|r| r.jitter).sum::<f64>() / rs.len() as f64;
            let mut worst: Vec<&&history::MinuteRow> = rs.iter().filter(|r| r.lost > 0 || r.max > 150.0).collect();
            worst.sort_by(|a, b| (b.lost, b.max as i64).cmp(&(a.lost, a.max as i64)));
            let worst: Vec<Value> = worst
                .iter()
                .take(5)
                .map(|r| json!({ "minute": iso(r.t as f64), "lost": r.lost, "sent": r.sent, "max_ms": (r.max * 10.0).round() / 10.0 }))
                .collect();

            // Série com no máximo ~60 pontos: agrupa minutos em blocos.
            let mut series: BTreeMap<i64, (i64, i64, f64, i64, f64)> = BTreeMap::new();
            for r in rs {
                let b = r.t / 60 / bucket * bucket * 60;
                let e = series.entry(b).or_insert((0, 0, 0.0, 0, 0.0));
                e.0 += r.sent;
                e.1 += r.lost;
                e.2 += r.avg * (r.sent - r.lost) as f64;
                e.3 += r.sent - r.lost;
                e.4 = e.4.max(r.max);
            }
            let series: Vec<Value> = series
                .iter()
                .map(|(t, (s, l, sum, n, mx))| {
                    json!({
                        "time": iso(*t as f64),
                        "loss_pct": if *s > 0 { (*l as f64 * 1000.0 / *s as f64).round() / 10.0 } else { 0.0 },
                        "avg_ms": if *n > 0 { Some((sum / *n as f64 * 10.0).round() / 10.0) } else { None },
                        "max_ms": (mx * 10.0).round() / 10.0,
                    })
                })
                .collect();

            json!({
                "name": name,
                "address": rs[0].host,
                "minutes_with_data": rs.len(),
                "sent": sent,
                "lost": lost,
                "loss_pct": if sent > 0 { (lost as f64 * 1000.0 / sent as f64).round() / 10.0 } else { 0.0 },
                "avg_ms": (avg * 10.0).round() / 10.0,
                "best_ms": (best < f64::MAX).then(|| (best * 10.0).round() / 10.0),
                "worst_ms": (max * 10.0).round() / 10.0,
                "avg_jitter_ms": (jitter * 10.0).round() / 10.0,
                "spikes": rs.iter().map(|r| r.spikes).sum::<i64>(),
                "worst_minutes": worst,
                "series_bucket_minutes": bucket,
                "series": series,
            })
        })
        .collect();

    Ok(json!({ "period_hours": hours, "from": iso(since as f64), "to": iso(now_secs()), "targets": targets }))
}

fn probe_host(args: &Value) -> Result<Value, String> {
    let address = args.get("address").and_then(Value::as_str).ok_or("informe 'address'")?.trim().to_string();
    if address.is_empty() {
        return Err("informe 'address'".into());
    }
    let count = num(args, "count", 10.0, 1.0, 60.0) as usize;
    let interval = Duration::from_millis(num(args, "interval_ms", 500.0, 200.0, 5000.0) as u64);
    let timeout = Duration::from_millis(num(args, "timeout_ms", 2000.0, 100.0, 10_000.0) as u64);

    let ep = Endpoint::parse(&address);
    let ip = resolve(&ep.host).map_err(|e| format!("Não consegui resolver {address}: {e}"))?;
    let mut prober = Prober::new();
    let mut samples = Vec::with_capacity(count);
    for i in 0..count {
        let start = Instant::now();
        samples.push(Sample { t: now_secs(), rtt: prober.probe(&ep.method, ip, timeout) });
        if i + 1 < count {
            std::thread::sleep(interval.saturating_sub(start.elapsed()));
        }
    }
    let s = compute_stats(samples.iter(), 150.0);
    let (level, label) = crate::assess::target_status(&s, false, s.lost >= 3 && !s.has_replies());
    Ok(json!({
        "address": address,
        "resolved_ip": ip.to_string(),
        "method": ep.label(),
        "status": level,
        "status_label": label,
        "stats": stats_json(&s),
        "samples_ms": samples.iter().map(|x| x.rtt.map(|r| (r * 10.0).round() / 10.0)).collect::<Vec<_>>(),
    }))
}

fn list_targets() -> Result<Value, String> {
    let cfg = Config::load();
    let targets: Vec<Value> = cfg
        .targets
        .iter()
        .map(|t| json!({ "name": t.name, "address": t.host, "enabled": t.enabled, "method": Endpoint::parse(&t.host).label() }))
        .collect();
    Ok(json!({
        "targets": targets,
        "interval_ms": cfg.interval_ms,
        "timeout_ms": cfg.timeout_ms,
        "spike_ms": cfg.spike_ms,
        "loss_alert_pct": cfg.loss_alert_pct,
        "history_enabled": cfg.history_enabled,
        "retention_days": cfg.retention_days,
    }))
}

/// Trechos de configuração para os clientes de IA mais comuns.
pub mod setup {
    pub fn exe() -> String {
        std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_else(|_| "monitor-rede".into())
    }

    pub fn claude_code() -> String {
        format!("claude mcp add monitor-rede -- \"{}\" mcp", exe())
    }

    /// Formato `mcpServers` usado por Claude Desktop, Antigravity, Cursor e outros.
    pub fn json() -> String {
        let v = serde_json::json!({ "mcpServers": { "monitor-rede": { "command": exe(), "args": ["mcp"] } } });
        serde_json::to_string_pretty(&v).unwrap_or_default()
    }

    pub fn codex() -> String {
        format!("[mcp_servers.monitor-rede]\ncommand = '{}'\nargs = [\"mcp\"]\n", exe())
    }

    /// Nome sugerido para o servidor pela rede: `monitor-rede-<computador>`.
    pub fn lan_name() -> String {
        let pc: String = crate::status::pc_name()
            .to_lowercase()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect();
        format!("monitor-rede-{}", pc.trim_matches('-'))
    }

    /// Trechos para usar o MCP de outro computador da rede.
    pub mod lan {
        use super::lan_name;

        pub fn claude_code(url: &str, token: &str) -> String {
            format!("claude mcp add --transport http {} {url} --header \"Authorization: Bearer {token}\"", lan_name())
        }

        pub fn json(url: &str, token: &str) -> String {
            let v = serde_json::json!({ "mcpServers": { lan_name(): {
                "type": "http", "url": url, "headers": { "Authorization": format!("Bearer {token}") }
            } } });
            serde_json::to_string_pretty(&v).unwrap_or_default()
        }

        pub fn codex(url: &str, token: &str) -> String {
            format!("[mcp_servers.{}]\nurl = \"{url}\"\nhttp_headers = {{ \"Authorization\" = \"Bearer {token}\" }}\n", lan_name())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handshake_and_tools() {
        let init = handle(&json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"t","version":"1"}}})).unwrap();
        assert_eq!(init["result"]["protocolVersion"], "2025-06-18");
        assert!(handle(&json!({"jsonrpc":"2.0","method":"notifications/initialized"})).is_none());
        let list = handle(&json!({"jsonrpc":"2.0","id":2,"method":"tools/list"})).unwrap();
        let names: Vec<&str> = list["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert_eq!(names, ["get_status", "get_events", "get_history", "probe_host", "list_targets"]);
        let bad = handle(&json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"nope"}})).unwrap();
        assert_eq!(bad["error"]["code"], -32602);
        let unknown = handle(&json!({"jsonrpc":"2.0","id":4,"method":"foo"})).unwrap();
        assert_eq!(unknown["error"]["code"], -32601);
    }

    #[test]
    fn codex_toml_uses_literal_string() {
        assert!(setup::codex().contains("command = '"));
    }
}
