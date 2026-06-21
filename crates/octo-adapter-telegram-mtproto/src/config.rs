//! MtprotoTelegramConfig — bot/user mode, DC selection, data_dir.
//!
//! The shape mirrors `octo-adapter-telegram::TelegramConfig` so a
//! user can flip `octo.telegram.adapter = mtproto | tdlib` with no
//! other config changes. The MTProto-only fields are additive
//! (`api_layer`, `device_model`, `system_version`, `app_version`)
//! and only meaningful in the MTProto code path; the TDLib path
//! silently ignores them.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Default `api_layer` value to advertise to Telegram during the
/// `initConnection` handshake. The grammers default is 195; we
/// pin it to 197 (the May 2026 release) for stability. Override
/// per-deployment in the config file.
pub const DEFAULT_API_LAYER: i32 = 197;

/// Default `device_model` advertised in `initConnection`.
pub const DEFAULT_DEVICE_MODEL: &str = "CipherOcto";

/// Default `system_version` advertised in `initConnection`.
pub const DEFAULT_SYSTEM_VERSION: &str = "1.0";

/// Default `app_version` advertised in `initConnection`.
pub const DEFAULT_APP_VERSION: &str = "0.1.0";

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct MtprotoTelegramFeatures {
    /// Enable access to secret chats (user mode only).
    #[serde(default)]
    pub e2e_chats: bool,

    /// Enable voice/video call hooks (user mode only).
    #[serde(default)]
    pub voice_video: bool,
}

#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct MtprotoTelegramConfig {
    /// "bot" | "user" (default: bot)
    #[serde(default)]
    pub mode: Option<String>,

    /// Required if mode=bot
    #[serde(default)]
    pub bot_token: Option<String>,

    /// Required if mode=user (from my.telegram.org)
    #[serde(default)]
    pub api_id: Option<i32>,

    /// Required if mode=user
    #[serde(default)]
    pub api_hash: Option<String>,

    /// Required if mode=user on first auth
    #[serde(default)]
    pub phone: Option<String>,

    /// Session store directory. For bot mode, defaults to a
    /// subdir of `$XDG_DATA_HOME/cipherocto/telegram-mtproto/`
    /// (or the platform equivalent via the `directories` crate
    /// — resolved in `MtprotoTelegramConfig::default_data_dir`).
    /// For user mode, REQUIRED.
    #[serde(default)]
    pub data_dir: Option<PathBuf>,

    /// Optional: 2FA password for user mode
    #[serde(default)]
    pub password: Option<String>,

    /// Optional: feature gates
    #[serde(default)]
    pub features: MtprotoTelegramFeatures,

    /// MTProto `api_layer` value to advertise in `initConnection`.
    /// Defaults to `DEFAULT_API_LAYER` (197). Pin a specific
    /// value to lock down the layer (downgrade or upgrade).
    #[serde(default)]
    pub api_layer: Option<i32>,

    /// Device model string for `initConnection` (cosmetic; logged
    /// in the Telegram session list as the device that connected).
    #[serde(default)]
    pub device_model: Option<String>,

    /// System version string for `initConnection` (cosmetic).
    #[serde(default)]
    pub system_version: Option<String>,

    /// App version string for `initConnection` (cosmetic).
    #[serde(default)]
    pub app_version: Option<String>,

    /// Override the default Telegram test DC URL. Defaults to
    /// `https://telegram.org` (production). Set to a test-DC URL
    /// to exercise the integration-test feature without
    /// touching production credentials.
    #[serde(default)]
    pub test_dc_url: Option<String>,
}

impl std::fmt::Debug for MtprotoTelegramConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Custom Debug: redact credentials. The
        // derive(Default) form does NOT redact secrets, so
        // we keep this manual impl. (clippy::derivable_impls
        // is a false positive here: the derived form would
        // leak `bot_token` / `api_hash` / `password` /
        // `phone` into the Debug output.)
        f.debug_struct("MtprotoTelegramConfig")
            .field("mode", &self.mode)
            .field("bot_token", &self.bot_token.as_ref().map(|_| "<redacted>"))
            .field("api_id", &self.api_id)
            .field("api_hash", &self.api_hash.as_ref().map(|_| "<redacted>"))
            .field("phone", &self.phone.as_ref().map(|_| "<redacted>"))
            .field("data_dir", &self.data_dir)
            .field("password", &self.password.as_ref().map(|_| "<redacted>"))
            .field("features", &self.features)
            .field("api_layer", &self.api_layer)
            .field("device_model", &self.device_model)
            .field("system_version", &self.system_version)
            .field("app_version", &self.app_version)
            .field("test_dc_url", &self.test_dc_url)
            .finish()
    }
}

impl MtprotoTelegramConfig {
    /// Returns "bot" or "user" (default "bot").
    pub fn mode_str(&self) -> &str {
        self.mode.as_deref().unwrap_or("bot")
    }

