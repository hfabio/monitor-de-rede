//! Histórico em SQLite: um resumo por minuto por alvo e todos os eventos.
//! Roda numa thread própria e recebe mensagens por canal.

use crate::config::app_dir;
use crate::monitor::{Event, EventKind, Stats};
use rusqlite::{params, Connection};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::{Duration, Instant};

pub enum Msg {
    Minute { t: i64, name: String, host: String, stats: Stats },
    Event(Event),
    Settings { enabled: bool, retention_days: u32 },
}

pub fn db_path() -> std::path::PathBuf {
    app_dir().join("historico.sqlite")
}

fn open() -> rusqlite::Result<Connection> {
    let _ = std::fs::create_dir_all(app_dir());
    let db = Connection::open(db_path())?;
    db.execute_batch(
        "PRAGMA journal_mode=WAL;
         CREATE TABLE IF NOT EXISTS minutos (
            t INTEGER NOT NULL, nome TEXT, host TEXT,
            enviados INTEGER, perdidos INTEGER,
            min_ms REAL, media_ms REAL, max_ms REAL, jitter_ms REAL, picos INTEGER);
         CREATE INDEX IF NOT EXISTS minutos_t ON minutos(t);
         CREATE TABLE IF NOT EXISTS eventos (
            t REAL NOT NULL, nome TEXT, host TEXT, tipo TEXT, detalhe TEXT);
         CREATE INDEX IF NOT EXISTS eventos_t ON eventos(t);",
    )?;
    Ok(db)
}

/// Carrega eventos desde `since` (segundos unix), mais novos primeiro.
pub fn load_events(since: f64) -> Vec<Event> {
    let Ok(db) = open() else { return Vec::new() };
    let Ok(mut st) = db.prepare("SELECT t, nome, host, tipo, detalhe FROM eventos WHERE t >= ?1 ORDER BY t DESC LIMIT 5000") else {
        return Vec::new();
    };
    st.query_map(params![since], |r| {
        let tipo: String = r.get(3)?;
        let ev = (r.get::<_, f64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(4)?);
        Ok(EventKind::from_db(&tipo).map(|kind| Event { t: ev.0, name: ev.1, host: ev.2, kind, detail: ev.3 }))
    })
    .map(|rows| rows.filter_map(|r| r.ok().flatten()).collect())
    .unwrap_or_default()
}

pub fn spawn(enabled: bool, retention_days: u32) -> Sender<Msg> {
    let (tx, rx) = channel();
    std::thread::spawn(move || run(rx, enabled, retention_days));
    tx
}

fn run(rx: Receiver<Msg>, mut enabled: bool, mut retention: u32) {
    let mut db = open().ok();
    let mut last_cleanup: Option<Instant> = None;
    loop {
        let msg = match rx.recv_timeout(Duration::from_secs(60)) {
            Ok(m) => Some(m),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => None,
            Err(_) => return,
        };
        if db.is_none() {
            db = open().ok();
        }
        let Some(conn) = db.as_ref() else { continue };
        match msg {
            Some(Msg::Settings { enabled: e, retention_days }) => {
                enabled = e;
                retention = retention_days;
                last_cleanup = None;
            }
            Some(Msg::Minute { t, name, host, stats: s }) if enabled => {
                let _ = conn.execute(
                    "INSERT INTO minutos VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
                    params![t, name, host, s.sent as i64, s.lost as i64, s.min, s.avg, s.max, s.jitter, s.spikes as i64],
                );
            }
            Some(Msg::Event(e)) if enabled => {
                let _ = conn.execute(
                    "INSERT INTO eventos VALUES (?1,?2,?3,?4,?5)",
                    params![e.t, e.name, e.host, e.kind.db_name(), e.detail],
                );
            }
            _ => {}
        }
        if last_cleanup.is_none_or(|t| t.elapsed() > Duration::from_secs(3600)) {
            last_cleanup = Some(Instant::now());
            let cutoff = crate::monitor::now_secs() - retention.max(1) as f64 * 86400.0;
            let _ = conn.execute("DELETE FROM minutos WHERE t < ?1", params![cutoff as i64]);
            let _ = conn.execute("DELETE FROM eventos WHERE t < ?1", params![cutoff]);
        }
    }
}
