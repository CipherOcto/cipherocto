# Mission 0903-e: Per-Key Rate Limiting

> **For Claude:** Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Enforce per-key RPM (requests per minute) and TPM (tokens per minute) limits.

**Status:** ✅ IMPLEMENTED (2026-04-14) - RFC-0903 TokenBucket algorithm

**Architecture:** TokenBucket algorithm per RFC-0903 §Rate Limiting Algorithm using DashMap for concurrent access.

---

## Task 1: Add per-key rate limit tracking

**Files:**
- Create: `crates/quota-router-core/src/key_rate_limiter.rs`

**Step 1: Create key rate limiter**

```rust
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use std::time::{SystemTime, UNIX_EPOCH};

/// Tracks rate limit usage per key
pub struct KeyRateLimiter {
    /// key_id -> (rpm_count, window_start)
    rpm_tracker: Arc<RwLock<HashMap<String, (u32, u64)>>>,
    /// key_id -> (tpm_count, window_start)
    tpm_tracker: Arc<RwLock<HashMap<String, (u64, u64)>>>,
}

impl KeyRateLimiter {
    pub fn new() -> Self {
        Self {
            rpm_tracker: Arc::new(RwLock::new(HashMap::new())),
            tpm_tracker: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Check and record RPM
    pub async fn check_rpm(&self, key_id: &str, limit: Option<i32>) -> Result<(), KeyError> {
        let Some(limit) = limit else { return Ok(()); };
        let limit = limit as u32;

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let mut tracker = self.rpm_tracker.write().await;

        if let Some((count, window_start)) = tracker.get(key_id) {
            if now - window_start < 60 {
                if *count >= limit {
                    return Err(KeyError::RateLimitExceeded("RPM limit exceeded".to_string()));
                }
                tracker.insert(key_id.to_string(), (*count + 1, window_start));
            } else {
                // Window expired, reset
                tracker.insert(key_id.to_string(), (1, now));
            }
        } else {
            tracker.insert(key_id.to_string(), (1, now));
        }

        Ok(())
    }

    /// Check and record TPM
    pub async fn check_tpm(&self, key_id: &str, tokens: u32, limit: Option<i32>) -> Result<(), KeyError> {
        let Some(limit) = limit else { return Ok(()) };
        let limit = limit as u64;

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let mut tracker = self.tpm_tracker.write().await;

        if let Some((count, window_start)) = tracker.get(key_id) {
            if now - window_start < 60 {
                let new_count = *count + tokens as u64;
                if new_count >= limit {
                    return Err(KeyError::RateLimitExceeded("TPM limit exceeded".to_string()));
                }
                tracker.insert(key_id.to_string(), (new_count, window_start));
            } else {
                tracker.insert(key_id.to_string(), (tokens as u64, now));
            }
        } else {
            tracker.insert(key_id.to_string(), (tokens as u64, now));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_rpm_limit() {
        let limiter = KeyRateLimiter::new();

        // Should allow up to limit
        for _ in 0..10 {
            limiter.check_rpm("key1", Some(10)).await.unwrap();
        }

        // 11th should fail
        let result = limiter.check_rpm("key1", Some(10)).await;
        assert!(result.is_err());
    }
}
```

**Step 2: Add to lib.rs exports**

**Step 3: Test**

**Step 4: Commit**

---

## Task 2: Integrate with key validation

**Files:**
- Modify: `crates/quota-router-core/src/middleware.rs`

**Step 1: Add rate limit check**

```rust
use crate::key_rate_limiter::KeyRateLimiter;

pub struct KeyMiddleware<S: KeyStorage> {
    storage: Arc<S>,
    rate_limiter: Arc<KeyRateLimiter>,
}

impl<S: KeyStorage> KeyMiddleware<S> {
    /// Check rate limits for key
    pub async fn check_rate_limits(&self, key: &ApiKey) -> Result<(), KeyError> {
        // Check RPM
        self.rate_limiter.check_rpm(&key.key_id, key.rpm_limit).await?;

        // TPM is checked after tokens are known (in request processing)
        Ok(())
    }
}
```

**Step 2: Test**

**Step 3: Commit**

---

## Implementation Notes (2026-04-14)

**Actual Implementation:** RFC-0903 TokenBucket algorithm supersedes the old HashMap design above.

**Files created/modified:**
- `crates/quota-router-core/src/key_rate_limiter.rs` - TokenBucket + RateLimiterStore with DashMap
- `crates/quota-router-core/src/middleware.rs` - check_rate_limits() using RateLimiterStore
- `crates/quota-router-core/src/lib.rs` - exports RateLimiterStore
- `crates/quota-router-core/src/keys/errors.rs` - added RateLimited error variant
- `crates/quota-router-core/Cargo.toml` - added dashmap, instant dependencies

**Key features:**
- TokenBucket with continuous refill (tokens per second = rate / 60)
- DashMap for concurrent per-key storage (replaces HashMap + RwLock)
- Instant for monotonic time (immune to clock adjustments)
- Per-key (RPM, TPM) token bucket pairs
- cleanup_stale_buckets() for memory management
- Determinism disclaimer: rate limiting is NOT deterministic across nodes**
