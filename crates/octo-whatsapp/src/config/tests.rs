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

#[test]
fn validate_rejects_uppercase() {
    let mut cfg = WhatsAppRuntimeConfig::from_toml(MINIMAL.as_bytes()).unwrap();
    cfg.name = "Default".to_string();
    assert!(matches!(cfg.validate(), Err(ConfigError::InvalidName(_))));
}

#[test]
fn validate_rejects_path_traversal() {
    let mut cfg = WhatsAppRuntimeConfig::from_toml(MINIMAL.as_bytes()).unwrap();
    cfg.name = "../etc".to_string();
    assert!(matches!(cfg.validate(), Err(ConfigError::InvalidName(_))));
}

#[test]
fn validate_rejects_empty_name() {
    let mut cfg = WhatsAppRuntimeConfig::from_toml(MINIMAL.as_bytes()).unwrap();
    cfg.name = String::new();
    assert!(matches!(cfg.validate(), Err(ConfigError::InvalidName(_))));
}

#[test]
fn media_buffer_config_validates() {
    let cfg = WhatsAppRuntimeConfig {
        name: "x".into(),
        data_dir: std::env::temp_dir(),
        log_dir: std::env::temp_dir(),
        socket_dir: std::env::temp_dir(),
        media_buffer: MediaBufferConfig {
            max_concurrent_uploads: 4,
            root: std::env::temp_dir().join("mb"),
        },
    };
    assert!(cfg.validate().is_ok());
    let bad = WhatsAppRuntimeConfig {
        name: "x".into(),
        data_dir: std::env::temp_dir(),
        log_dir: std::env::temp_dir(),
        socket_dir: std::env::temp_dir(),
        media_buffer: MediaBufferConfig {
            max_concurrent_uploads: 0,
            root: std::env::temp_dir(),
        },
    };
    assert!(bad.validate().is_err());
}
