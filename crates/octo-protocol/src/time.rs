//! Clock injection (RFC-0871 §Algorithms Clock Injection).
//!
//! Both sender (TTL computation) and receiver (TTL enforcement) MUST obtain
//! current time via an injected `Clock` trait, not via direct
//! `std::time::SystemTime::now()` calls. Required for byte-exact reproducibility
//! of test vectors and for deterministic simulation runs.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Wall-clock abstraction for envelope TTL computation + enforcement.
///
/// Production: `SystemClock` wraps `SystemTime`. Tests: `MockClock { now_unix_ms }`.
pub trait Clock: Send + Sync {
    /// Current time in unix milliseconds.
    fn now_unix_ms(&self) -> u64;
}

/// Production clock backed by `SystemTime`.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_unix_ms(&self) -> u64 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        u64::try_from(now.as_millis()).unwrap_or(u64::MAX)
    }
}

/// Test-only mutable clock for deterministic envelope construction + dispatch.
///
/// Thread-safe via `AtomicU64` so async dispatchers can advance time without
/// external locking.
#[derive(Debug)]
pub struct MockClock {
    now_unix_ms: AtomicU64,
}

impl MockClock {
    /// New mock clock starting at `now_unix_ms`.
    #[must_use]
    pub fn new(now_unix_ms: u64) -> Self {
        Self {
            now_unix_ms: AtomicU64::new(now_unix_ms),
        }
    }

    /// Advance the clock by `delta_ms`. Returns the new `now_unix_ms`.
    pub fn advance(&self, delta_ms: u64) -> u64 {
        self.now_unix_ms.fetch_add(delta_ms, Ordering::SeqCst) + delta_ms
    }

    /// Set the clock to `now_unix_ms`.
    pub fn set(&self, now_unix_ms: u64) {
        self.now_unix_ms.store(now_unix_ms, Ordering::SeqCst);
    }
}

impl Clock for MockClock {
    fn now_unix_ms(&self) -> u64 {
        self.now_unix_ms.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_clock_starts_and_advances() {
        let c = MockClock::new(1_000);
        assert_eq!(c.now_unix_ms(), 1_000);
        assert_eq!(c.advance(500), 1_500);
        assert_eq!(c.now_unix_ms(), 1_500);
        c.set(10_000);
        assert_eq!(c.now_unix_ms(), 10_000);
    }

    #[test]
    fn system_clock_returns_monotonic_nonzero() {
        // Sanity check: wall clock returns a non-zero value at unix-ms scale.
        let now = SystemClock.now_unix_ms();
        assert!(
            now > 1_700_000_000_000,
            "expected post-2023 timestamp; got {now}"
        );
    }
}
