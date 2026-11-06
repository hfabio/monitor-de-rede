//! Uma thread por alvo faz as medições e guarda o histórico num estado compartilhado.

use crate::config::{Config, Target};
use crate::history::{self, Msg};
use crate::probe::{resolve, Endpoint, Prober};
use chrono::{Local, TimeZone};
use std::collections::VecDeque;
use std::net::Ipv4Addr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Histórico em memória para os gráficos.
const HISTORY_SECS: f64 = 3600.0;
/// Eventos mantidos em memória (a aba mostra as últimas 24 h).
pub const EVENTS_SECS: f64 = 86400.0;
const MAX_EVENTS: usize = 5000;
/// Falhas seguidas para considerar "queda".
const OUTAGE_AFTER: u32 = 3;

pub fn now_secs() -> f64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
}

pub fn fmt_time(t: f64) -> String {
    Local
        .timestamp_opt(t as i64, 0)
        .single()
        .map(|d| d.format("%d/%m %H:%M:%S").to_string())
        .unwrap_or_default()
}

#[derive(Clone, Copy)]
pub struct Sample {
    pub t: f64,
    pub rtt: Option<f64>,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum EventKind {
    OutageStart,
    OutageEnd,
    Spike,
    Loss,
    DnsError,
}

/// Categorias dos filtros da aba Eventos.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Category {
    Queda,
    Pico,
    Perda,
    Dns,
}

impl EventKind {
    pub fn label(self) -> &'static str {
        match self {
            EventKind::OutageStart => "Queda",
            EventKind::OutageEnd => "Voltou",
            EventKind::Spike => "Pico",
            EventKind::Loss => "Perda",
            EventKind::DnsError => "Erro DNS",
        }
    }

    pub fn category(self) -> Category {
        match self {
            EventKind::OutageStart | EventKind::OutageEnd => Category::Queda,
            EventKind::Spike => Category::Pico,
            EventKind::Loss => Category::Perda,
            EventKind::DnsError => Category::Dns,
        }
    }

    pub fn db_name(self) -> &'static str {
        match self {
            EventKind::OutageStart => "queda",
            EventKind::OutageEnd => "voltou",
            EventKind::Spike => "pico",
            EventKind::Loss => "perda",
            EventKind::DnsError => "dns",
        }
    }

    pub fn from_db(s: &str) -> Option<Self> {
        Some(match s {
            "queda" => EventKind::OutageStart,
            "voltou" => EventKind::OutageEnd,
            "pico" => EventKind::Spike,
            "perda" => EventKind::Loss,
            "dns" => EventKind::DnsError,
            _ => return None,
        })
    }
}

#[derive(Clone, Debug)]
pub struct Event {
    pub t: f64,
    pub name: String,
    pub host: String,
    pub kind: EventKind,
    pub detail: String,
}

pub struct TargetState {
    pub name: String,
    pub host: String,
    /// "ping", "HTTPS" ou "TCP".
    pub method: &'static str,
    /// IP privado (rede de casa) ou internet.
    pub local: bool,
    pub resolved: Option<Ipv4Addr>,
    pub error: Option<String>,
    pub samples: VecDeque<Sample>,
    pub total_sent: u64,
    pub total_lost: u64,
    pub outage_since: Option<f64>,
}

#[derive(Default, Clone, Copy, Debug)]
pub struct Stats {
    pub sent: usize,
    pub lost: usize,
    pub min: f64,
    pub avg: f64,
    pub max: f64,
    pub jitter: f64,
    pub last: Option<f64>,
    pub spikes: usize,
}

impl Stats {
    pub fn loss_pct(&self) -> f64 {
        if self.sent == 0 {
            0.0
        } else {
            self.lost as f64 * 100.0 / self.sent as f64
        }
    }

    pub fn has_replies(&self) -> bool {
        self.sent > self.lost
    }
}

pub fn compute_stats<'a>(samples: impl Iterator<Item = &'a Sample>, spike_ms: f64) -> Stats {
    let mut s = Stats { min: f64::MAX, ..Default::default() };
    let (mut sum, mut ok) = (0.0, 0usize);
    let (mut prev, mut jit_sum, mut jit_n) = (None::<f64>, 0.0, 0usize);
    for x in samples {
        s.sent += 1;
        s.last = x.rtt;
        match x.rtt {
            Some(r) => {
                ok += 1;
                sum += r;
                s.min = s.min.min(r);
                s.max = s.max.max(r);
                if r > spike_ms {
                    s.spikes += 1;
                }
                if let Some(p) = prev {
                    jit_sum += (r - p).abs();
                    jit_n += 1;
                }
                prev = Some(r);
            }
            None => s.lost += 1,
        }
    }
    if ok > 0 {
        s.avg = sum / ok as f64;
    } else {
        s.min = 0.0;
    }
    if jit_n > 0 {
        s.jitter = jit_sum / jit_n as f64;
    }
    s
}

