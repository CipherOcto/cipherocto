// Per-key rate limiting - TokenBucket algorithm per RFC-0903 §Rate Limiting Algorithm
//
// RATE LIMITING DETERMINISM DISCLAIMER:
// Rate limiting is NOT deterministic across router nodes. TokenBucket refill
// depends on monotonic clock elapsed time, which may vary between nodes.
// Rate limiting MUST NOT influence accounting logic - it is purely for
// throttling purposes. Budget enforcement happens at the storage/ledger layer.
//

use crate::keys::{ApiKey, KeyError};
use dashmap::DashMap;
use std::time::Instant;

/// Token bucket rate limiter for per-key rate limiting.
///
/// Uses u64 integers for cross-platform deterministic behavior.
/// Uses Instant for monotonic time source (immune to clock adjustments).
///
/// # Determinism Note
/// While the algorithm itself uses integer arithmetic (deterministic), the
/// elapsed time measurement via `Instant::elapsed()` is NOT deterministic
/// across processes. This is acceptable for rate limiting which is purely
/// a throttling mechanism, not part of accounting.
pub struct TokenBucket {
    capacity: u64,
    tokens: u64,
    /// Refill rate: tokens per minute (stored as-is, converted in calculations)
    refill_rate_per_minute: u64,
    last_refill: Instant, // Monotonic time - immune to clock adjustments
    last_access: Instant, // For cleanup tracking
}

impl TokenBucket {
    /// Create a new TokenBucket with given capacity and refill rate.
    pub fn new(capacity: u32, refill_per_minute: u32) -> Self {
        let refill_rate_per_minute = refill_per_minute as u64;
        let now = Instant::now();
        Self {
            capacity: capacity as u64,
            tokens: capacity as u64,
            refill_rate_per_minute,
            last_refill: now,
            last_access: now,
        }
    }

    /// Try to consume tokens, returns true if successful.
    pub fn try_consume(&mut self, tokens_to_consume: u32) -> bool {
        self.refill();

        let tokens_needed = tokens_to_consume as u64;
        if self.tokens >= tokens_needed {
            self.tokens = self.tokens.saturating_sub(tokens_needed);
            self.last_access = Instant::now();
            true
        } else {
            false
        }
    }

    /// Refill tokens based on elapsed time.
    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = self.last_refill.elapsed();
        let delta_secs = elapsed.as_secs();

        // Second-granularity refill: tokens per second = rate / 60
        let new_tokens = delta_secs
            .saturating_mul(self.refill_rate_per_minute)
            .saturating_div(60);

        self.tokens = self.tokens.saturating_add(new_tokens).min(self.capacity);
        self.last_refill = now;
    }

    /// Calculate seconds until next token available.
    pub fn retry_after(&self) -> u64 {
        if self.tokens >= 1 {
            0
        } else {
            // Calculate seconds needed to get 1 token: 60 seconds / tokens_per_second
            // Using ceiling division: (60 + rate - 1) / rate
            let seconds_per_token = if self.refill_rate_per_minute > 0 {
                60_u64.div_ceil(self.refill_rate_per_minute)
            } else {
                60 // 1 minute if no refill
            };
            seconds_per_token.max(1)
        }
    }

    /// Check if bucket is stale (idle for too long).
    pub fn is_stale(&self, max_idle_ms: u64) -> bool {
        self.last_access.elapsed().as_millis() as u64 > max_idle_ms
    }
}

/// Rate limiter store using DashMap for concurrent access.
///
/// Stores per-key (RPM, TPM) token bucket pairs.
pub struct RateLimiterStore {
    /// Per-key token buckets - DashMap for concurrent access
    buckets: DashMap<String, (TokenBucket, TokenBucket)>,
}

impl RateLimiterStore {
    /// Create a new RateLimiterStore.
    pub fn new() -> Self {
        Self {
            buckets: DashMap::new(),
        }
    }

    /// Check and consume tokens for RPM and TPM.
    ///
    /// Returns Err(KeyError::RateLimited { retry_after }) if rate limited.
    pub fn check_rate_limit(&self, key: &ApiKey, tokens: u32) -> Result<(), KeyError> {
        // Get or create entry
        let mut entry = self.buckets.entry(key.key_id.clone()).or_insert_with(|| {
            (
                TokenBucket::new(key.rpm_limit.unwrap_or(100) as u32, 0),
                TokenBucket::new(key.tpm_limit.unwrap_or(1000) as u32, 0),
            )
        });

        // Check RPM (consume 1 token per request)
        let rpm_bucket = &mut entry.value_mut().0;
        if !rpm_bucket.try_consume(1) {
            return Err(KeyError::RateLimited {
                retry_after: rpm_bucket.retry_after(),
            });
        }

        // Check TPM (consume tokens for this request)
        let tpm_bucket = &mut entry.value_mut().1;
        if !tpm_bucket.try_consume(tokens) {
            return Err(KeyError::RateLimited {
                retry_after: tpm_bucket.retry_after(),
            });
        }

        Ok(())
    }

