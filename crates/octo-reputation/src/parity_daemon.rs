//! Parity daemon for the dual-read retirement gate — mission 0968-b Phase D.
//!
//! Wraps `parity::compute_parity_report` with stateful roll-up across
//! the 24h dual-read window, exposed in 1-hour buckets. The daemon is
//! the entity that decides whether the legacy stores can be retired
//! once 24 consecutive 1-hour buckets clear the gate.
//!
//! ## Gate (per mission 0968-b Phase D acceptance)
//!
//! 1. **Per-adapter sustained parity `parity_score >= 0.999`** across
//!    24 CONSECUTIVE 1-hour rolling buckets with `bucket_total >= 10`
//!    each. Sparse-traffic buckets break the chain.
//! 2. Operator-supplied `GovernanceProof` (mission 0968-b retirement
//!    declaration) confirms the elapsed window.
//! 3. **PARITY_GATE_DEADLINE_UNIX = now + 90 days** auto-retires the
//!    legacy stores regardless of parity score, provided
//!    `INVALID_TRIPLES / total_triples < 1e-6`. Hard deadline enforced
//!    here as `parity_gate_deadline_unix()`.
//! 4. `quota_router_reputation_freeze_cutover: bool` (default `false`)
//!    suppresses BOTH the deadline auto-retirement AND any
//!    operator-initiated retirement while set.
//!
//! The state in this daemon is pure — it does not depend on a database.
//! Persistence of bucket counters is the runtime's responsibility
//! (e.g., Prometheus counters `reputation_parity_match_count` and
//! `reputation_parity_total_count`).

use std::collections::BTreeMap;

use crate::parity::PARITY_GATE_DEADLINE_DAYS;

/// One hour's worth of parity observations, summed across all
/// `(did, kind, layer)` triples the runtime reported.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ParityBucket {
    /// Matches between legacy + canonical.
    pub match_count: u64,
    /// Mismatches between legacy + canonical.
    pub mismatch_count: u64,
    /// Invalid triples (NaN `score_ewma`, unsupported `SignalKind`,
    /// length-mismatched BLOB). Excluded from the parity denominator
    /// per Phase D Round 6 I13.
    pub invalid_count: u64,
}

impl ParityBucket {
    /// Total triples observed in this bucket (matches + mismatches +
    /// invalid). Invalid triples are surfaced separately and excluded
    /// from the parity denominator via `valid_total`.
    #[must_use]
    pub fn total(&self) -> u64 {
        self.match_count + self.mismatch_count + self.invalid_count
    }

    /// Valid total (matches + mismatches), the parity denominator.
    #[must_use]
    pub fn valid_total(&self) -> u64 {
        self.match_count + self.mismatch_count
    }

    /// Parity score = match / valid_total. `None` when valid_total == 0
    /// (no valid observations in the bucket).
    #[must_use]
    pub fn parity_score(&self) -> Option<f64> {
        let total = self.valid_total();
        if total == 0 {
            None
        } else {
            Some(self.match_count as f64 / total as f64)
        }
    }

    /// True iff this bucket passes the 1e-6 invalid-share guard and the
    /// parity score threshold (`>= 0.999`).
    #[must_use]
    pub fn passes_threshold(&self, threshold: f64) -> bool {
        let total = self.total();
        if total == 0 {
            return false;
        }
        // Invalid-share guard: the invalid triples must be < 1e-6 of
        // the total observed traffic in this bucket. Persistent
        // malformed inputs are excluded from the parity denominator
        // but their presence still blocks the gate.
        if (self.invalid_count as f64) / (total as f64) >= 1e-6 {
            return false;
        }
        match self.parity_score() {
            Some(score) => score >= threshold,
            None => false,
        }
    }
}

/// Outer state of the daemon: an ordered map of bucket-unix → bucket,
/// keyed by `now_unix / 3600`.
#[derive(Debug, Clone, Default)]
pub struct ParityDaemonState {
    pub buckets: BTreeMap<u64, ParityBucket>,
    pub freeze: bool,
}

impl ParityDaemonState {
    /// Compute the bucket index for a `now_unix` value.
    #[must_use]
    pub fn bucket_index(now_unix: u64) -> u64 {
        now_unix / 3600
    }

