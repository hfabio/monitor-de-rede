//! Avaliação em linguagem simples: usada pela tela Início e pelo servidor MCP.

use crate::monitor::Stats;
use serde::Serialize;

/// Limites da legenda da tela Início.
pub const GOOD_MS: f64 = 80.0;
pub const BAD_MS: f64 = 1000.0;
pub const BAD_LOSS_PCT: f64 = 5.0;

#[derive(Clone, Copy, PartialEq, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Waiting,
    Good,
    Slow,
    Bad,
}

/// Situação de um endereço no último minuto: (nível, rótulo).
pub fn target_status(recent: &Stats, has_error: bool, in_outage: bool) -> (Level, &'static str) {
    if recent.sent == 0 {
        (Level::Waiting, "Aguardando…")
    } else if has_error && !recent.has_replies() {
        (Level::Bad, "Endereço não encontrado")
    } else if in_outage || !recent.has_replies() {
        (Level::Bad, "Sem resposta")
    } else if recent.loss_pct() > BAD_LOSS_PCT {
        (Level::Bad, "Perdendo pacotes")
    } else if recent.avg > BAD_MS {
        (Level::Bad, "Muito lento")
    } else if recent.avg > GOOD_MS {
        (Level::Slow, "Um pouco lento")
    } else {
        (Level::Good, "Funcionando")
    }
}

/// O mínimo que o resumo geral precisa saber de cada endereço.
pub struct View<'a> {
    pub name: &'a str,
    pub local: bool,
    pub level: Level,
    pub has_replies: bool,
    pub in_outage: bool,
    /// Nunca resolveu: erro de configuração, não problema de rede.
    pub config_error: bool,
}

/// Resumo geral em linguagem comum: (nível, título, frase).
pub fn overall(views: &[View]) -> (Level, &'static str, String) {
    let st: Vec<&View> = views.iter().filter(|v| !v.config_error).collect();
    if st.is_empty() {
        return (Level::Waiting, "Nenhum endereço ativo", "Adicione endereços em Configurações.".into());
    }
    if st.iter().all(|v| v.level == Level::Waiting) {
        return (Level::Waiting, "Verificando a conexão…", "Os primeiros resultados aparecem em alguns segundos.".into());
    }
    let net: Vec<&&View> = st.iter().filter(|v| !v.local).collect();
    let net_down = !net.is_empty() && net.iter().all(|v| v.level == Level::Bad && (v.in_outage || !v.has_replies));

    if let Some(v) = st.iter().find(|v| v.local && v.level == Level::Bad) {
        return (Level::Bad, "Problema na rede da casa", format!("\"{}\" não está respondendo bem. Verifique o cabo, o Deco ou o roteador.", v.name));
    }
    if net_down {
        return (Level::Bad, "Sem internet", "A rede da casa funciona, mas a internet caiu. Provavelmente é a operadora.".into());
    }
    let net_bad: Vec<&&&View> = net.iter().filter(|v| v.level == Level::Bad).collect();
    if let Some(v) = net_bad.first() {
        if net_bad.len() * 2 > net.len() {
            return (Level::Bad, "Internet instável", "Vários endereços com problemas. Chamadas de vídeo podem travar ou cair.".into());
        }
        return (Level::Slow, "Um endereço com problemas", format!("\"{}\" não está respondendo bem; o resto da internet está normal.", v.name));
    }
    if st.iter().any(|v| v.level == Level::Slow) {
        return (Level::Slow, "Internet um pouco lenta", "Dá para usar, mas chamadas de vídeo podem travar um pouco.".into());
    }
    (Level::Good, "Sua internet está boa", "Pode atender tranquila. Se algo mudar, esta tela avisa.".into())
}
