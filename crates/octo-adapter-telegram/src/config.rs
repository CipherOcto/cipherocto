//! TelegramConfig — bot vs user mode, groups, data_dir.
//! Mission AC line 136.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
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

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
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

    /// TDLib auth_key persistence directory
    #[serde(default)]
    pub data_dir: PathBuf,

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
                if self.api_id.is_none() || self.api_id == Some(0) {
                    return Err("bot mode requires api_id (from my.telegram.org)".into());
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
                if self.api_id == Some(0) {
                    return Err("user mode api_id must be non-zero".into());
                }
                if self.api_hash.is_none() || self.api_hash.as_deref().unwrap().is_empty() {
                    return Err("user mode requires api_hash".into());
                }
                if self.phone.is_none() || self.phone.as_deref().unwrap().is_empty() {
                    return Err("user mode requires phone".into());
                }
            }
            other => {
                return Err(format!("unknown mode: {}", other));
            }
        }
        Ok(())
    }
}
