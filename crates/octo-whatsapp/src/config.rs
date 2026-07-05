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
pub struct WhatsAppRuntimeConfig {
    pub name: String,
    #[serde(default = "default_data_dir")]
    pub data_dir: PathBuf,
    #[serde(default = "default_log_dir")]
    pub log_dir: PathBuf,
    #[serde(default = "default_socket_dir")]
    pub socket_dir: PathBuf,
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
        Ok(())
    }
}

#[cfg(test)]
mod tests;
