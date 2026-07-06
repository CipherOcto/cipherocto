//! Runtime configuration loaded from a TOML file.
//!
//! Phase 1: minimal schema (name + paths + socket). Rules, triggers,
//! event-retention, observability, and security fields arrive in later
//! phases. The schema is intentionally additive.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("toml parse: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("invalid name {0:?}: must match [a-z0-9_-]+")]
    InvalidName(String),
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct MediaBufferConfig {
    /// Maximum concurrent in-flight media uploads. Bounded to keep
    /// disk + memory under control. `0` is invalid.
    pub max_concurrent_uploads: usize,
    /// Root temp directory under which per-request `.bin` files live.
    pub root: PathBuf,
}

impl Default for MediaBufferConfig {
    fn default() -> Self {
        Self {
            max_concurrent_uploads: 4,
            root: std::env::temp_dir().join("octo-whatsapp"),
        }
    }
}

/// Phase 3: in-memory events retention. Bounded by `max_rows` (cap)
/// and `retention_days` (TTL, currently advisory — `max_rows` is the
/// primary bound). Default `max_rows = 1_000_000` and
/// `retention_days = 30` per design §InboundEvent retention.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct EventsConfig {
    pub max_rows: usize,
    pub retention_days: u32,
}

impl Default for EventsConfig {
    fn default() -> Self {
        Self {
            max_rows: 1_000_000,
            retention_days: 30,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct WhatsAppRuntimeConfig {
    pub name: String,
    #[serde(default = "default_data_dir")]
    pub data_dir: PathBuf,
    #[serde(default = "default_log_dir")]
    pub log_dir: PathBuf,
    #[serde(default = "default_socket_dir")]
    pub socket_dir: PathBuf,
    /// Media buffer tuning. Defaults to 4 concurrent uploads under
    /// `$TMPDIR/octo-whatsapp` (safe production default).
    #[serde(default)]
    pub media_buffer: MediaBufferConfig,
    /// Phase 3: events retention. Default 1M rows, 30 days.
    #[serde(default)]
    pub events: EventsConfig,
}

impl Default for WhatsAppRuntimeConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            data_dir: default_data_dir(),
            log_dir: default_log_dir(),
            socket_dir: default_socket_dir(),
            media_buffer: MediaBufferConfig::default(),
            events: EventsConfig::default(),
        }
    }
}

fn default_data_dir() -> PathBuf {
    PathBuf::from("/var/lib/octo/whatsapp")
}
fn default_log_dir() -> PathBuf {
    PathBuf::from("/var/log/octo/whatsapp")
}
fn default_socket_dir() -> PathBuf {
    PathBuf::from("/run/octo/whatsapp")
}

impl WhatsAppRuntimeConfig {
    pub fn from_toml(bytes: &[u8]) -> Result<Self, ConfigError> {
        let s = std::str::from_utf8(bytes).map_err(|e| {
            ConfigError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
        })?;
        let cfg: Self = toml::from_str(s)?;
        cfg.validate()?;
        Ok(cfg)
    }

    pub fn from_path(path: &Path) -> Result<Self, ConfigError> {
        let bytes = std::fs::read(path)?;
        Self::from_toml(&bytes)
    }

    pub fn socket_path(&self) -> PathBuf {
        self.socket_dir
            .join(format!("octo-whatsapp-{}.sock", self.name))
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.name.is_empty()
            || !self
                .name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
        {
            return Err(ConfigError::InvalidName(self.name.clone()));
        }
        if self.media_buffer.max_concurrent_uploads == 0 {
            return Err(ConfigError::InvalidName(
                "media_buffer.max_concurrent_uploads must be > 0 (got 0)".to_string(),
            ));
        }
        if self.media_buffer.root.as_os_str().is_empty() {
            return Err(ConfigError::InvalidName(
                "media_buffer.root must be non-empty".into(),
            ));
        }
        if self.events.max_rows == 0 {
            return Err(ConfigError::InvalidName(
                "events.max_rows must be > 0 (got 0)".to_string(),
            ));
        }
        if self.events.retention_days == 0 {
            return Err(ConfigError::InvalidName(
                "events.retention_days must be > 0 (got 0)".to_string(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
