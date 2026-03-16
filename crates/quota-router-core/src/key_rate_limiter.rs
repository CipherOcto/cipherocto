// Per-key rate limiting - RPM/TPM enforcement per API key

use crate::keys::KeyError;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

/// Tracks rate limit usage per key
pub struct KeyRateLimiter {
    /// key_id -> (rpm_count, window_start)
    rpm_tracker: RwLock<HashMap<String, (u32, u64)>>,
    /// key_id -> (tpm_count, window_start)
    tpm_tracker: RwLock<HashMap<String, (u64, u64)>>,
}

impl KeyRateLimiter {
    pub fn new() -> Self {
        Self {
            rpm_tracker: RwLock::new(HashMap::new()),
            tpm_tracker: RwLock::new(HashMap::new()),
        }
    }

    /// Check and record RPM
    pub fn check_rpm(&self, key_id: &str, limit: Option<i32>) -> Result<(), KeyError> {
        let Some(limit) = limit else { return Ok(()) };
        let limit = limit as u32;

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let mut tracker = self.rpm_tracker.write();

        // Check existing entry
        if let Some(entry) = tracker.get_mut(key_id) {
            let (count, window_start) = *entry;
            if now - window_start < 60 {
                if count >= limit {
                    return Err(KeyError::RateLimited);
                }
                *entry = (count + 1, window_start);
            } else {
                // Window expired, reset
                *entry = (1, now);
            }
        } else {
            tracker.insert(key_id.to_string(), (1, now));
        }

        Ok(())
    }

    /// Check and record TPM
    pub fn check_tpm(&self, key_id: &str, tokens: u32, limit: Option<i32>) -> Result<(), KeyError> {
        let Some(limit) = limit else { return Ok(()) };
        let limit = limit as u64;

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let mut tracker = self.tpm_tracker.write();

        if let Some(entry) = tracker.get_mut(key_id) {
            let (count, window_start) = *entry;
            if now - window_start < 60 {
                let new_count = count + tokens as u64;
                if new_count > limit {
                    return Err(KeyError::RateLimited);
                }
                *entry = (new_count, window_start);
            } else {
                *entry = (tokens as u64, now);
            }
        } else {
            tracker.insert(key_id.to_string(), (tokens as u64, now));
        }

        Ok(())
    }

    /// Reset rate limits for a key (e.g., when window expires)
    pub fn reset(&self, key_id: &str) {
        self.rpm_tracker.write().remove(key_id);
        self.tpm_tracker.write().remove(key_id);
    }
}

impl Default for KeyRateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rpm_limit() {
        let limiter = KeyRateLimiter::new();

        // Should allow up to limit
        for _ in 0..10 {
            limiter.check_rpm("key1", Some(10)).unwrap();
        }

        // 11th should fail
        let result = limiter.check_rpm("key1", Some(10));
        assert!(result.is_err());
    }

    #[test]
    fn test_rpm_window_reset() {
        let limiter = KeyRateLimiter::new();

        // Should allow after window reset
        limiter.check_rpm("key2", Some(2)).unwrap();
        limiter.check_rpm("key2", Some(2)).unwrap();

        // Third should fail
        let result = limiter.check_rpm("key2", Some(2));
        assert!(result.is_err());
    }

    #[test]
    fn test_tpm_limit() {
        let limiter = KeyRateLimiter::new();

        // Should allow up to limit
        for _ in 0..5 {
            limiter.check_tpm("key3", 100, Some(500)).unwrap();
        }

        // 6th should fail (600 tokens > 500 limit)
        let result = limiter.check_tpm("key3", 100, Some(500));
        assert!(result.is_err());
    }

    #[test]
    fn test_no_limit() {
        let limiter = KeyRateLimiter::new();

        // Should always pass when no limit set
        limiter.check_rpm("key4", None).unwrap();
        limiter.check_tpm("key4", 1000, None).unwrap();
    }
}
