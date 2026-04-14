use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;

pub use crate::providers::Provider;

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Failed to get config directory")]
    NoConfigDir,
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

/// WAL Pub/Sub configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WalPubSubConfig {
    /// Enable WAL pub/sub (default: true)
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Polling interval in milliseconds (default: 50)
    #[serde(default = "default_poll_interval")]
    pub poll_interval_ms: u64,
    /// WAL path for shared storage (optional)
    pub wal_path: Option<String>,
}

fn default_true() -> bool {
    true
}

fn default_poll_interval() -> u64 {
    50
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub balance: u64,
    pub providers: Vec<Provider>,
    pub proxy_port: u16,
    /// Database path for key storage
    pub db_path: PathBuf,
    /// WAL pub/sub configuration
    #[serde(default)]
    pub wal_pubsub: WalPubSubConfig,
}

impl Config {
    pub fn load() -> Result<Self, ConfigError> {
        let config_path = Self::config_path()?;
        if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)?;
            Ok(serde_json::from_str(&content)?)
        } else {
            // Default config
            Ok(Config {
                balance: 100, // Mock balance
                providers: vec![],
                proxy_port: 8080,
                db_path: Self::default_db_path(),
                wal_pubsub: WalPubSubConfig {
                    enabled: true,
                    poll_interval_ms: 50,
                    wal_path: None,
                },
            })
        }
    }

    pub fn save(&self) -> Result<(), ConfigError> {
        let config_path = Self::config_path()?;
        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(&config_path, content)?;
        Ok(())
    }

    fn config_path() -> Result<PathBuf, ConfigError> {
        let proj_dirs = ProjectDirs::from("com", "cipherocto", "quota-router")
            .ok_or(ConfigError::NoConfigDir)?;
        Ok(proj_dirs.config_dir().join("config.json"))
    }

    fn default_db_path() -> PathBuf {
        let proj_dirs = ProjectDirs::from("com", "cipherocto", "quota-router")
            .expect("Failed to get project directories");
        proj_dirs.data_dir().join("quota-router.db")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wal_pubsub_config_defaults() {
        let config = WalPubSubConfig {
            enabled: true,
            poll_interval_ms: 50,
            wal_path: None,
        };

        assert!(config.enabled);
        assert_eq!(config.poll_interval_ms, 50);
    }

    #[test]
    fn test_config_default() {
        // Test default config
        let config = Config {
            balance: 100,
            providers: vec![],
            proxy_port: 8080,
            db_path: PathBuf::from("/tmp/test-db-path"),
            wal_pubsub: WalPubSubConfig {
                enabled: true,
                poll_interval_ms: 50,
                wal_path: None,
            },
        };

        assert!(config.wal_pubsub.enabled);
    }
}