pub fn is_private(ip: Ipv4Addr) -> bool {
    ip.is_private() || ip.is_loopback() || ip.is_link_local()
}

pub struct Shared {
    pub targets: Vec<TargetState>,
    /// Mais novos primeiro.
    pub events: VecDeque<Event>,
    pub started: f64,
    history: Sender<Msg>,
}

impl Shared {
    fn push_event(&mut self, ev: Event) {
        let _ = self.history.send(Msg::Event(ev.clone()));
        self.events.push_front(ev);
        let cutoff = now_secs() - EVENTS_SECS;
        while self.events.len() > MAX_EVENTS || self.events.back().is_some_and(|e| e.t < cutoff) {
            self.events.pop_back();
        }
    }
}

pub struct Monitor {
    pub shared: Arc<Mutex<Shared>>,
    stops: Vec<Arc<AtomicBool>>,
}

impl Monitor {
    pub fn new(cfg: &Config) -> Self {
        let history = history::spawn(cfg.history_enabled, cfg.retention_days);
        let events = if cfg.history_enabled {
            history::load_events(now_secs() - EVENTS_SECS).into()
        } else {
            VecDeque::new()
        };
        let mut m = Self {
            shared: Arc::new(Mutex::new(Shared { targets: Vec::new(), events, started: now_secs(), history })),
            stops: Vec::new(),
        };
        m.apply(cfg);
        m
    }

    /// Para as threads atuais e inicia novas conforme a configuração.
    /// O histórico de alvos que continuam na lista é mantido.
    pub fn apply(&mut self, cfg: &Config) {
        for s in self.stops.drain(..) {
            s.store(true, Ordering::Relaxed);
        }
        let active: Vec<Target> = cfg.targets.iter().filter(|t| t.enabled && !t.host.trim().is_empty()).cloned().collect();

        {
            let mut sh = self.shared.lock().unwrap();
            let _ = sh.history.send(Msg::Settings { enabled: cfg.history_enabled, retention_days: cfg.retention_days });
            let mut old = std::mem::take(&mut sh.targets);
            for t in &active {
                let host = t.host.trim().to_string();
                let name = if t.name.trim().is_empty() { host.clone() } else { t.name.trim().to_string() };
                if let Some(i) = old.iter().position(|o| o.host == host) {
                    let mut st = old.remove(i);
                    st.name = name;
                    sh.targets.push(st);
                } else {
                    let ep = Endpoint::parse(&host);
                    let local = ep.host == "gateway" || ep.host.parse::<Ipv4Addr>().map(is_private).unwrap_or(false);
                    sh.targets.push(TargetState {
                        name,
                        host,
                        method: ep.label(),
                        local,
                        resolved: None,
                        error: None,
                        samples: VecDeque::new(),
                        total_sent: 0,
                        total_lost: 0,
                        outage_since: None,
                    });
                }
            }
        }

        for t in active {
            let stop = Arc::new(AtomicBool::new(false));
            self.stops.push(stop.clone());
            let w = Worker {
                host: t.host.trim().to_string(),
                shared: self.shared.clone(),
                stop,
                interval: Duration::from_millis(cfg.interval_ms.max(200)),
                timeout: Duration::from_millis(cfg.timeout_ms.clamp(100, 10_000)),
                spike_ms: cfg.spike_ms as f64,
                loss_alert_pct: cfg.loss_alert_pct,
            };
            thread::spawn(move || w.run());
        }
    }
}

impl Drop for Monitor {
    fn drop(&mut self) {
        for s in &self.stops {
            s.store(true, Ordering::Relaxed);
        }
    }
}

struct Worker {
    host: String,
    shared: Arc<Mutex<Shared>>,
    stop: Arc<AtomicBool>,
    interval: Duration,
    timeout: Duration,
    spike_ms: f64,
    loss_alert_pct: f64,
}

impl Worker {
    fn stopped(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }

