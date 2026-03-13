// Rate limiting module - RPM/TPM enforcement for routing
// Based on LiteLLM's rate limiting implementation

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Rate limit mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum RateLimitMode {
    /// RPM/TPM used for routing decisions only (default)
    #[default]
    Soft,
    /// Hard blocking when limit exceeded
    Hard,
}

/// Rate limit configuration per model/deployment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    /// Requests per minute limit
    #[serde(default)]
    pub rpm: Option<u32>,
    /// Tokens per minute limit
    #[serde(default)]
    pub tpm: Option<u32>,
    /// Enforcement mode
    #[serde(default)]
    pub mode: RateLimitMode,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            rpm: None,
            tpm: None,
            mode: RateLimitMode::Soft,
        }
    }
}

/// Rate limit status for a provider
#[derive(Debug, Clone, Default)]
pub struct RateLimitStatus {
    /// Current RPM usage
    pub current_rpm: u32,
    /// Current TPM usage
    pub current_tpm: u32,
    /// Last reset timestamp (seconds since epoch)
    pub last_reset: u64,
}

/// Rate limiter - enforces RPM/TPM limits
#[derive(Debug, Clone)]
pub struct RateLimiter {
    config: RateLimitConfig,
    /// Current usage per provider
    usage: HashMap<String, RateLimitStatus>,
    /// Window size in seconds (typically 60 for 1 minute)
    window_seconds: u64,
}

impl RateLimiter {
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            config,
            usage: HashMap::new(),
            window_seconds: 60,
        }
    }

    /// Get the configuration
    pub fn config(&self) -> &RateLimitConfig {
        &self.config
    }

    /// Check if request is allowed (for hard mode)
    pub fn check(&self, provider_id: &str) -> RateLimitResult {
        let default_status = RateLimitStatus::default();
        let status = self.usage.get(provider_id).unwrap_or(&default_status);

        // Check RPM limit
        if let Some(limit) = self.config.rpm {
            if status.current_rpm >= limit {
                return RateLimitResult::Blocked {
                    reason: format!("RPM limit exceeded: {}/{}", status.current_rpm, limit),
                    retry_after: Some(
                        self.window_seconds - (status.last_reset % self.window_seconds),
                    ),
                };
            }
        }

        // Check TPM limit
        if let Some(limit) = self.config.tpm {
            if status.current_tpm >= limit {
                return RateLimitResult::Blocked {
                    reason: format!("TPM limit exceeded: {}/{}", status.current_tpm, limit),
                    retry_after: Some(
                        self.window_seconds - (status.last_reset % self.window_seconds),
                    ),
                };
            }
        }

        RateLimitResult::Allowed
    }

    /// Record a request (increment usage)
    pub fn record(&mut self, provider_id: &str, tokens: u32) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let status = self
            .usage
            .entry(provider_id.to_string())
            .or_insert_with(|| RateLimitStatus {
                current_rpm: 0,
                current_tpm: 0,
                last_reset: now,
            });

        // Reset if window has passed
        if now - status.last_reset >= self.window_seconds {
            status.current_rpm = 0;
            status.current_tpm = 0;
            status.last_reset = now;
        }

        // Increment usage
        status.current_rpm = status.current_rpm.saturating_add(1);
        status.current_tpm = status.current_tpm.saturating_add(tokens);
    }

    /// Get current usage for a provider
    pub fn usage(&self, provider_id: &str) -> Option<&RateLimitStatus> {
        self.usage.get(provider_id)
    }

    /// Reset usage for a provider
    pub fn reset(&mut self, provider_id: &str) {
        if let Some(status) = self.usage.get_mut(provider_id) {
            status.current_rpm = 0;
            status.current_tpm = 0;
        }
    }
}

/// Result of a rate limit check
#[derive(Debug, Clone)]
pub enum RateLimitResult {
    /// Request is allowed
    Allowed,
    /// Request is blocked
    Blocked {
        reason: String,
        retry_after: Option<u64>,
    },
}

