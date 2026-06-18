//! Tests for TelegramConfig.
//! Mission AC line 136: "Config: mode, bot_token, api_id+api_hash+phone, data_dir, groups, webhook_port, password, features"

use octo_adapter_telegram::TelegramConfig;

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