    fn run(self) {
        let ep = Endpoint::parse(&self.host);
        let mut prober = Prober::new();
        let mut ip: Option<Ipv4Addr> = None;
        let mut last_resolve: Option<Instant> = None;
        let mut dns_error_logged = false;
        let mut fails = 0u32;
        let mut first_fail_t = 0.0;
        let mut prev_spike = false;
        let mut minute = MinuteAgg::default();
        let mut next = Instant::now();

        while !self.stopped() {
            // Resolve no início, a cada 5 min, e a cada 10 s enquanto falhar.
            let needs_resolve = last_resolve
                .is_none_or(|t| t.elapsed() > Duration::from_secs(300) || (ip.is_none() && t.elapsed() > Duration::from_secs(10)));
            if needs_resolve {
                last_resolve = Some(Instant::now());
                let res = resolve(&ep.host);
                let mut sh = self.shared.lock().unwrap();
                let Some(idx) = sh.targets.iter().position(|t| t.host == self.host) else { return };
                match res {
                    Ok(addr) => {
                        ip = Some(addr);
                        dns_error_logged = false;
                        let st = &mut sh.targets[idx];
                        st.resolved = Some(addr);
                        st.local = is_private(addr);
                        st.error = None;
                    }
                    Err(e) => {
                        sh.targets[idx].error = Some(e.clone());
                        if !dns_error_logged {
                            dns_error_logged = true;
                            let name = sh.targets[idx].name.clone();
                            sh.push_event(Event { t: now_secs(), name, host: self.host.clone(), kind: EventKind::DnsError, detail: e });
                        }
                    }
                }
            }

            let rtt = ip.and_then(|addr| prober.probe(&ep.method, addr, self.timeout));
            if self.stopped() {
                break;
            }
            let t = now_secs();

            {
                let mut sh = self.shared.lock().unwrap();
                let Some(idx) = sh.targets.iter().position(|x| x.host == self.host) else { return };
                let mut new_events = Vec::new();
                {
                    let st = &mut sh.targets[idx];
                    st.samples.push_back(Sample { t, rtt });
                    while st.samples.front().is_some_and(|s| s.t < t - HISTORY_SECS) {
                        st.samples.pop_front();
                    }
                    st.total_sent += 1;
                    match rtt {
                        // Sem IP o problema já foi registrado como erro de DNS.
                        None if ip.is_none() => st.total_lost += 1,
                        None => {
                            st.total_lost += 1;
                            fails += 1;
                            if fails == 1 {
                                first_fail_t = t;
                            }
                            if fails == OUTAGE_AFTER {
                                st.outage_since = Some(first_fail_t);
                                new_events.push((first_fail_t, EventKind::OutageStart, "sem resposta".to_string()));
                            }
                            prev_spike = false;
                        }
                        Some(r) => {
                            if let Some(since) = st.outage_since.take() {
                                new_events.push((t, EventKind::OutageEnd, format!("ficou {:.0} s sem resposta", t - since)));
                            }
                            fails = 0;
                            let is_spike = r > self.spike_ms;
                            if is_spike && !prev_spike {
                                new_events.push((t, EventKind::Spike, format!("{r:.0} ms")));
                            }
                            prev_spike = is_spike;
                        }
                    }
                }
                let name = sh.targets[idx].name.clone();
                if let Some((m, s)) = minute.add(t, rtt, self.spike_ms) {
                    // Perda parcial no minuto (queda total já vira evento de "Queda").
                    if s.lost > 0 && s.lost < s.sent && s.loss_pct() >= self.loss_alert_pct {
                        new_events.push((
                            t,
                            EventKind::Loss,
                            format!("{} de {} pacotes perdidos no último minuto ({:.1}%)", s.lost, s.sent, s.loss_pct()).replace('.', ","),
                        ));
                    }
                    let _ = sh.history.send(Msg::Minute { t: m * 60, name: name.clone(), host: self.host.clone(), stats: s });
                }
                for (et, kind, detail) in new_events {
                    sh.push_event(Event { t: et, name: name.clone(), host: self.host.clone(), kind, detail });
                }
            }

            // Espera o próximo ciclo em pedaços curtos para responder rápido ao "parar".
            next += self.interval;
            let now = Instant::now();
            if next < now {
                next = now;
            }
            while !self.stopped() && Instant::now() < next {
                thread::sleep((next - Instant::now()).min(Duration::from_millis(200)));
            }
        }
    }
}

/// Junta as amostras de um minuto para o resumo do histórico.
#[derive(Default)]
struct MinuteAgg {
    minute: i64,
    samples: Vec<Sample>,
}

impl MinuteAgg {
    fn add(&mut self, t: f64, rtt: Option<f64>, spike_ms: f64) -> Option<(i64, Stats)> {
        let m = (t / 60.0).floor() as i64;
        let mut out = None;
        if m != self.minute {
            if !self.samples.is_empty() {
                out = Some((self.minute, compute_stats(self.samples.iter(), spike_ms)));
            }
            self.samples.clear();
            self.minute = m;
        }
        self.samples.push(Sample { t, rtt });
        out
    }
}