    /// Record one match/mismatch/invalid observation into the
    /// bucket for `now_unix`.
    pub fn record_match(&mut self, now_unix: u64) {
        let idx = Self::bucket_index(now_unix);
        self.buckets.entry(idx).or_default().match_count += 1;
    }

    pub fn record_mismatch(&mut self, now_unix: u64) {
        let idx = Self::bucket_index(now_unix);
        self.buckets.entry(idx).or_default().mismatch_count += 1;
    }

    pub fn record_invalid(&mut self, now_unix: u64) {
        let idx = Self::bucket_index(now_unix);
        self.buckets.entry(idx).or_default().invalid_count += 1;
    }

    /// Set the operator-supplied freeze flag. While set, both the
    /// deadline auto-retirement AND any operator-initiated retirement
    /// are suppressed.
    pub fn set_freeze(&mut self, freeze: bool) {
        self.freeze = freeze;
    }

    /// Number of consecutive trailing buckets (oldest → newest) that
    /// each clear the gate with `bucket_total >= MIN_BUCKET_TOTAL`.
    /// A gap in the bucket-index sequence (no observations during an
    /// hour) breaks the run: per mission 0968-b Phase D the gate
    /// requires 24 CONSECUTIVE 1-hour buckets.
    #[must_use]
    pub fn consecutive_passing_buckets(&self, threshold: f64) -> u64 {
        // Walk newest → oldest in strictly-increasing key order,
        // breaking when (a) the current bucket doesn't pass the gate,
        // OR (b) the bucket index is not exactly one more than the
        // previous (gap detected).
        let mut count: u64 = 0;
        let mut prev: Option<u64> = None;
        for (&idx, b) in self.buckets.iter().rev() {
            // (b) Gap check first: a missing intermediate hour breaks
            // consecutive without affecting the current bucket's
            // pass status.
            if let Some(p) = prev {
                if p != idx + 1 {
                    break;
                }
            }
            // (a) Gate check: must have traffic + threshold + invalid-share.
            if b.total() < MIN_BUCKET_TOTAL || !b.passes_threshold(threshold) {
                break;
            }
            count = count
                .checked_add(1)
                .expect("consecutive bucket count fits in u64 for 24h+ windows");
            prev = Some(idx);
        }
        count
    }

    /// Returns `true` iff at least `required` consecutive buckets
    /// (default = 24) pass the parity threshold. Frozen daemons
    /// fail-closed under any retirement check.
    #[must_use]
    pub fn retirement_eligible(&self, required: u64, threshold: f64) -> bool {
        if self.freeze {
            return false;
        }
        self.consecutive_passing_buckets(threshold) >= required
    }

    /// Auto-retire eligibility check. Compares `now_unix` against the
    /// caller-supplied `deadline_unix`: the gate opens only when
    /// `now_unix >= deadline_unix`. Per mission 0968-b Phase D: the
    /// 90-day deadline after which the gate auto-retires the legacy
    /// stores regardless of parity score, provided
    /// `INVALID_TRIPLES / total_triples < 1e-6` over the whole window.
    ///
    /// Three yields:
    /// - `Ok(true)`: caller-supplied wall-clock is past the deadline,
    ///   freeze is off, the invalid-share guard passes.
    /// - `Ok(false)`: pre-deadline, OR frozen, OR no traffic observed.
    /// - `Err(msg)`: freeze off AND post-deadline AND invalid-share
    ///   guard fails.
    ///
    /// Both timestamps are caller-supplied; the daemon does NOT read
    /// `SystemTime::now()`. Two replicas with skewed clocks reach the
    /// same decision given the same buckets and the same pair of
    /// (now, deadline) values.
    pub fn deadline_eligible(
        &self,
        now_unix: u64,
        deadline_unix: u64,
    ) -> Result<bool, &'static str> {
        if self.freeze {
            return Ok(false);
        }
        if now_unix < deadline_unix {
            return Ok(false);
        }
        let (mut invalid, mut total) = (0u64, 0u64);
        for b in self.buckets.values() {
            invalid += b.invalid_count;
            total += b.total();
        }
        if total == 0 {
            return Ok(false); // no data; gate stays closed
        }
        // Invalid-share gate (1e-6): IEEE-754-exact literal so the
        // comparison is bit-deterministic between replicas.
        if (invalid as f64) / (total as f64) >= 1e-6 {
            return Err("invalid-share exceeds 1e-6");
        }
        Ok(true)
    }

    /// Aggregate metrics across the entire window. Used by the runtime
    /// to populate Prometheus counters `reputation_parity_match_count`
    /// and `reputation_parity_total_count`.
    #[must_use]
    pub fn aggregate(&self) -> ParityBucket {
        let mut out = ParityBucket::default();
        for b in self.buckets.values() {
            out.match_count += b.match_count;
            out.mismatch_count += b.mismatch_count;
            out.invalid_count += b.invalid_count;
        }
        out
    }
}

