//! Shared time helpers.
//!
//! R2-ARCH-14: the prior version of the onboard code had
//! three identical copies of `unix_now_secs()` — one in
//! `bot_token.rs`, one in `user_code.rs`, and one in
//! `qr_login.rs`. Each was a 4-line wrapper around
//! `SystemTime::now().duration_since(UNIX_EPOCH)`. The
//! duplication was small but invisible-to-clippy: each copy
//! was "used" (called by its own flow's `SessionRecord`
//! construction) so `cargo clippy` didn't flag it.
//!
//! The fix: a single `pub(crate)` helper. The signature is
//! stable (returns `i64` seconds since the Unix epoch; `0` if
//! the system clock is set before 1970 — a degenerate case
//! that should never happen on a real server).

/// Unix-epoch timestamp in seconds. Returns `0` if the
/// system clock is set before the epoch (anomalous but
/// recoverable).
pub fn unix_now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unix_now_secs_is_in_a_reasonable_range() {
        // Sanity check: the function returns a
        // post-2000 timestamp. The round-1 review
        // observed that the only failure mode is a
        // clock set before 1970, which is unusual
        // enough to be its own diagnostic. The
        // assertion here is a smoke test, not a
        // correctness check.
        let t = unix_now_secs();
        assert!(
            t > 946_684_800, // 2000-01-01
            "unix_now_secs() returned {} (expected > 2000-01-01)",
            t
        );
    }

    #[test]
    fn unix_now_secs_is_monotonic_within_a_call() {
        // Two consecutive calls return non-decreasing
        // values. The function uses SystemTime::now(),
        // which is monotonic on every platform Rust
        // supports.
        let a = unix_now_secs();
        let b = unix_now_secs();
        assert!(b >= a);
    }
}