    /// Invalidate rate limiter for a key (call on key revocation).
    pub fn invalidate(&self, key_id: &str) {
        self.buckets.remove(key_id);
    }

    /// Cleanup stale buckets to prevent memory growth.
    ///
    /// - Removes buckets idle longer than max_idle_ms
    /// - If still over max_size, removes oldest entries
    pub fn cleanup_stale_buckets(&self, max_idle_ms: u64, max_size: usize) {
        // Remove stale entries
        let stale_keys: Vec<String> = self
            .buckets
            .iter()
            .filter(|entry| entry.value().0.is_stale(max_idle_ms))
            .map(|entry| entry.key().clone())
            .collect();

        for key in stale_keys {
            self.buckets.remove(&key);
        }

        // If still over max_size, remove oldest entries
        if self.buckets.len() > max_size {
            let mut buckets: Vec<_> = self
                .buckets
                .iter()
                .map(|entry| (entry.key().clone(), entry.value().0.last_access))
                .collect();

            buckets.sort_by_key(|(_, access)| *access);

            let to_remove = self.buckets.len() - max_size;
            for (key, _) in buckets.into_iter().take(to_remove) {
                self.buckets.remove(&key);
            }
        }
    }

    /// Get number of active buckets (for monitoring).
    pub fn len(&self) -> usize {
        self.buckets.len()
    }

    /// Check if store is empty.
    pub fn is_empty(&self) -> bool {
        self.buckets.is_empty()
    }
}

impl Default for RateLimiterStore {
    fn default() -> Self {
        Self::new()
    }
}

// Legacy alias for backwards compatibility during migration
#[deprecated(since = "0.1.0", note = "Use RateLimiterStore instead")]
pub type KeyRateLimiter = RateLimiterStore;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_bucket_basic() {
        let mut bucket = TokenBucket::new(10, 10);
        assert!(bucket.try_consume(1)); // 10 - 1 = 9
        assert!(bucket.try_consume(1)); // 9 - 1 = 8
        assert!(bucket.try_consume(5)); // 8 - 5 = 3
        assert_eq!(bucket.tokens, 3);
    }

    #[test]
    fn test_token_bucket_exhausted() {
        let mut bucket = TokenBucket::new(3, 0); // No refill
        assert!(bucket.try_consume(1));
        assert!(bucket.try_consume(1));
        assert!(bucket.try_consume(1));
        assert!(!bucket.try_consume(1)); // Exhausted
    }

    #[test]
    fn test_token_bucket_refill() {
        let mut bucket = TokenBucket::new(10, 60); // Full refill in 1 minute
                                                   // Simulate time passing by consuming and checking
        assert!(bucket.try_consume(10)); // Exhaust bucket
        assert!(!bucket.try_consume(1));
        // Without actual time passing, won't refill
        assert_eq!(bucket.retry_after(), 1);
    }

    #[test]
    fn test_rate_limiter_store_new() {
        let store = RateLimiterStore::new();
        assert!(store.is_empty());
    }

    #[test]
    fn test_rate_limiter_store_check() {
        use crate::keys::KeyType;

        let store = RateLimiterStore::new();
        let key = ApiKey {
            key_id: "test-key".to_string(),
            key_hash: vec![],
            key_prefix: "sk-qr-tes".to_string(),
            team_id: None,
            budget_limit: 1000,
            rpm_limit: Some(5),
            tpm_limit: Some(100),
            created_at: 0,
            expires_at: None,
            revoked: false,
            revoked_at: None,
            revoked_by: None,
            revocation_reason: None,
            key_type: KeyType::Default,
            allowed_routes: None,
            auto_rotate: false,
            rotation_interval_days: None,
            description: None,
            metadata: None,
        };

        // Should allow up to RPM limit
        for _ in 0..5 {
            assert!(store.check_rate_limit(&key, 10).is_ok());
        }

        // 6th should fail
        assert!(store.check_rate_limit(&key, 10).is_err());
    }

    #[test]
    fn test_rate_limiter_invalidate() {
        use crate::keys::KeyType;

        let store = RateLimiterStore::new();
        let key = ApiKey {
            key_id: "test-key".to_string(),
            key_hash: vec![],
            key_prefix: "sk-qr-tes".to_string(),
            team_id: None,
            budget_limit: 1000,
            rpm_limit: Some(5),
            tpm_limit: None,
            created_at: 0,
            expires_at: None,
            revoked: false,
            revoked_at: None,
            revoked_by: None,
            revocation_reason: None,
            key_type: KeyType::Default,
            allowed_routes: None,
            auto_rotate: false,
            rotation_interval_days: None,
            description: None,
            metadata: None,
        };

        // Use up rate limit
        for _ in 0..5 {
            store.check_rate_limit(&key, 0).unwrap();
        }
        assert!(store.check_rate_limit(&key, 0).is_err());

        // Invalidate
        store.invalidate(&key.key_id);

        // Should be able to use again
        assert!(store.check_rate_limit(&key, 0).is_ok());
    }
}
