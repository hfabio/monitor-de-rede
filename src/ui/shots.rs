//! Modo de depuração: com `MONITOR_REDE_SHOTS=<pasta>`, o app espera coletar
//! dados, passa por todas as telas, salva um print de cada (.ppm) e fecha.

use super::*;
use std::path::PathBuf;
use std::time::Instant;

const WARMUP_SECS: f64 = 20.0;
const STEP_SECS: f64 = 1.5;

pub struct Shots {
    dir: PathBuf,
    start: Instant,
    step: usize,
    requested: bool,
    /// Teste da bandeja: esconde a janela e reabre como o clique no ícone faz.
    hidden_at: Option<Instant>,
    reopened_shot: bool,
}

const STEPS: [(&str, View, Chart); 7] = [
    ("1-inicio", View::Home, Chart::Latency),
    ("2-monitor-latencia", View::Tab(Tab::Monitor), Chart::Latency),
    ("3-monitor-jitter", View::Tab(Tab::Monitor), Chart::Jitter),
    ("4-monitor-perda", View::Tab(Tab::Monitor), Chart::Loss),
    ("5-uso", View::Tab(Tab::Usage), Chart::Latency),
    ("6-eventos", View::Tab(Tab::Events), Chart::Latency),
    ("7-configuracoes", View::Tab(Tab::Settings), Chart::Latency),
];

impl Shots {
    pub fn from_env() -> Option<Self> {
        let dir = PathBuf::from(std::env::var_os("MONITOR_REDE_SHOTS")?);
        let _ = std::fs::create_dir_all(&dir);
        Some(Self { dir, start: Instant::now(), step: 0, requested: false, hidden_at: None, reopened_shot: false })
    }
}

impl App {
    pub(super) fn drive_shots(&mut self, ctx: &egui::Context) {
        let Some(s) = self.shots.as_mut() else { return };
        ctx.request_repaint_after(Duration::from_millis(100));
        let t = s.start.elapsed().as_secs_f64() - WARMUP_SECS;
        if t < 0.0 {
            return;
        }
        let step = (t / STEP_SECS) as usize;
        if step >= STEPS.len() {
            if self.tray.is_none() {
                std::process::exit(0);
            }
            // Esconde, e 2 s depois outra thread reabre (como o clique no ícone).
            let s = self.shots.as_mut().unwrap();
            match s.hidden_at {
                None => {
                    s.hidden_at = Some(Instant::now());
                    self.view = View::Home;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                    let c = ctx.clone();
                    std::thread::spawn(move || {
                        std::thread::sleep(Duration::from_secs(2));
                        crate::tray::wake(&c);
                    });
                }
                Some(h) if h.elapsed().as_secs_f64() > 4.0 && !s.reopened_shot => {
                    s.reopened_shot = true;
                    let path = s.dir.join("8-reaberto.ppm");
                    ctx.request_screenshot(move |img| {
                        let [w, h] = img.size;
                        let mut data = format!("P6\n{w} {h}\n255\n").into_bytes();
                        for p in &img.pixels {
                            data.extend_from_slice(&[p.r(), p.g(), p.b()]);
                        }
                        let _ = std::fs::write(path, data);
                    });
                }
                Some(h) if h.elapsed().as_secs_f64() > 6.0 => std::process::exit(0),
                _ => {}
            }
            return;
        }
        if step != s.step {
            s.step = step;
            s.requested = false;
        }
        let (name, view, chart) = STEPS[step];
        self.view = view;
        self.chart = chart;
        let s = self.shots.as_mut().unwrap();
        // Tira o print na metade do passo, depois de a tela já ter sido desenhada.
        if !s.requested && t - step as f64 * STEP_SECS > STEP_SECS / 2.0 {
            s.requested = true;
            let path = s.dir.join(format!("{name}.ppm"));
            ctx.request_screenshot(move |img| {
                let [w, h] = img.size;
                let mut data = format!("P6\n{w} {h}\n255\n").into_bytes();
                for p in &img.pixels {
                    data.extend_from_slice(&[p.r(), p.g(), p.b()]);
                }
                let _ = std::fs::write(path, data);
            });
        }
    }
}