/// Constant: required consecutive passing buckets (24 hours × 1 hour
/// buckets). Mission 0968-b Phase D acceptance.
pub const REQUIRED_CONSECUTIVE_BUCKETS: u64 = 24;

/// Threshold below which a bucket is too sparse to count.
pub const MIN_BUCKET_TOTAL: u64 = 10;

/// Return the unix-seconds deadline after which the auto-retirement
/// path opens. Currently `now + PARITY_GATE_DEADLINE_DAYS * 86400`,
/// computed at daemon construction time. The runtime computes this
/// from its trusted clock and passes the result into
/// `ParityDaemonState::deadline_eligible(deadline_unix)`.
#[must_use]
pub fn parity_deadline_unix_from_epoch(epoch_unix: u64) -> u64 {
    epoch_unix.saturating_add(PARITY_GATE_DEADLINE_DAYS * 86_400)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_state_has_no_passing_buckets() {
        let s = ParityDaemonState::default();
        assert_eq!(s.consecutive_passing_buckets(0.999), 0);
        assert!(!s.retirement_eligible(24, 0.999));
    }

    #[test]
    fn sparse_buckets_do_not_count() {
        // Each bucket has only 1 observation; below MIN_BUCKET_TOTAL = 10.
        let mut s = ParityDaemonState::default();
        for i in 0..10u64 {
            s.record_match(i * 3600);
        }
        assert_eq!(s.consecutive_passing_buckets(0.999), 0);
    }

    #[test]
    fn twenty_four_passing_buckets_unlock_retirement() {
        let mut s = ParityDaemonState::default();
        let now = 1_000_000_000u64;
        for hour in 0..24u64 {
            for _ in 0..15 {
                s.record_match(now - hour * 3600);
            }
        }
        assert_eq!(s.consecutive_passing_buckets(0.999), 24);
        assert!(s.retirement_eligible(REQUIRED_CONSECUTIVE_BUCKETS, 0.999));
    }

    #[test]
    fn mismatching_bucket_breaks_consecutive_run() {
        let mut s = ParityDaemonState::default();
        let now = 1_000_000_000u64;
        for hour in 0..24u64 {
            let ts = now - hour * 3600;
            for _ in 0..9 {
                s.record_match(ts);
            }
            // hour 0 has one mismatch → parity_score drops below 0.999
            if hour == 0 {
                for _ in 0..2 {
                    s.record_mismatch(ts);
                }
            } else {
                for _ in 0..6 {
                    s.record_match(ts);
                }
            }
        }
        // Latest bucket (hour 0) has 9+2=11 obs, parity = 9/11 ≈ 0.818 — fails
        // 0.999. Consecutive run is broken (0 passing buckets).
        let score = s
            .buckets
            .get(&ParityDaemonState::bucket_index(now))
            .unwrap()
            .parity_score()
            .unwrap();
        assert!(score < 0.999, "score: {score}");
        assert_eq!(s.consecutive_passing_buckets(0.999), 0);
    }

    #[test]
    fn invalid_share_above_threshold_blocks_bucket() {
        let mut b = ParityBucket::default();
        for _ in 0..9_000 {
            b.match_count += 1;
        }
        b.invalid_count = 1; // 1 / 9_001 ≈ 1.1e-4 > 1e-6
        assert!(!b.passes_threshold(0.999));
    }

    #[test]
    fn freeze_suppresses_retirement_eligibility() {
        let mut s = ParityDaemonState::default();
        for hour in 0..24u64 {
            for _ in 0..15 {
                s.record_match(hour * 3600);
            }
        }
        s.set_freeze(true);
        assert!(!s.retirement_eligible(REQUIRED_CONSECUTIVE_BUCKETS, 0.999));
        s.set_freeze(false);
        assert!(s.retirement_eligible(REQUIRED_CONSECUTIVE_BUCKETS, 0.999));
    }

    #[test]
    fn deadline_eligible_after_90_days_when_no_invalid_dominance() {
        let mut s = ParityDaemonState::default();
        for hour in 0..24u64 {
            for _ in 0..15 {
                s.record_match(hour * 3600);
            }
        }
        // Caller-supplied (now, deadline). 90 days + 1 hour past epoch.
        let now = 90 * 86_400 + 3_600;
        let deadline = 90 * 86_400;
        let r = s.deadline_eligible(now, deadline).unwrap();
        assert!(r);
    }

    #[test]
    fn deadline_blocked_when_invalid_share_too_high() {
        let mut s = ParityDaemonState::default();
        for hour in 0..24u64 {
            for _ in 0..999_999 {
                s.record_match(hour * 3600);
            }
            s.record_invalid(hour * 3600);
        }
        for hour in 0..24u64 {
            s.record_invalid(hour * 3600);
        }
        // The exact threshold check is `>= 1e-6`. With 2 invalid per
        // bucket × 24 buckets = 48 invalid / 24 million+ total ≈ 2e-6 → exceeds.
        let now = 90 * 86_400 + 1;
        let deadline = 90 * 86_400;
        let r = s.deadline_eligible(now, deadline);
        assert!(r.is_err(), "expected invalid-share error, got {r:?}");
    }

    #[test]
    fn deadline_pre_now_returns_false() {
        // now < deadline → gate stays closed regardless of buckets.
        let mut s = ParityDaemonState::default();
        for hour in 0..24u64 {
            for _ in 0..15 {
                s.record_match(hour * 3600);
            }
        }
        let now = 1_000;
        let deadline = 90 * 86_400;
        assert!(!s.deadline_eligible(now, deadline).unwrap());
    }

    #[test]
    fn freeze_suppresses_deadline_eligibility() {
        let mut s = ParityDaemonState::default();
        for hour in 0..24u64 {
            for _ in 0..15 {
                s.record_match(hour * 3600);
            }
        }
        s.set_freeze(true);
        // Frozen daemons must fail closed under ANY retirement check,
        // including the deadline auto-retire path.
        let now = 90 * 86_400 + 1;
        let deadline = 90 * 86_400;
        assert!(!s.deadline_eligible(now, deadline).unwrap());
        s.set_freeze(false);
        assert!(s.deadline_eligible(now, deadline).unwrap());
    }

    #[test]
    fn gap_in_consecutive_buckets_breaks_run() {
        let mut s = ParityDaemonState::default();
        // 24 buckets but with an intentional gap at index 23 (newest).
        // Indexes 0..=22 inclusive = 23 consecutive passing buckets.
        for hour in 0..=22u64 {
            for _ in 0..15 {
                s.record_match(hour * 3600);
            }
        }
        // Skip hour 23 entirely.
        // Then add hour 24 - but with mismatches so it doesn't pass.
        for _ in 0..10 {
            s.record_match(24 * 3600);
        }
        for _ in 0..5 {
            s.record_mismatch(24 * 3600);
        }
        // The reverse iter starts at hour 24 (latest key), sees it
        // fails the gate, breaks immediately. Count = 0.
        assert_eq!(s.consecutive_passing_buckets(0.999), 0);
        // Now skip hour 24 entirely and inspect: only 0..=22 are
        // present. The walk should observe 23 consecutive buckets.
        let mut s2 = ParityDaemonState::default();
        for hour in 0..=22u64 {
            for _ in 0..15 {
                s2.record_match(hour * 3600);
            }
        }
        assert_eq!(s2.consecutive_passing_buckets(0.999), 23);
        // Add a gap then a passing bucket (hour 24). The gap breaks
        // the run; count resets to 1 (just hour 24).
        for _ in 0..15 {
            s2.record_match(24 * 3600);
        }
        assert_eq!(s2.consecutive_passing_buckets(0.999), 1);
    }

    #[test]
    fn aggregate_sums_buckets() {
        let mut s = ParityDaemonState::default();
        for hour in 0..3u64 {
            for _ in 0..5 {
                s.record_match(hour * 3600);
            }
            s.record_mismatch(hour * 3600);
        }
        let agg = s.aggregate();
        assert_eq!(agg.match_count, 15);
        assert_eq!(agg.mismatch_count, 3);
        assert_eq!(agg.total(), 18);
    }
}
