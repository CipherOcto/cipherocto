use super::*;

const MINIMAL: &str = r#"
name = "default"
"#;

#[test]
fn from_toml_parses_minimal() {
    let cfg = WhatsAppRuntimeConfig::from_toml(MINIMAL.as_bytes()).unwrap();
    assert_eq!(cfg.name, "default");
}

#[test]
fn defaults_apply() {
    let cfg = WhatsAppRuntimeConfig::from_toml(MINIMAL.as_bytes()).unwrap();
    assert_eq!(cfg.data_dir, PathBuf::from("/var/lib/octo/whatsapp"));
    assert_eq!(cfg.log_dir, PathBuf::from("/var/log/octo/whatsapp"));
    assert_eq!(cfg.socket_dir, PathBuf::from("/run/octo/whatsapp"));
}

#[test]
fn override_paths() {
    let cfg = WhatsAppRuntimeConfig::from_toml(
        br#"
name = "alice"
data_dir = "/srv/whatsapp/alice/data"
log_dir  = "/srv/whatsapp/alice/log"
socket_dir = "/run/user/1000"
"#,
    )
    .unwrap();
    assert_eq!(cfg.name, "alice");
    assert_eq!(cfg.data_dir, PathBuf::from("/srv/whatsapp/alice/data"));
    assert_eq!(cfg.log_dir, PathBuf::from("/srv/whatsapp/alice/log"));
    assert_eq!(cfg.socket_dir, PathBuf::from("/run/user/1000"));
}

#[test]
fn socket_path_uses_name() {
    let cfg = WhatsAppRuntimeConfig::from_toml(MINIMAL.as_bytes()).unwrap();
    assert_eq!(
        cfg.socket_path(),
        PathBuf::from("/run/octo/whatsapp/octo-whatsapp-default.sock"),
    );
}

#[test]
fn from_path_reads_file() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("config.toml");
    std::fs::write(&p, MINIMAL).unwrap();
    let cfg = WhatsAppRuntimeConfig::from_path(&p).unwrap();
    assert_eq!(cfg.name, "default");
}
