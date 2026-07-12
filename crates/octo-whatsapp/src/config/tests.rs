use super::*;

const MINIMAL: &str = r#"
name = "default"
"#;

#[test]
fn config_default_account_id_is_default() {
    let cfg = WhatsAppRuntimeConfig::default();
    assert_eq!(cfg.account_id, "default");
}

#[test]
fn config_default_groups_and_allowlist_are_empty() {
    let cfg = WhatsAppRuntimeConfig::default();
    assert!(cfg.groups.is_empty());
    assert!(cfg.sender_allowlist.is_empty());
}

#[test]
fn validate_rejects_empty_account_id() {
    let cfg = WhatsAppRuntimeConfig {
        account_id: String::new(),
        ..Default::default()
    };
    assert!(cfg.validate().is_err());
}

#[test]
fn adapter_config_passes_groups_and_allowlist_through() {
    use std::collections::BTreeMap;
    let mut allowlist = BTreeMap::new();
    allowlist.insert("group-a@g.us".to_string(), vec!["+15551234567".to_string()]);
    let cfg = WhatsAppRuntimeConfig {
        groups: vec!["group-a@g.us".to_string(), "group-b@g.us".to_string()],
        sender_allowlist: allowlist,
        ..Default::default()
    };
    let ac = cfg.adapter_config();
    assert_eq!(
        ac.groups,
        vec!["group-a@g.us".to_string(), "group-b@g.us".to_string()]
    );
    assert_eq!(ac.sender_allowlist.len(), 1);
    assert_eq!(
        ac.sender_allowlist.get("group-a@g.us").unwrap(),
        &vec!["+15551234567".to_string()]
    );
}

#[test]
fn adapter_config_derives_session_path_from_data_dir_and_account_id() {
    let cfg = WhatsAppRuntimeConfig {
        account_id: "work".into(),
        data_dir: PathBuf::from("/var/lib/octo/whatsapp"),
        ..Default::default()
    };
    let ac = cfg.adapter_config();
    assert_eq!(ac.session_path, "/var/lib/octo/whatsapp/work/session.db");
}

#[test]
fn adapter_config_default_account_id_uses_default_subdir() {
    let cfg = WhatsAppRuntimeConfig::default();
    let ac = cfg.adapter_config();
    assert!(
        ac.session_path.ends_with("/default/session.db"),
        "got {:?}",
        ac.session_path
    );
}

#[test]
fn adapter_config_empty_groups_and_allowlist() {
    let cfg = WhatsAppRuntimeConfig::default();
    let ac = cfg.adapter_config();
    assert!(ac.groups.is_empty());
    assert!(ac.sender_allowlist.is_empty());
    assert!(ac.ws_url.is_none());
    assert!(ac.pair_phone.is_none());
    assert!(ac.pair_code.is_none());
}

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
        events: EventsConfig::default(),
        security: SecurityConfig::default(),
        observability: Default::default(),
        rules: RulesConfig::default(),
        ..Default::default()
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
        events: EventsConfig::default(),
        security: SecurityConfig::default(),
        observability: Default::default(),
        rules: RulesConfig::default(),
        ..Default::default()
    };
    assert!(bad.validate().is_err());
}

#[test]
fn query_config_defaults_match_documented_values() {
    let q = QueryConfig::default();
    assert_eq!(q.embed_provider, "local");
    assert_eq!(q.queue_capacity, 1024);
    assert_eq!(q.batch_size, 32);
    assert_eq!(q.batch_window_ms, 50);
    assert_eq!(q.subscriber_capacity, 4096);
    assert!(q.rebuild_on_boot);
    assert!(q.model_dir.is_none());
}

#[test]
fn query_config_round_trips_through_toml() {
    let toml_str = r#"
        name = "x"
        data_dir = "/tmp/x"
        log_dir = "/tmp/x/log"
        socket_dir = "/tmp/x/sock"
        [query]
        embed_provider = "mock"
        queue_capacity = 256
        batch_size = 8
        batch_window_ms = 25
        subscriber_capacity = 1024
        rebuild_on_boot = false
    "#;
    let cfg: WhatsAppRuntimeConfig = toml::from_str(toml_str).expect("parse");
    assert_eq!(cfg.query.embed_provider, "mock");
    assert_eq!(cfg.query.queue_capacity, 256);
    assert_eq!(cfg.query.batch_size, 8);
    assert_eq!(cfg.query.batch_window_ms, 25);
    assert_eq!(cfg.query.subscriber_capacity, 1024);
    assert!(!cfg.query.rebuild_on_boot);
}

#[test]
fn query_config_defaults_when_field_omitted_in_toml() {
    let toml_str = r#"
        name = "x"
        data_dir = "/tmp/x"
        log_dir = "/tmp/x/log"
        socket_dir = "/tmp/x/sock"
    "#;
    let cfg: WhatsAppRuntimeConfig = toml::from_str(toml_str).expect("parse");
    assert_eq!(cfg.query.embed_provider, "local");
    assert_eq!(cfg.query.batch_size, 32);
    assert!(cfg.query.rebuild_on_boot);
}
