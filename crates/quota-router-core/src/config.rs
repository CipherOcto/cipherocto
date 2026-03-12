// Configuration module for quota-router-core
// Supports both JSON and YAML config files

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
    #[error("YAML error: {0}")]
    Yaml(#[from] serde_yaml::Error),
}

/// Router configuration (matches LiteLLM config format)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RouterConfig {
    /// Model list with provider info
    #[serde(default)]
    pub model_list: Vec<ModelGroup>,
    /// LiteLLM settings
    #[serde(default)]
    pub litellm_settings: Option<LiteLLMSettings>,
    /// General settings
    #[serde(default)]
    pub general_settings: Option<GeneralSettings>,
}

/// Model group configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelGroup {
    /// Model name
    pub model_name: String,
    /// Provider name
    pub provider_name: String,
    /// API base URL (optional)
    #[serde(default)]
    pub api_base: Option<String>,
}

/// LiteLLM-specific settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiteLLMSettings {
    /// Metadata for routing
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
    /// Fallback models
    #[serde(default)]
    pub fallbacks: Option<Vec<ModelGroup>>,
}

/// General settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralSettings {
    /// Master key for authentication
    #[serde(default)]
    pub master_key: Option<String>,
    /// Database URL
    #[serde(default)]
    pub database_url: Option<String>,
}

/// Main configuration struct
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// OCTO-W balance
    pub balance: u64,
    /// List of providers
    pub providers: Vec<Provider>,
    /// Proxy server port
    #[serde(default = "default_port")]
    pub proxy_port: u16,
    /// Router configuration (LiteLLM-style)
    #[serde(default)]
    pub router: Option<RouterConfig>,
}

fn default_port() -> u16 {
    8080
}

impl Config {
    /// Load config from a specific file path (YAML or JSON)
    pub fn load_from_path(path: &str) -> Result<Self, ConfigError> {
        let path = PathBuf::from(path);
        let content = std::fs::read_to_string(&path)?;

        // Try YAML first, then JSON
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        match ext.to_lowercase().as_str() {
            "yaml" | "yml" => Ok(serde_yaml::from_str(&content)?),
            "json" => Ok(serde_json::from_str(&content)?),
            _ => {
                // Try YAML first, then JSON
                serde_yaml::from_str(&content)
                    .or_else(|_| serde_json::from_str(&content))
                    .map_err(ConfigError::Json)
            }
        }
    }

    /// Load config from default location
    pub fn load() -> Result<Self, ConfigError> {
        let config_path = Self::config_path()?;
        if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)?;
            // Try YAML first, then JSON
            serde_yaml::from_str(&content)
                .or_else(|_: serde_yaml::Error| serde_json::from_str(&content))
                .map_err(ConfigError::Json)
        } else {
            // Default config
            Ok(Config {
                balance: 100, // Mock balance
                providers: vec![],
                proxy_port: 8080,
                router: None,
            })
        }
    }

    /// Save config to file
    pub fn save(&self) -> Result<(), ConfigError> {
        let config_path = Self::config_path()?;
        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // Save as YAML
        let content = serde_yaml::to_string(self)?;
        std::fs::write(&config_path, content)?;
        Ok(())
    }

    /// Save config to specific path
    pub fn save_to_path(&self, path: &str) -> Result<(), ConfigError> {
        let path = PathBuf::from(path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_yaml::to_string(self)?;
        std::fs::write(&path, content)?;
        Ok(())
    }

    fn config_path() -> Result<PathBuf, ConfigError> {
        let proj_dirs = ProjectDirs::from("com", "cipherocto", "quota-router")
            .ok_or(ConfigError::NoConfigDir)?;
        Ok(proj_dirs.config_dir().join("config.yaml"))
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            balance: 100,
            providers: vec![],
            proxy_port: 8080,
            router: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.balance, 100);
        assert_eq!(config.proxy_port, 8080);
    }

    #[test]
    fn test_yaml_roundtrip() {
        let config = Config {
            balance: 500,
            providers: vec![Provider::new("openai", "https://api.openai.com/v1")],
            proxy_port: 9090,
            router: Some(RouterConfig {
                model_list: vec![ModelGroup {
                    model_name: "gpt-4".to_string(),
                    provider_name: "openai".to_string(),
                    api_base: None,
                }],
                litellm_settings: None,
                general_settings: None,
            }),
        };

        let yaml = serde_yaml::to_string(&config).unwrap();
        let loaded: Config = serde_yaml::from_str(&yaml).unwrap();

        assert_eq!(loaded.balance, 500);
        assert_eq!(loaded.providers.len(), 1);
        assert_eq!(loaded.proxy_port, 9090);
        assert!(loaded.router.is_some());
    }
}
