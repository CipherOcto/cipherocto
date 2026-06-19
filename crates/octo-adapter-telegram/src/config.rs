//! TelegramConfig — bot vs user mode, groups, data_dir.
//! Mission AC line 136.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TelegramFeatures {
    /// Enable access to secret chats (user mode only).
    /// Mission AC line 136: "features.e2e_chats (default false, user mode only)"
    #[serde(default)]
    pub e2e_chats: bool,

    /// Enable voice/video call hooks (user mode only).
    /// Mission AC line 136: "features.voice_video (default false, user mode only)"
    #[serde(default)]
    pub voice_video: bool,
}

#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TelegramConfig {
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

    /// TDLib auth_key persistence directory. Required for user mode.
    /// For bot mode, defaults to `None` (a temporary directory is used).
    #[serde(default)]
    pub data_dir: Option<PathBuf>,

    /// List of chat IDs to monitor (Bot mode)
    #[serde(default)]
    pub groups: Vec<String>,

    /// Optional: 2FA password for user mode
    #[serde(default)]
    pub password: Option<String>,

    /// Optional: webhook fallback (matches 0850f's webhook_port)
    #[serde(default)]
    pub webhook_port: Option<u16>,

    /// Optional: feature gates
    #[serde(default)]
    pub features: TelegramFeatures,
    /// Ed25519 verifying key for envelope signature verification (R7 CRYPTO-C3).
    /// Base64-encoded 32-byte public key.
    #[serde(default)]
    pub verifying_key: Option<String>,
}

impl std::fmt::Debug for TelegramConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TelegramConfig")
            .field("mode", &self.mode)
            .field("bot_token", &self.bot_token.as_ref().map(|_| "<redacted>"))
            .field("api_id", &self.api_id)
            .field("api_hash", &self.api_hash.as_ref().map(|_| "<redacted>"))
            .field("phone", &self.phone.as_ref().map(|_| "<redacted>"))
            .field("password", &self.password.as_ref().map(|_| "<redacted>"))
            .field("data_dir", &self.data_dir)
            .field("groups", &self.groups)
            .field("webhook_port", &self.webhook_port)
            .field("features", &self.features)
            .field(
                "verifying_key",
                &self.verifying_key.as_ref().map(|_| "<redacted>"),
            )
            .finish()
    }
}

impl TelegramConfig {
    /// Returns "bot" or "user" (default "bot").
    pub fn mode_str(&self) -> &str {
        self.mode.as_deref().unwrap_or("bot")
    }

    /// Validate the config for the selected mode.
    /// Returns `Err` with a message for the first missing required field.
    pub fn validate(&self) -> std::result::Result<(), String> {
        // Feature gates: e2e_chats and voice_video are user-mode only
        // API-L1: validate verifying_key is valid base64 if set
        if let Some(ref key) = self.verifying_key {
            if key.len() != 44 {
                // base64 of 32 bytes = Ceil(32*4/3) = 44
                return Err("verifying_key must be 44-char base64 string (32 bytes)".into());
            }
            // M2: also validate that the string is actually valid base64
            use base64::Engine as _;
            if let Err(e) = base64::engine::general_purpose::STANDARD.decode(key) {
                return Err(format!("verifying_key is not valid base64: {}", e));
            }
        }
        // CFG-L3: validate groups are parseable as i64
        for group in &self.groups {
            if group.parse::<i64>().is_err() {
                return Err(format!("groups: {} is not a valid i64 chat_id", group));
            }
        }
        if self.webhook_port == Some(0) {
            return Err("webhook_port must be positive or absent".into());
        }
        if self.features.e2e_chats && self.mode_str() != "user" {
            return Err("e2e_chats feature is only available in user mode".into());
        }
        if self.features.voice_video && self.mode_str() != "user" {
            return Err("voice_video feature is only available in user mode".into());
        }
        match self.mode_str() {
            "bot" => {
                if self.bot_token.is_none() || self.bot_token.as_deref().unwrap().is_empty() {
                    return Err("bot mode requires bot_token".into());
                }
                // C2: bot mode calls `set_tdlib_parameters` with these
                // credentials, so they are required even for bot mode.
                // Synthetic credentials (`api_id=0`, `api_hash=""`) are only
                // valid on the test DC; production callers must supply real
                // credentials from my.telegram.org.
                // M3: api_id must be positive (negative values pass the == Some(0) check).
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
                // L2: api_id == 0 is a sentinel value that TDLib rejects; reject
                // it at config-validate time so callers fail fast.
                // M3: also reject negative values.
                if self.api_id.is_none_or(|id| id <= 0) {
                    return Err("user mode api_id must be positive".into());
                }
                if self.api_hash.is_none() || self.api_hash.as_deref().unwrap().is_empty() {
                    return Err("user mode requires api_hash".into());
                }
                if self.phone.is_none() || self.phone.as_deref().unwrap().is_empty() {
                    return Err("user mode requires phone".into());
                }
                // L3: user mode requires a data_dir for TDLib auth persistence.
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

    /// Load config from environment variables (R7 CFG-C2).
    /// Supported vars:
    /// - `TELEGRAM_MODE` — "bot" or "user"
    /// - `TELEGRAM_BOT_TOKEN` — bot token (mode=bot)
    /// - `TELEGRAM_API_ID` — api_id from my.telegram.org
    /// - `TELEGRAM_API_HASH` — api_hash from my.telegram.org
    /// - `TELEGRAM_PHONE` — phone number (mode=user)
    /// - `TELEGRAM_PASSWORD` — 2FA password (optional, mode=user)
    /// - `TELEGRAM_DATA_DIR` — data directory for TDLib database
    /// - `TELEGRAM_VERIFYING_KEY` — Ed25519 public key (base64, optional)
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
            groups: vec![],
            webhook_port: None,
            features: TelegramFeatures::default(),
            verifying_key: std::env::var("TELEGRAM_VERIFYING_KEY").ok(),
        }
    }

    /// Load config from a JSON file. Returns Err with a human-readable
    /// message if the file can't be read or parsed. Use this for
    /// "load an existing on-disk config" use cases (live tests, CLI
    /// tools that read the config the auth flow wrote). For fresh
    /// env-only construction, use `from_env()`.
    pub fn from_file(path: &std::path::Path) -> std::result::Result<Self, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("read {}: {}", path.display(), e))?;
        serde_json::from_slice(&bytes).map_err(|e| format!("parse {}: {}", path.display(), e))
    }

    /// Load from file; fall back to env vars if the file is missing.
    /// Other read/parse errors are returned (not silently masked).
    /// This is the common path for live tests: the auth flow wrote
    /// `telegram.json`, the test reads it, and we want env vars as
    /// an override layer.
    pub fn from_file_or_env(path: &std::path::Path) -> std::result::Result<Self, String> {
        match Self::from_file(path) {
            Ok(c) => Ok(c),
            Err(e) if e.contains("No such file") || e.contains("not found") => Ok(Self::from_env()),
            Err(e) => Err(e),
        }
    }
}
