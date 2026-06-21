//! Tests for TelegramConfig.
//! Mission AC line 136: "Config: mode, bot_token, api_id+api_hash+phone, data_dir, groups, webhook_port, password, features"

use octo_adapter_telegram::{AdapterKind, TelegramConfig};

#[test]
fn test_default_config() {
    let cfg = TelegramConfig::default();
    assert_eq!(cfg.mode_str(), "bot");
    assert!(cfg.bot_token.is_none());
    assert!(cfg.api_id.is_none());
    assert!(cfg.api_hash.is_none());
    assert!(cfg.phone.is_none());
    assert!(cfg.password.is_none());
    assert!(cfg.groups.is_empty());
    assert!(cfg.webhook_port.is_none());
    assert!(cfg.data_dir.is_none());
    assert!(!cfg.features.e2e_chats);
    assert!(!cfg.features.voice_video);
    // Mission AC: default adapter_kind is Tdlib (no breaking change).
    assert_eq!(cfg.adapter_kind, AdapterKind::Tdlib);
}

#[test]
fn test_bot_mode_config_parses() {
    let yaml = r#"
mode: bot
bot_token: "123:ABC"
data_dir: "/tmp/tg"
groups: ["-100123", "-100456"]
webhook_port: 8443
"#;
    let cfg: TelegramConfig = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(cfg.mode_str(), "bot");
    assert_eq!(cfg.bot_token.as_deref(), Some("123:ABC"));
    assert_eq!(cfg.groups, vec!["-100123", "-100456"]);
    assert_eq!(cfg.webhook_port, Some(8443));
    // AdapterKind defaults to Tdlib when not specified — backward compatible.
    assert_eq!(cfg.adapter_kind, AdapterKind::Tdlib);
}

#[test]
fn test_adapter_kind_mtproto_opt_in() {
    // New opt-in path: pure-Rust MTProto adapter (RFC-0850ab-c).
    let yaml = r#"
mode: bot
bot_token: "123:ABC"
data_dir: "/tmp/tg"
adapter_kind: mtproto
"#;
    let cfg: TelegramConfig = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(cfg.adapter_kind, AdapterKind::Mtproto);
}

#[test]
fn test_adapter_kind_round_trip_json() {
    let yaml = r#"
mode: bot
bot_token: "x:y"
data_dir: "/tmp/tg"
adapter_kind: mtproto
"#;
    let cfg: TelegramConfig = serde_yaml::from_str(yaml).unwrap();
    let json = serde_json::to_string(&cfg).unwrap();
    let back: TelegramConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(back.adapter_kind, AdapterKind::Mtproto);
}

#[test]
fn test_user_mode_config_parses() {
    let yaml = r#"
mode: user
api_id: 12345
api_hash: "abcdef"
phone: "+1234567890"
data_dir: "/tmp/tg-user"
password: "2fa-secret"
features:
  e2e_chats: true
"#;
    let cfg: TelegramConfig = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(cfg.mode_str(), "user");
    assert_eq!(cfg.api_id, Some(12345));
    assert_eq!(cfg.api_hash.as_deref(), Some("abcdef"));
    assert_eq!(cfg.phone.as_deref(), Some("+1234567890"));
    assert_eq!(cfg.password.as_deref(), Some("2fa-secret"));
    assert!(cfg.features.e2e_chats);
}
