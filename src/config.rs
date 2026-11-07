use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Target {
    pub name: String,
    /// IP, domínio, `gateway` (roteador detectado automaticamente),
    /// `https://host/...` ou `host:porta` (medido por conexão TCP).
    pub host: String,
    pub enabled: bool,
}

impl Target {
    pub fn new(name: &str, host: &str) -> Self {
        Self { name: name.into(), host: host.into(), enabled: true }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Config {
    pub targets: Vec<Target>,
    /// Intervalo entre medições de cada alvo.
    pub interval_ms: u64,
    /// Tempo máximo de espera por resposta.
    pub timeout_ms: u64,
    /// Acima disso a medição vira evento de "pico".
    pub spike_ms: u64,
    /// Perda (% em 1 minuto) que gera evento de "perda de pacotes".
    pub loss_alert_pct: f64,
    /// Guarda resumos por minuto e eventos em SQLite.
    pub history_enabled: bool,
    pub retention_days: u32,
    /// Painel no celular: servidor HTTP só leitura na rede de casa.
    pub web_enabled: bool,
    pub web_port: u16,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            targets: vec![
                Target::new("Roteador", "gateway"),
                Target::new("Google DNS", "8.8.8.8"),
                Target::new("Cloudflare DNS", "1.1.1.1"),
            ],
            interval_ms: 1000,
            timeout_ms: 2000,
            spike_ms: 150,
            loss_alert_pct: 2.0,
            history_enabled: true,
            retention_days: 30,
            web_enabled: true,
            web_port: 8080,
        }
    }
}

pub fn app_dir() -> PathBuf {
    dirs::data_dir().unwrap_or_else(|| PathBuf::from(".")).join("MonitorRede")
}

fn config_path() -> PathBuf {
    app_dir().join("config.json")
}

impl Config {
    pub fn load() -> Self {
        std::fs::read_to_string(config_path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(app_dir())?;
        let json = serde_json::to_string_pretty(self).expect("config serializa");
        std::fs::write(config_path(), json)
    }
}

/// Sugestões da tela de configurações: (grupo, nome, host).
pub const SUGGESTIONS: &[(&str, &str, &str)] = &[
    ("Jogos", "Fortnite", AWS_SP),
    ("Jogos", "League of Legends", AWS_SP),
    ("Jogos", "CS2", AWS_SP),
    ("Jogos", "Valorant", AWS_SP),
    ("Jogos", "Free Fire", AWS_SP),
    ("Apps", "Teams", "https://teams.microsoft.com"),
    ("Apps", "Zoom", "https://zoom.us"),
    ("Apps", "WhatsApp", "https://web.whatsapp.com"),
    ("Apps", "Netflix", "https://www.netflix.com"),
    ("Apps", "YouTube", "https://www.youtube.com"),
    ("Apps", "Instagram", "https://www.instagram.com"),
    ("Apps", "TikTok", "https://www.tiktok.com"),
];

/// Servidores BR dos jogos ficam em São Paulo; esse endpoint da AWS São Paulo
/// é uma boa aproximação da latência até eles.
pub const AWS_SP: &str = "https://dynamodb.sa-east-1.amazonaws.com/ping";
