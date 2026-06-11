//! Exponential backoff with jitter for platform adapters (RFC-0850 §8.4)
//!
//! Provides a reusable retry strategy for all DOT transport adapters.
//! Based on ZeroClaw's `compute_exponential_backoff_delay` pattern.
//!
//! All arithmetic uses saturating operations to prevent overflow.

use std::time::Duration;

/// Default initial backoff duration (1 second).
pub const DEFAULT_INITIAL_BACKOFF_SECS: u64 = 1;

/// Default maximum backoff duration (120 seconds).
pub const DEFAULT_MAX_BACKOFF_SECS: u64 = 120;

/// Default maximum jitter (500 milliseconds).
pub const DEFAULT_MAX_JITTER_MS: u64 = 500;

/// Default maximum retry attempts.
pub const DEFAULT_MAX_RETRIES: u32 = 3;

/// Compute exponential backoff delay with jitter.
///
/// Formula: `min(initial * 2^attempt, max_backoff) + jitter`
///
/// - `initial_backoff_secs`: base delay for attempt 0
/// - `attempt`: current retry attempt (0-indexed)
/// - `max_backoff_secs`: upper bound on delay
/// - `jitter_ms`: random jitter added (caller provides)
///
/// Uses saturating arithmetic throughout to prevent overflow.
pub fn compute_backoff_delay(
    initial_backoff_secs: u64,
    attempt: u32,
    max_backoff_secs: u64,
    jitter_ms: u64,
) -> Duration {
    let multiplier = 1u64.checked_shl(attempt).unwrap_or(u64::MAX);
    let backoff_secs = initial_backoff_secs
        .saturating_mul(multiplier)
        .min(max_backoff_secs);
    Duration::from_secs(backoff_secs) + Duration::from_millis(jitter_ms)
}

/// Compute backoff delay with default parameters.
///
/// Uses: initial=1s, max=120s, jitter=0-500ms.
pub fn default_backoff(attempt: u32) -> Duration {
    let jitter_ms = compute_jitter(DEFAULT_MAX_JITTER_MS);
    compute_backoff_delay(
        DEFAULT_INITIAL_BACKOFF_SECS,
        attempt,
        DEFAULT_MAX_BACKOFF_SECS,
        jitter_ms,
    )
}

/// Compute deterministic jitter from pid + counter hash.
///
/// CR-L4: renamed from `simple_jitter` to `compute_jitter` for
/// naming consistency — the function is not "simple" (it uses blake3)
/// and `compute_*` matches the crate convention (`compute_backoff_delay`).
///
/// For production, callers should use a proper RNG. This provides
/// deterministic jitter for testing and consensus-safe paths.
pub fn compute_jitter(max_jitter_ms: u64) -> u64 {
    if max_jitter_ms == 0 {
        return 0;
    }
    // CR-M1: jitter varies per-attempt by mixing pid + incrementing counter
    // so thundering-herd scenarios don't synchronize retries.
    use std::sync::atomic::{AtomicU64, Ordering};
    static JITTER_COUNTER: AtomicU64 = AtomicU64::new(0);
    let seq = JITTER_COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut seed = Vec::with_capacity(16);
    seed.extend_from_slice(&(max_jitter_ms).to_be_bytes());
    seed.extend_from_slice(&std::process::id().to_be_bytes());
    seed.extend_from_slice(&seq.to_be_bytes());
    let hash = blake3::hash(&seed);
    let bytes = hash.as_bytes();
    let val = u64::from_be_bytes([
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
    ]);
    val % (max_jitter_ms + 1)
}

/// Parse a `Retry-After` header value (seconds) and compute delay.
///
/// If the header is present, uses `max(retry_after, computed_backoff)`.
/// If absent, uses computed backoff only.
pub fn retry_after_delay(retry_after_secs: Option<u64>, attempt: u32) -> Duration {
    let backoff = default_backoff(attempt);
    match retry_after_secs {
        Some(ra) => {
            let ra_duration = Duration::from_secs(ra);
            if ra_duration > backoff {
                ra_duration
            } else {
                backoff
            }
        }
        None => backoff,
    }
}

/// Retry configuration for adapter operations.
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts.
    pub max_retries: u32,
    /// Initial backoff duration in seconds.
    pub initial_backoff_secs: u64,
    /// Maximum backoff duration in seconds.
    pub max_backoff_secs: u64,
    /// Maximum jitter in milliseconds.
    pub max_jitter_ms: u64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: DEFAULT_MAX_RETRIES,
            initial_backoff_secs: DEFAULT_INITIAL_BACKOFF_SECS,
            max_backoff_secs: DEFAULT_MAX_BACKOFF_SECS,
            max_jitter_ms: DEFAULT_MAX_JITTER_MS,
        }
    }
}

impl RetryConfig {
    /// Compute the backoff delay for a given attempt.
    pub fn delay_for_attempt(&self, attempt: u32) -> Duration {
        let jitter_ms = compute_jitter(self.max_jitter_ms);
        compute_backoff_delay(
            self.initial_backoff_secs,
            attempt,
            self.max_backoff_secs,
            jitter_ms,
        )
    }

    /// Check if we should retry after `attempt` failed attempts.
    ///
    /// `attempt=0` means "the first attempt has not yet been made";
    /// `should_retry(0)` returning true permits up to `max_retries` total
    /// tries. Total attempts = 1 + max_retries.
    ///
    /// Example: with `max_retries=3`, `should_retry(0)` returns true
    /// (allow first attempt), `should_retry(1)` true (first retry),
    /// `should_retry(2)` true (second retry), `should_retry(3)` false (stop).
    pub fn should_retry(&self, attempt: u32) -> bool {
        attempt < self.max_retries
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backoff_increases() {
        let d0 = compute_backoff_delay(1, 0, 120, 0);
        let d1 = compute_backoff_delay(1, 1, 120, 0);
        let d2 = compute_backoff_delay(1, 2, 120, 0);
        assert!(d0 < d1);
        assert!(d1 < d2);
    }

    #[test]
    fn test_backoff_capped() {
        let d = compute_backoff_delay(1, 100, 120, 0);
        assert!(d <= Duration::from_secs(120));
    }

    #[test]
    fn test_backoff_saturating() {
        // Should not panic even with large attempt
        let d = compute_backoff_delay(1, 63, 120, 0);
        assert!(d <= Duration::from_secs(120));
    }

    #[test]
    fn test_jitter_adds_time() {
        let without = compute_backoff_delay(1, 0, 120, 0);
        let with = compute_backoff_delay(1, 0, 120, 500);
        assert!(with >= without);
    }

    #[test]
    fn test_compute_jitter_zero() {
        assert_eq!(compute_jitter(0), 0);
    }

    #[test]
    fn test_compute_jitter_bounded() {
        for _ in 0..100 {
            let j = compute_jitter(500);
            assert!(j <= 500);
        }
    }

    #[test]
    fn test_retry_config_default() {
        let cfg = RetryConfig::default();
        assert_eq!(cfg.max_retries, 3);
        assert!(cfg.should_retry(0));
        assert!(cfg.should_retry(2));
        assert!(!cfg.should_retry(3));
    }

    #[test]
    fn test_retry_after_delay_uses_max() {
        // Retry-After: 200s > default backoff at attempt 0 (1s)
        let d = retry_after_delay(Some(200), 0);
        assert_eq!(d, Duration::from_secs(200));
    }

    #[test]
    fn test_retry_after_delay_falls_back() {
        // Retry-After: 0.5s < default backoff at attempt 2 (4s)
        let d = retry_after_delay(Some(0), 2);
        assert!(d >= Duration::from_secs(4));
    }
}
