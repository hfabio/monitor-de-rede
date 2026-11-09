//! Windows: embute no executável o ícone e os dados de versão (nome do
//! produto, versão), que aparecem nas Propriedades do arquivo e são
//! conferidos na assinatura de código.
use std::env;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let num = |k: &str| env::var(k).unwrap_or_else(|_| "0".into());
    let (major, minor, patch) = (num("CARGO_PKG_VERSION_MAJOR"), num("CARGO_PKG_VERSION_MINOR"), num("CARGO_PKG_VERSION_PATCH"));
    let version = env::var("CARGO_PKG_VERSION").unwrap();
    let icon = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("assets").join("icon.ico");
    let icon = icon.display().to_string().replace('\\', "\\\\");
    let rc = format!(
        r#"1 ICON "{icon}"
1 VERSIONINFO
FILEVERSION {major},{minor},{patch},0
PRODUCTVERSION {major},{minor},{patch},0
FILEOS 0x40004
FILETYPE 0x1
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "041604B0"
    BEGIN
      VALUE "CompanyName", "hfabio"
      VALUE "FileDescription", "Monitor de Rede"
      VALUE "FileVersion", "{version}"
      VALUE "InternalName", "MonitorRede"
      VALUE "LegalCopyright", "GPL-3.0-or-later"
      VALUE "OriginalFilename", "MonitorRede.exe"
      VALUE "ProductName", "Monitor de Rede"
      VALUE "ProductVersion", "{version}"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x0416, 1200
  END
END
"#
    );
    let path = PathBuf::from(env::var("OUT_DIR").unwrap()).join("app.rc");
    std::fs::write(&path, rc).expect("app.rc");
    embed_resource::compile_for(&path, ["monitor-rede"], embed_resource::NONE)
        .manifest_optional()
        .expect("recursos do Windows");
}
