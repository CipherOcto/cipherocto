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
    pub api_id: Option<u32>,

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
}