impl RateLimitResult {
    pub fn is_allowed(&self) -> bool {
        matches!(self, RateLimitResult::Allowed)
    }

    pub fn is_blocked(&self) -> bool {
        matches!(self, RateLimitResult::Blocked { .. })
    }
}

/// Global rate limiter manager
#[derive(Debug, Clone)]
pub struct RateLimiterManager {
    /// Rate limiters per model group
    limiters: HashMap<String, RateLimiter>,
    /// Default config
    default_config: RateLimitConfig,
}

impl RateLimiterManager {
    pub fn new(default_config: RateLimitConfig) -> Self {
        Self {
            limiters: HashMap::new(),
            default_config,
        }
    }

    /// Get or create a rate limiter for a model group
    pub fn get_or_create(
        &mut self,
        model_group: &str,
        config: Option<RateLimitConfig>,
    ) -> &mut RateLimiter {
        self.limiters
            .entry(model_group.to_string())
            .or_insert_with(|| {
                RateLimiter::new(config.unwrap_or_else(|| self.default_config.clone()))
            })
    }

    /// Check if request is allowed (hard mode)
    pub fn check(&self, model_group: &str, provider_id: &str) -> RateLimitResult {
        self.limiters
            .get(model_group)
            .map(|l| l.check(provider_id))
            .unwrap_or(RateLimitResult::Allowed)
    }

    /// Record a request
    pub fn record(&mut self, model_group: &str, provider_id: &str, tokens: u32) {
        if let Some(limiter) = self.limiters.get_mut(model_group) {
            limiter.record(provider_id, tokens);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_rate_limiter() -> RateLimiter {
        RateLimiter::new(RateLimitConfig {
            rpm: Some(10),
            tpm: Some(1000),
            mode: RateLimitMode::Hard,
        })
    }

    #[test]
    fn test_rate_limit_allowed() {
        let limiter = test_rate_limiter();
        // First 10 requests should be allowed
        for _ in 0..10 {
            assert!(limiter.check("provider1").is_allowed());
        }
    }

    #[test]
    fn test_rate_limit_blocked() {
        let mut limiter = test_rate_limiter();
        // Record 10 requests
        for _ in 0..10 {
            limiter.record("provider1", 100);
        }
        // 11th should be blocked
        assert!(limiter.check("provider1").is_blocked());
    }

    #[test]
    fn test_rate_limit_tpm() {
        let mut limiter = RateLimiter::new(RateLimitConfig {
            rpm: Some(100),
            tpm: Some(500),
            mode: RateLimitMode::Hard,
        });

        // Record 5 requests with 100 tokens each = 500 TPM
        for _ in 0..5 {
            limiter.record("provider1", 100);
        }

        // Next request should be blocked due to TPM
        let result = limiter.check("provider1");
        assert!(result.is_blocked());
    }

    #[test]
    fn test_soft_mode_allows_over_limit() {
        let limiter = RateLimiter::new(RateLimitConfig {
            rpm: Some(10),
            tpm: Some(1000),
            mode: RateLimitMode::Soft,
        });

        // Soft mode always allows, used for routing decisions only
        for _ in 0..100 {
            assert!(limiter.check("provider1").is_allowed());
        }
    }

    #[test]
    fn test_rate_limiter_manager() {
        let mut manager = RateLimiterManager::new(RateLimitConfig {
            rpm: Some(10),
            tpm: None,
            mode: RateLimitMode::Hard,
        });

        // Create limiter for model group
        let limiter = manager.get_or_create("gpt-3.5-turbo", None);
        assert!(limiter.config().rpm.is_some());

        // Record some requests
        for _ in 0..5 {
            manager.record("gpt-3.5-turbo", "openai", 100);
        }

        // Check should still allow
        assert!(manager.check("gpt-3.5-turbo", "openai").is_allowed());
    }
}
