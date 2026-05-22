// Fallback module - Fallback mechanisms for routing failures
// Based on LiteLLM's fallback handling

use parking_lot::Mutex;
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FallbackConfig {
    /// General fallbacks: model -> [fallback models]
    #[serde(default)]
    pub fallbacks: Vec<FallbackEntry>,
    /// Context window fallbacks: model -> [fallback models]
    #[serde(default)]
    pub context_window_fallbacks: HashMap<String, Vec<String>>,
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
    /// Allowed consecutive failures before marking model unhealthy
    #[serde(default = "default_allowed_fails")]
    pub allowed_fails: u32,
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

fn default_allowed_fails() -> u32 {
    5
}

impl Default for FallbackConfig {
    fn default() -> Self {
        Self {
            fallbacks: Vec::new(),
            context_window_fallbacks: HashMap::new(),
            content_policy_fallbacks: HashMap::new(),
            max_retries: default_max_retries(),
            retry_delay_ms: default_retry_delay_ms(),
            backoff_multiplier: default_backoff_multiplier(),
            max_backoff_ms: default_max_backoff_ms(),
            allowed_fails: default_allowed_fails(),
        }
    }
}

impl FallbackConfig {
    /// Get fallback models for a given model and error type
    pub fn get_fallback_models(&self, model: &str, error: RouterError) -> Option<Vec<String>> {
        let fallback_type = error.fallback_type();

        match fallback_type {
            FallbackType::ContextWindow => {
                // Check context window fallbacks first
                self.context_window_fallbacks.get(model).cloned()
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

/// Health state for a model
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum HealthState {
    #[default]
    Healthy,
    Unhealthy,
}

/// Per-model health tracking with consecutive failure counting
#[derive(Debug, Clone, Default)]
pub struct ModelHealthTracker {
    /// Current health state
    pub state: HealthState,
    /// Consecutive failure count
    pub consecutive_failures: u32,
}

impl ModelHealthTracker {
    pub fn new() -> Self {
        Self {
            state: HealthState::Healthy,
            consecutive_failures: 0,
        }
    }

    /// Record a successful request - resets consecutive failures
    pub fn record_success(&mut self) {
        self.consecutive_failures = 0;
        self.state = HealthState::Healthy;
    }

    /// Record a failure - increments consecutive failures
    pub fn record_failure(&mut self) {
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
    }

    /// Check if model should be marked unhealthy based on allowed_fails threshold
    pub fn should_mark_unhealthy(&self, allowed_fails: u32) -> bool {
        self.consecutive_failures >= allowed_fails
    }

    /// Mark model as unhealthy
    pub fn mark_unhealthy(&mut self) {
        self.state = HealthState::Unhealthy;
    }

    /// Check if model is available
    pub fn is_available(&self) -> bool {
        self.state == HealthState::Healthy
    }
}

/// Fallback executor - handles fallback logic
///
/// Uses interior mutability for model health tracking so it can be
/// safely mutated through `Arc<FallbackExecutor>` from the proxy request path.
#[derive(Debug)]
pub struct FallbackExecutor {
    config: FallbackConfig,
    /// Per-model health tracking (interior mutability for Arc usage)
    model_health: Mutex<HashMap<String, ModelHealthTracker>>,
}

impl Clone for FallbackExecutor {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            model_health: Mutex::new(self.model_health.lock().clone()),
        }
    }
}

impl FallbackExecutor {
    pub fn new(config: FallbackConfig) -> Self {
        Self {
            config,
            model_health: Mutex::new(HashMap::new()),
        }
    }

    /// Get the configuration
    pub fn config(&self) -> &FallbackConfig {
        &self.config
    }

    /// Check if fallback is available for a model
    pub fn has_fallback(&self, model: &str, error: RouterError) -> bool {
        self.config
            .get_fallback_models(model, error)
            .map(|v| !v.is_empty())
            .unwrap_or(false)
    }

    /// Get max retries
    pub fn max_retries(&self) -> u32 {
        self.config.max_retries
    }

    /// Calculate retry delay
    pub fn retry_delay(&self, attempt: u32) -> u64 {
        self.config.retry_delay(attempt)
    }

    /// Record a successful request for a model - resets consecutive failures
    pub fn record_success(&self, model: &str) {
        let mut health = self.model_health.lock();
        let tracker = health.entry(model.to_string()).or_default();
        tracker.record_success();
    }

    /// Record a failure for a model - increments consecutive failures
    /// and marks unhealthy if threshold exceeded
    pub fn record_failure(&self, model: &str) {
        let allowed_fails = self.config.allowed_fails;
        let mut health = self.model_health.lock();
        let tracker = health.entry(model.to_string()).or_default();
        tracker.record_failure();
        if tracker.should_mark_unhealthy(allowed_fails) {
            tracker.mark_unhealthy();
        }
    }

    /// Check if a model is healthy (hasn't exceeded allowed_fails)
    pub fn is_model_healthy(&self, model: &str) -> bool {
        let health = self.model_health.lock();
        health.get(model).map(|t| t.is_available()).unwrap_or(true) // Unknown models are considered healthy
    }

    /// Get a copy of the health tracker for a model (for inspection)
    pub fn get_model_health(&self, model: &str) -> Option<ModelHealthTracker> {
        let health = self.model_health.lock();
        health.get(model).cloned()
    }

    /// Reset a model's health to healthy (e.g., after cooldown)
    pub fn reset_model_health(&self, model: &str) {
        let mut health = self.model_health.lock();
        let tracker = health.entry(model.to_string()).or_default();
        tracker.record_success();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_fallback_config() -> FallbackConfig {
        let mut context_map = HashMap::new();
        context_map.insert(
            "gpt-3.5-turbo".to_string(),
            vec!["gpt-3.5-turbo-16k".to_string()],
        );
        let mut content_map = HashMap::new();
        content_map.insert("gpt-4".to_string(), "claude-3-opus".to_string());

        FallbackConfig {
            fallbacks: vec![FallbackEntry {
                model: "gpt-3.5-turbo".to_string(),
                fallback_models: vec!["gpt-4".to_string(), "claude-3-opus".to_string()],
            }],
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
        let fallbacks =
            config.get_fallback_models("gpt-3.5-turbo", RouterError::ContextWindowExceeded);
        assert!(fallbacks.is_some());
        assert_eq!(fallbacks.unwrap(), vec!["gpt-3.5-turbo-16k"]);
    }

    #[test]
    fn test_context_window_fallback_multiple() {
        let mut context_map = HashMap::new();
        context_map.insert(
            "gpt-3.5-turbo".to_string(),
            vec![
                "gpt-3.5-turbo-16k".to_string(),
                "gpt-4".to_string(),
                "claude-3-opus".to_string(),
            ],
        );
        let config = FallbackConfig {
            context_window_fallbacks: context_map,
            ..Default::default()
        };
        let fallbacks =
            config.get_fallback_models("gpt-3.5-turbo", RouterError::ContextWindowExceeded);
        assert!(fallbacks.is_some());
        assert_eq!(
            fallbacks.unwrap(),
            vec!["gpt-3.5-turbo-16k", "gpt-4", "claude-3-opus"]
        );
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

        assert_eq!(config.retry_delay(0), 100); // 100ms
        assert_eq!(config.retry_delay(1), 200); // 100 * 2
        assert_eq!(config.retry_delay(2), 400); // 100 * 4
        assert_eq!(config.retry_delay(3), 800); // 100 * 8
        assert_eq!(config.retry_delay(10), 5000); // Capped at max
    }

    #[test]
    fn test_allowed_fails_default() {
        let config = FallbackConfig::default();
        assert_eq!(config.allowed_fails, 5);
    }

    #[test]
    fn test_model_health_tracker_new() {
        let tracker = ModelHealthTracker::new();
        assert_eq!(tracker.state, HealthState::Healthy);
        assert_eq!(tracker.consecutive_failures, 0);
        assert!(tracker.is_available());
    }

    #[test]
    fn test_model_health_tracker_record_success() {
        let mut tracker = ModelHealthTracker::new();
        tracker.record_failure();
        tracker.record_failure();
        assert_eq!(tracker.consecutive_failures, 2);

        tracker.record_success();
        assert_eq!(tracker.consecutive_failures, 0);
        assert_eq!(tracker.state, HealthState::Healthy);
    }

    #[test]
    fn test_model_health_tracker_should_mark_unhealthy() {
        let mut tracker = ModelHealthTracker::new();
        assert!(!tracker.should_mark_unhealthy(5));

        for _ in 0..4 {
            tracker.record_failure();
        }
        assert!(!tracker.should_mark_unhealthy(5));

        tracker.record_failure();
        assert!(tracker.should_mark_unhealthy(5));
    }

    #[test]
    fn test_fallback_executor_health_tracking() {
        let config = FallbackConfig {
            allowed_fails: 3,
            ..Default::default()
        };
        let executor = FallbackExecutor::new(config);

        // Model starts healthy
        assert!(executor.is_model_healthy("gpt-4"));

        // Record failures
        executor.record_failure("gpt-4");
        assert!(executor.is_model_healthy("gpt-4"));
        executor.record_failure("gpt-4");
        assert!(executor.is_model_healthy("gpt-4"));
        executor.record_failure("gpt-4");
        assert!(!executor.is_model_healthy("gpt-4"));

        // Check health tracker
        let health = executor.get_model_health("gpt-4").unwrap();
        assert_eq!(health.state, HealthState::Unhealthy);
        assert_eq!(health.consecutive_failures, 3);
    }

    #[test]
    fn test_fallback_executor_success_resets_health() {
        let config = FallbackConfig {
            allowed_fails: 3,
            ..Default::default()
        };
        let executor = FallbackExecutor::new(config);

        // Fail twice, succeed, fail twice more - should still be healthy
        executor.record_failure("gpt-4");
        executor.record_failure("gpt-4");
        executor.record_success("gpt-4");
        executor.record_failure("gpt-4");
        executor.record_failure("gpt-4");
        assert!(executor.is_model_healthy("gpt-4"));
    }

    #[test]
    fn test_fallback_executor_reset_model_health() {
        let config = FallbackConfig {
            allowed_fails: 2,
            ..Default::default()
        };
        let executor = FallbackExecutor::new(config);

        executor.record_failure("gpt-4");
        executor.record_failure("gpt-4");
        assert!(!executor.is_model_healthy("gpt-4"));

        executor.reset_model_health("gpt-4");
        assert!(executor.is_model_healthy("gpt-4"));
    }

    #[test]
    fn test_fallback_executor_unknown_model_is_healthy() {
        let config = FallbackConfig::default();
        let executor = FallbackExecutor::new(config);
        assert!(executor.is_model_healthy("nonexistent-model"));
    }
}
