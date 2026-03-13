// Fallback module - Fallback mechanisms for routing failures
// Based on LiteLLM's fallback handling

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Error types that can trigger fallbacks
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RouterError {
    /// Rate limit exceeded (429)
    RateLimit,
    /// Provider is unavailable (503)
    ProviderUnavailable,
    /// Authentication failed (401/403)
    AuthError,
    /// Content policy violation
    ContentPolicyViolation,
    /// Context window exceeded
    ContextWindowExceeded,
    /// General timeout
    Timeout,
    /// Unknown error
    Unknown,
}

impl RouterError {
    /// Determine fallback type based on error
    pub fn fallback_type(&self) -> FallbackType {
        match self {
            RouterError::RateLimit => FallbackType::General,
            RouterError::ProviderUnavailable => FallbackType::General,
            RouterError::AuthError => FallbackType::General,
            RouterError::ContentPolicyViolation => FallbackType::ContentPolicy,
            RouterError::ContextWindowExceeded => FallbackType::ContextWindow,
            RouterError::Timeout => FallbackType::General,
            RouterError::Unknown => FallbackType::General,
        }
    }
}

/// Type of fallback to use
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FallbackType {
    /// General fallback for rate limits, timeouts, etc.
    General,
    /// Fallback for content policy violations
    ContentPolicy,
    /// Fallback for context window exceeded
    ContextWindow,
}

/// A single fallback entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FallbackEntry {
    /// The model to fallback from
    pub model: String,
    /// Models to try in order
    pub fallback_models: Vec<String>,
}

/// Fallback configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FallbackConfig {
    /// General fallbacks: model -> [fallback models]
    #[serde(default)]
    pub fallbacks: Vec<FallbackEntry>,
    /// Context window fallbacks: model -> fallback model
    #[serde(default)]
    pub context_window_fallbacks: HashMap<String, String>,
    /// Content policy fallbacks: model -> fallback model
    #[serde(default)]
    pub content_policy_fallbacks: HashMap<String, String>,
    /// Maximum number of retries per request
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    /// Initial retry delay in milliseconds
    #[serde(default = "default_retry_delay_ms")]
    pub retry_delay_ms: u64,
    /// Backoff multiplier for exponential backoff
    #[serde(default = "default_backoff_multiplier")]
    pub backoff_multiplier: f64,
    /// Maximum backoff delay in milliseconds
    #[serde(default = "default_max_backoff_ms")]
    pub max_backoff_ms: u64,
}

fn default_max_retries() -> u32 {
    3
}

fn default_retry_delay_ms() -> u64 {
    100
}

fn default_backoff_multiplier() -> f64 {
    2.0
}

fn default_max_backoff_ms() -> u64 {
    5000
}

impl FallbackConfig {
    /// Get fallback models for a given model and error type
    pub fn get_fallback_models(&self, model: &str, error: RouterError) -> Option<Vec<String>> {
        let fallback_type = error.fallback_type();

        match fallback_type {
            FallbackType::ContextWindow => {
                // Check context window fallbacks first
                self.context_window_fallbacks
                    .get(model)
                    .map(|fb| vec![fb.clone()])
            }
            FallbackType::ContentPolicy => {
                // Check content policy fallbacks
                self.content_policy_fallbacks
                    .get(model)
                    .map(|fb| vec![fb.clone()])
            }
            FallbackType::General => {
                // Check general fallbacks
                self.fallbacks
                    .iter()
                    .find(|e| e.model == model)
                    .map(|e| e.fallback_models.clone())
            }
        }
    }

    /// Calculate retry delay with exponential backoff
    pub fn retry_delay(&self, attempt: u32) -> u64 {
        let delay = self.retry_delay_ms as f64 * self.backoff_multiplier.powi(attempt as i32);
        delay.min(self.max_backoff_ms as f64) as u64
    }
}

/// Fallback executor - handles fallback logic
#[derive(Debug, Clone)]
pub struct FallbackExecutor {
    config: FallbackConfig,
}

impl FallbackExecutor {
    pub fn new(config: FallbackConfig) -> Self {
        Self { config }
    }

    /// Get the configuration
    pub fn config(&self) -> &FallbackConfig {
        &self.config
    }

    /// Check if fallback is available for a model
    pub fn has_fallback(&self, model: &str, error: RouterError) -> bool {
        self.config.get_fallback_models(model, error).map(|v| !v.is_empty()).unwrap_or(false)
    }

    /// Get max retries
    pub fn max_retries(&self) -> u32 {
        self.config.max_retries
    }

    /// Calculate retry delay
    pub fn retry_delay(&self, attempt: u32) -> u64 {
        self.config.retry_delay(attempt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_fallback_config() -> FallbackConfig {
        let mut context_map = HashMap::new();
        context_map.insert("gpt-3.5-turbo".to_string(), "gpt-3.5-turbo-16k".to_string());
        let mut content_map = HashMap::new();
        content_map.insert("gpt-4".to_string(), "claude-3-opus".to_string());

        FallbackConfig {
            fallbacks: vec![
                FallbackEntry {
                    model: "gpt-3.5-turbo".to_string(),
                    fallback_models: vec!["gpt-4".to_string(), "claude-3-opus".to_string()],
                },
            ],
            context_window_fallbacks: context_map,
            content_policy_fallbacks: content_map,
            ..Default::default()
        }
    }

    #[test]
    fn test_general_fallback() {
        let config = test_fallback_config();
        let fallbacks = config.get_fallback_models("gpt-3.5-turbo", RouterError::RateLimit);
        assert!(fallbacks.is_some());
        assert_eq!(fallbacks.unwrap(), vec!["gpt-4", "claude-3-opus"]);
    }

    #[test]
    fn test_context_window_fallback() {
        let config = test_fallback_config();
        let fallbacks = config.get_fallback_models("gpt-3.5-turbo", RouterError::ContextWindowExceeded);
        assert!(fallbacks.is_some());
        assert_eq!(fallbacks.unwrap(), vec!["gpt-3.5-turbo-16k"]);
    }

    #[test]
    fn test_content_policy_fallback() {
        let config = test_fallback_config();
        let fallbacks = config.get_fallback_models("gpt-4", RouterError::ContentPolicyViolation);
        assert!(fallbacks.is_some());
        assert_eq!(fallbacks.unwrap(), vec!["claude-3-opus"]);
    }

    #[test]
    fn test_no_fallback() {
        let config = test_fallback_config();
        let fallbacks = config.get_fallback_models("unknown-model", RouterError::RateLimit);
        assert!(fallbacks.is_none());
    }

    #[test]
    fn test_exponential_backoff() {
        let config = FallbackConfig {
            max_retries: 3,
            retry_delay_ms: 100,
            backoff_multiplier: 2.0,
            max_backoff_ms: 5000,
            ..Default::default()
        };

        assert_eq!(config.retry_delay(0), 100);   // 100ms
        assert_eq!(config.retry_delay(1), 200);   // 100 * 2
        assert_eq!(config.retry_delay(2), 400);   // 100 * 4
        assert_eq!(config.retry_delay(3), 800);   // 100 * 8
        assert_eq!(config.retry_delay(10), 5000); // Capped at max
    }
}
