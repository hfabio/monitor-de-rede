//! Embute o ícone no executável do Windows.
fn main() {
    println!("cargo:rerun-if-changed=assets/app.rc");
    println!("cargo:rerun-if-changed=assets/icon.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile_for("assets/app.rc", ["monitor-rede"], embed_resource::NONE)
            .manifest_optional()
            .expect("ícone do Windows");
    }
}
