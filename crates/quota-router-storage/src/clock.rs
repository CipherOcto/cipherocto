//! `Clock` trait (RFC-0957-A1 §Algorithms prerequisite).
//!
//! Mission 0957-c deviation: the trait is referenced in the RFC and the
//! mission text assumes `Clock` already exists in scope ("the `Clock` trait
//! used in `lookup_active(cap_root_hash, &dyn Clock)` and `revoke(cap_root_hash,
//! &dyn Clock)` already exists from RFC-0853"). On disk, RFC-0853 does NOT
//! export a `Clock` trait. 0957-c ships the trait + a `SystemClock` impl
//! here so the registry can inject a clock source.
//!
//! Per RFC-0957-A1 R14-N4 + R15-N3 fix: the clock is injected as a parameter
//! to `lookup_active` and `revoke` so the catalog impl is unaware of
//! identity-key internals (the prior phantom `wallet.identity_key().node_clock()`
//! method had no implementation).

/// Wall-clock abstraction. Returns Unix time in milliseconds.
pub trait Clock: Send + Sync {
    /// Current Unix time in milliseconds.
    fn unix_millis(&self) -> u64;
}

/// System clock backed by `std::time::SystemTime`.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn unix_millis(&self) -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }
}

/// Fixed clock for tests and deterministic replay.
#[derive(Debug, Clone, Copy)]
pub struct FixedClock {
    /// Pinned millis-unix value.
    pub millis: u64,
}

impl FixedClock {
    #[must_use]
    pub const fn new(millis: u64) -> Self {
        Self { millis }
    }
}

impl Clock for FixedClock {
    fn unix_millis(&self) -> u64 {
        self.millis
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_clock_returns_pinned_value() {
        let c = FixedClock::new(1_700_000_000_000);
        assert_eq!(c.unix_millis(), 1_700_000_000_000);
    }

    #[test]
    fn system_clock_returns_nonzero_post_epoch() {
        let c = SystemClock;
        let ms = c.unix_millis();
        // 2021-01-01 = 1_609_459_200_000ms. Anything below that is a clock bug.
        assert!(
            ms > 1_609_459_200_000,
            "system clock should be after 2021: got {ms}"
        );
    }

    #[test]
    fn clock_is_object_safe() {
        // Compile-time check: `&dyn Clock` is the canonical signature.
        fn _accept(_c: &dyn Clock) {}
        _accept(&SystemClock);
        _accept(&FixedClock::new(0));
    }
}