    /// Construct the runtime `AuthMode` from the flat config
    /// fields. Additive: existing JSON configs continue to
    /// deserialise identically; this method just interprets the
    /// `mode` discriminator + the flat credential fields.
    ///
    /// Recognised `mode` values: `"bot"`, `"user"`, `"qr"`,
    /// `"qr_login"`. Default is `AuthMode::BotToken` (empty
    /// token) — callers should call `validate()` first to get a
    /// usable error if the token is missing.
    pub fn auth_mode(&self) -> Result<crate::auth::AuthMode, String> {
        use crate::auth::AuthMode;
        match self.mode_str() {
            "bot" => Ok(AuthMode::BotToken(self.bot_token.clone().unwrap_or_default())),
            "user" => {
                let phone = self.phone.clone().ok_or_else(|| {
                    String::from("user mode requires phone field (set TELEGRAM_PHONE or mode=+phone)")
                })?;
                Ok(AuthMode::UserCredentials { phone })
            }
            "qr" | "qr_login" => Ok(AuthMode::QrLogin),
            other => Err(format!(
                "unknown mode '{}': expected 'bot', 'user', or 'qr'",
                other
            )),
        }
    }

    /// Resolved `api_layer` (configured value, or default).
    pub fn resolved_api_layer(&self) -> i32 {
        self.api_layer.unwrap_or(DEFAULT_API_LAYER)
    }

    /// Resolved `device_model` (configured value, or default).
    pub fn resolved_device_model(&self) -> &str {
        self.device_model.as_deref().unwrap_or(DEFAULT_DEVICE_MODEL)
    }

    /// Resolved `system_version`.
    pub fn resolved_system_version(&self) -> &str {
        self.system_version.as_deref().unwrap_or(DEFAULT_SYSTEM_VERSION)
    }

    /// Resolved `app_version`.
    pub fn resolved_app_version(&self) -> &str {
        self.app_version.as_deref().unwrap_or(DEFAULT_APP_VERSION)
    }

    /// Validate the config for the selected mode.
    pub fn validate(&self) -> Result<(), String> {
        // Feature gates: e2e_chats and voice_video are user-mode only.
        if self.features.e2e_chats && self.mode_str() != "user" {
            return Err("e2e_chats feature is only available in user mode".into());
        }
        if self.features.voice_video && self.mode_str() != "user" {
            return Err("voice_video feature is only available in user mode".into());
        }
        // api_layer sanity: must be in the [50, 200] range (Telegram
        // reserves below ~50 for tests and above ~200 for internal
        // use). 0 means "use default" so we accept that.
        if let Some(layer) = self.api_layer {
            if layer != 0 && !(50..=200).contains(&layer) {
                return Err(format!(
                    "api_layer out of range: {} (expected 0 or 50..=200)",
                    layer
                ));
            }
        }
        match self.mode_str() {
            "bot" => {
                if self.bot_token.is_none() || self.bot_token.as_deref().unwrap().is_empty() {
                    return Err("bot mode requires bot_token".into());
                }
                if self.api_id.is_none_or(|id| id <= 0) {
                    return Err("bot mode requires a positive api_id (from my.telegram.org)".into());
                }
                if self.api_hash.is_none() || self.api_hash.as_deref().unwrap().is_empty() {
                    return Err("bot mode requires api_hash (from my.telegram.org)".into());
                }
            }
            "user" => {
                if self.api_id.is_none() {
                    return Err("user mode requires api_id".into());
                }
                if self.api_id.is_none_or(|id| id <= 0) {
                    return Err("user mode api_id must be positive".into());
                }
                if self.api_hash.is_none() || self.api_hash.as_deref().unwrap().is_empty() {
                    return Err("user mode requires api_hash".into());
                }
                if self.phone.is_none() || self.phone.as_deref().unwrap().is_empty() {
                    return Err("user mode requires phone".into());
                }
                if self.data_dir.is_none() {
                    return Err("user mode requires data_dir".into());
                }
            }
            other => {
                return Err(format!("unknown mode: {}", other));
            }
        }
        Ok(())
    }

    /// Load config from environment variables. Mirrors
    /// `TelegramConfig::from_env` so a deployment that already
    /// uses `TELEGRAM_*` env vars works with the MTProto adapter
    /// too.
    pub fn from_env() -> Self {
        Self {
            mode: std::env::var("TELEGRAM_MODE").ok(),
            bot_token: std::env::var("TELEGRAM_BOT_TOKEN").ok(),
            api_id: std::env::var("TELEGRAM_API_ID")
                .ok()
                .and_then(|s| s.parse::<i32>().ok()),
            api_hash: std::env::var("TELEGRAM_API_HASH").ok(),
            phone: std::env::var("TELEGRAM_PHONE").ok(),
            password: std::env::var("TELEGRAM_PASSWORD").ok(),
            data_dir: std::env::var("TELEGRAM_DATA_DIR")
                .ok()
                .map(std::path::PathBuf::from),
            features: MtprotoTelegramFeatures::default(),
            api_layer: std::env::var("TELEGRAM_API_LAYER")
                .ok()
                .and_then(|s| s.parse::<i32>().ok()),
            device_model: std::env::var("TELEGRAM_DEVICE_MODEL").ok(),
            system_version: std::env::var("TELEGRAM_SYSTEM_VERSION").ok(),
            app_version: std::env::var("TELEGRAM_APP_VERSION").ok(),
            test_dc_url: std::env::var("TELEGRAM_TEST_DC_URL").ok(),
        }
    }

