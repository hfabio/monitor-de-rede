//! Status ao vivo em JSON. O app grava `status.json` a cada 2 s, e o servidor
//! MCP (outro processo) lê esse arquivo para responder ao que está
//! acontecendo agora.

use crate::assess::{self, Level};
use crate::config::app_dir;
use crate::monitor::{compute_stats, now_secs, Event, Shared, Stats};
use crate::netusage::{PerApp, Usage};
use chrono::{Local, TimeZone};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub fn path() -> std::path::PathBuf {
    app_dir().join("status.json")
}

pub fn iso(t: f64) -> String {
    Local.timestamp_opt(t as i64, 0).single().map(|d| d.to_rfc3339()).unwrap_or_default()
}

fn round(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

pub fn stats_json(s: &Stats) -> Value {
    let has = s.has_replies();
    json!({
        "sent": s.sent,
        "lost": s.lost,
        "loss_pct": round(s.loss_pct()),
        "avg_ms": has.then(|| round(s.avg)),
        "min_ms": has.then(|| round(s.min)),
        "max_ms": has.then(|| round(s.max)),
        "jitter_ms": has.then(|| round(s.jitter)),
        "spikes": s.spikes,
    })
}

pub fn event_json(e: &Event) -> Value {
    json!({
        "time": iso(e.t),
        "type": e.kind.db_name(),
        "label": e.kind.label(),
        "target": e.name,
        "address": e.host,
        "detail": e.detail,
    })
}

/// Monta o status a partir do estado do monitor e do uso da rede.
pub fn build(shared: &Mutex<Shared>, usage: &Mutex<Usage>) -> Value {
    let now = now_secs();
    let sh = shared.lock().unwrap();
    let spike = sh.spike_ms;

    let mut targets = Vec::new();
    let mut levels = Vec::new();
    for t in &sh.targets {
        let recent = compute_stats(t.samples.iter().filter(|s| s.t >= now - 60.0), spike);
        let five = compute_stats(t.samples.iter().filter(|s| s.t >= now - 300.0), spike);
        let (level, label) = assess::target_status(&recent, t.error.is_some(), t.outage_since.is_some());
        levels.push((t.name.clone(), t.local, level, recent.has_replies(), t.outage_since.is_some(), t.error.is_some() && !recent.has_replies()));
        targets.push(json!({
            "name": t.name,
            "address": t.host,
            "method": t.method,
            "resolved_ip": t.resolved.map(|ip| ip.to_string()),
            "scope": if t.local { "local" } else { "internet" },
            "status": level,
            "status_label": label,
            "last_ms": t.samples.back().and_then(|s| s.rtt).map(round),
            "last_1min": stats_json(&recent),
            "last_5min": stats_json(&five),
            "outage_since": t.outage_since.map(iso),
            "error": t.error,
        }));
    }
    let views: Vec<assess::View> = levels
        .iter()
        .map(|(name, local, level, has, out, cfg_err)| assess::View {
            name,
            local: *local,
            level: *level,
            has_replies: *has,
            in_outage: *out,
            config_error: *cfg_err,
        })
        .collect();
    let (level, title, message): (Level, &str, String) = assess::overall(&views);
    let recent_events: Vec<Value> = sh.events.iter().take(10).map(event_json).collect();
    let started = sh.started;
    drop(sh);

    let u = usage.lock().unwrap();
    let apps: Vec<Value> = u
        .apps
        .iter()
        .take(10)
        .map(|a| {
            json!({
                "app": a.name,
                "download_mbps": (u.per_app == PerApp::Full).then(|| round(a.down_bps / 1e6)),
                "upload_mbps": (u.per_app == PerApp::Full).then(|| round(a.up_bps / 1e6)),
                "connections": a.conns,
            })
        })
        .collect();
    let per_app = match u.per_app {
        PerApp::Full => "bandwidth",
        PerApp::ConnectionsOnly => "connections_only (abra o app como administrador para ver a banda por app)",
        PerApp::Unsupported => "unsupported (só no Windows)",
    };

    json!({
        "updated_at": iso(now),
        "updated_unix": now,
        "monitoring_since": iso(started),
        "thresholds": {
            "green_up_to_ms": assess::GOOD_MS,
            "yellow_up_to_ms": assess::BAD_MS,
            "red_loss_above_pct": assess::BAD_LOSS_PCT,
            "spike_above_ms": spike,
        },
        "overall": { "status": level, "title": title, "message": message },
        "targets": targets,
        "network_usage": {
            "download_mbps": round(u.down_bps / 1e6),
            "upload_mbps": round(u.up_bps / 1e6),
            "per_app_mode": per_app,
            "top_apps": apps,
        },
        "recent_events": recent_events,
    })
}

/// Grava o status a cada 2 s (escrita atômica: arquivo temporário + rename).
pub fn spawn_writer(shared: Arc<Mutex<Shared>>, usage: Arc<Mutex<Usage>>) {
    std::thread::spawn(move || loop {
        let v = build(&shared, &usage);
        let _ = std::fs::create_dir_all(app_dir());
        let tmp = app_dir().join("status.json.tmp");
        if std::fs::write(&tmp, serde_json::to_vec_pretty(&v).unwrap_or_default()).is_ok() {
            let _ = std::fs::rename(&tmp, path());
        }
        std::thread::sleep(Duration::from_secs(2));
    });
}
