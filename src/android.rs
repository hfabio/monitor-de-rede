//! Versão Android: entrada da NativeActivity, pasta de dados, roteador padrão
//! (pelo ConnectivityManager, via JNI) e margens das barras do sistema.
//!
//! Mede só com o app aberto: não há serviço em segundo plano.

use eframe::egui;
use jni::objects::{JObject, JString};
use jni::{jni_sig, jni_str, JavaVM};
use std::net::Ipv4Addr;
use android_activity::AndroidApp;
use std::sync::OnceLock;

static APP: OnceLock<AndroidApp> = OnceLock::new();

#[no_mangle]
fn android_main(app: AndroidApp) {
    // O winit só cria um loop de eventos por processo. Se a Activity for
    // recriada com o processo ainda vivo, encerra para o Android abrir limpo.
    if APP.set(app.clone()).is_err() {
        std::process::exit(0);
    }
    if let Some(dir) = app.internal_data_path() {
        crate::config::set_data_dir(dir);
    }
    let options = eframe::NativeOptions { android_app: Some(app), ..Default::default() };
    if let Err(e) = eframe::run_native("Monitor de Rede", options, Box::new(|cc| Ok(Box::new(crate::mobile::App::new(cc))))) {
        eprintln!("Monitor de Rede: {e}");
    }
    std::process::exit(0);
}

/// Espaço (em pontos) ocupado pela barra de status no topo e pela barra de
/// navegação embaixo, que ficam por cima da janela da NativeActivity.
pub fn insets(ctx: &egui::Context) -> (f32, f32) {
    let Some(app) = APP.get() else { return (0.0, 0.0) };
    let r = app.content_rect();
    let ppp = ctx.pixels_per_point();
    let screen_h = ctx.content_rect().height() * ppp;
    if r.bottom <= r.top {
        return (0.0, 0.0);
    }
    let top = r.top.max(0) as f32 / ppp;
    let bottom = (screen_h - r.bottom as f32).max(0.0) / ppp;
    (top, bottom)
}

/// Roteador padrão: o gateway informado pelo Android para a rede ativa. Se
/// não der para consultar e o celular estiver numa rede de Deco da TP-Link
/// (192.168.68.0/22), usa o endereço padrão do Deco principal.
pub fn default_gateway() -> Option<Ipv4Addr> {
    system_gateway().or_else(|| {
        let ip = local_ip()?;
        let o = ip.octets();
        (o[0] == 192 && o[1] == 168 && (68..=71).contains(&o[2])).then_some(Ipv4Addr::new(192, 168, 68, 1))
    })
}

/// IPv4 do celular na rede local. Conectar um socket UDP não envia pacotes;
/// só faz o sistema escolher a interface de saída.
pub fn local_ip() -> Option<Ipv4Addr> {
    let s = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    s.connect("8.8.8.8:80").ok()?;
    match s.local_addr().ok()?.ip() {
        std::net::IpAddr::V4(ip) if !ip.is_unspecified() && !ip.is_loopback() => Some(ip),
        _ => None,
    }
}

/// Gateway da rota padrão da rede ativa (Wi-Fi, normalmente), pelo
/// ConnectivityManager (precisa de ACCESS_NETWORK_STATE).
fn system_gateway() -> Option<Ipv4Addr> {
    let app = APP.get()?;
    // SAFETY: ponteiros fornecidos pela android-activity, válidos enquanto o app roda.
    let vm = unsafe { JavaVM::from_raw(app.vm_as_ptr().cast()) };
    let activity = app.activity_as_ptr();
    vm.attach_current_thread(|env| -> jni::errors::Result<Option<Ipv4Addr>> {
        let activity = unsafe { JObject::from_raw(env, activity.cast()) };
        let name = env.new_string("connectivity")?;
        let cm = env
            .call_method(&activity, jni_str!("getSystemService"), jni_sig!((name: JString) -> java.lang.Object), &[(&name).into()])?
            .l()?;
        if cm.is_null() {
            return Ok(None);
        }
        let net = env.call_method(&cm, jni_str!("getActiveNetwork"), jni_sig!(() -> android.net.Network), &[])?.l()?;
        if net.is_null() {
            return Ok(None);
        }
        let lp = env
            .call_method(&cm, jni_str!("getLinkProperties"), jni_sig!((net: android.net.Network) -> android.net.LinkProperties), &[(&net).into()])?
            .l()?;
        if lp.is_null() {
            return Ok(None);
        }
        let routes = env.call_method(&lp, jni_str!("getRoutes"), jni_sig!(() -> java.util.List), &[])?.l()?;
        let n = env.call_method(&routes, jni_str!("size"), jni_sig!(() -> jint), &[])?.i()?;
        for i in 0..n {
            let route = env.call_method(&routes, jni_str!("get"), jni_sig!((i: jint) -> java.lang.Object), &[i.into()])?.l()?;
            let default = env.call_method(&route, jni_str!("isDefaultRoute"), jni_sig!(() -> jboolean), &[])?.z()?;
            let has_gw = env.call_method(&route, jni_str!("hasGateway"), jni_sig!(() -> jboolean), &[])?.z()?;
            if !(default && has_gw) {
                continue;
            }
            let gw = env.call_method(&route, jni_str!("getGateway"), jni_sig!(() -> java.net.InetAddress), &[])?.l()?;
            if gw.is_null() {
                continue;
            }
            let text = env.call_method(&gw, jni_str!("getHostAddress"), jni_sig!(() -> java.lang.String), &[])?.l()?;
            let text = env.cast_local::<JString>(text)?.try_to_string(env)?;
            if let Ok(ip) = text.parse::<Ipv4Addr>() {
                return Ok(Some(ip));
            }
        }
        Ok(None)
    })
    .unwrap_or_else(|e| {
        eprintln!("Monitor de Rede: roteador padrão: {e}");
        None
    })
}

/// Fecha o app (botão "voltar" na tela inicial). Nada fica rodando depois:
/// as medições só acontecem com o app aberto.
pub fn finish() -> ! {
    finish_activity();
    std::process::exit(0);
}

fn finish_activity() {
    let Some(app) = APP.get() else { return };
    // SAFETY: ponteiros fornecidos pela android-activity, válidos enquanto o app roda.
    let vm = unsafe { JavaVM::from_raw(app.vm_as_ptr().cast()) };
    let activity = app.activity_as_ptr();
    let _ = vm.attach_current_thread(|env| -> jni::errors::Result<()> {
        let activity = unsafe { JObject::from_raw(env, activity.cast()) };
        env.call_method(&activity, jni_str!("finish"), jni_sig!(() -> void), &[])?;
        Ok(())
    });
}