    /// Load config from a JSON file. Returns Err with a
    /// human-readable message if the file can't be read or
    /// parsed.
    pub fn from_file(path: &std::path::Path) -> Result<Self, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("read {}: {}", path.display(), e))?;
        serde_json::from_slice(&bytes).map_err(|e| format!("parse {}: {}", path.display(), e))
    }

    /// Load from file; fall back to env vars if the file is
    /// missing. Other read/parse errors are returned.
    pub fn from_file_or_env(path: &std::path::Path) -> Result<Self, String> {
        match Self::from_file(path) {
            Ok(c) => Ok(c),
            Err(e) if e.contains("No such file") || e.contains("not found") => Ok(Self::from_env()),
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bot_config() -> MtprotoTelegramConfig {
        MtprotoTelegramConfig {
            mode: Some("bot".into()),
            bot_token: Some("123:abc".into()),
            api_id: Some(12345),
            api_hash: Some("0123456789abcdef0123456789abcdef".into()),
            ..Default::default()
        }
    }

    #[test]
    fn validate_bot_ok() {
        assert!(bot_config().validate().is_ok());
    }

    #[test]
    fn validate_user_requires_data_dir() {
        let mut c = bot_config();
        c.mode = Some("user".into());
        c.phone = Some("+15555550100".into());
        c.data_dir = None;
        assert!(c.validate().is_err());
    }

    #[test]
    fn validate_user_ok_with_data_dir() {
        let mut c = bot_config();
        c.mode = Some("user".into());
        c.phone = Some("+15555550100".into());
        c.data_dir = Some(PathBuf::from("/tmp/x"));
        assert!(c.validate().is_ok());
    }

    #[test]
    fn validate_e2e_chats_user_only() {
        let mut c = bot_config();
        c.features.e2e_chats = true;
        assert!(c.validate().is_err());
    }

    #[test]
    fn api_layer_out_of_range() {
        let mut c = bot_config();
        c.api_layer = Some(999);
        assert!(c.validate().is_err());
    }

    #[test]
    fn api_layer_in_range() {
        let mut c = bot_config();
        c.api_layer = Some(150);
        assert!(c.validate().is_ok());
    }

    #[test]
    fn default_api_layer_is_197() {
        let c = MtprotoTelegramConfig::default();
        assert_eq!(c.resolved_api_layer(), 197);
    }

    #[test]
    fn debug_redacts_secrets() {
        let c = bot_config();
        let dbg = format!("{:?}", c);
        assert!(!dbg.contains("123:abc"));
        assert!(!dbg.contains("0123456789abcdef"));
    }

    #[test]
    fn auth_mode_bot_default() {
        // Default config has no mode field; auth_mode() falls back
        // to BotToken with the (default-empty) bot_token.
        let c = MtprotoTelegramConfig::default();
        match c.auth_mode().unwrap() {
            crate::auth::AuthMode::BotToken(t) => assert!(t.is_empty()),
            other => panic!("expected BotToken default, got {:?}", other),
        }
    }

    #[test]
    fn auth_mode_bot_with_token() {
        let c = bot_config();
        match c.auth_mode().unwrap() {
            crate::auth::AuthMode::BotToken(t) => assert_eq!(t, "123:abc"),
            other => panic!("expected BotToken, got {:?}", other),
        }
    }

    #[test]
    fn auth_mode_user_requires_phone() {
        // mode=user without phone is an auth_mode error (validate()
        // also catches it; both methods report it).
        let c = MtprotoTelegramConfig {
            mode: Some("user".into()),
            ..Default::default()
        };
        assert!(c.auth_mode().is_err());
    }

    #[test]
    fn auth_mode_user_with_phone() {
        let c = MtprotoTelegramConfig {
            mode: Some("user".into()),
            phone: Some("+15555550100".into()),
            ..Default::default()
        };
        match c.auth_mode().unwrap() {
            crate::auth::AuthMode::UserCredentials { phone } => {
                assert_eq!(phone, "+15555550100");
            }
            other => panic!("expected UserCredentials, got {:?}", other),
        }
    }

    #[test]
    fn auth_mode_qr_login() {
        for mode in ["qr", "qr_login"] {
            let c = MtprotoTelegramConfig {
                mode: Some(mode.into()),
                ..Default::default()
            };
            assert!(matches!(
                c.auth_mode().unwrap(),
                crate::auth::AuthMode::QrLogin
            ));
        }
    }

    #[test]
    fn auth_mode_unknown_rejected() {
        let c = MtprotoTelegramConfig {
            mode: Some("websocket".into()),
            ..Default::default()
        };
        let err = c.auth_mode().unwrap_err();
        assert!(err.contains("unknown mode"));
        assert!(err.contains("websocket"));
    }
}
