// RFC-0957-A1 §Future Work F2 — Catalog GC.
//
// Sweep Revoked/Expired rows older than the configured retention window
// (default: 30 days). The sweep is policy-based and configurable via
// `HolderRegistry::set_retention_days(u32)` (deferred trait extension;
// this mission ships the policy struct + standalone sweep function).
//
// GC invariants:
//   - Only rows with `revoked_at_millis_unix` set OR `ttl_millis_unix < now`
//     are eligible for sweep.
//   - The retention window is `revoked_at_millis_unix + retention_millis`:
//     older than that → swept; newer → preserved.
//   - Perpetual records (`ttl_millis_unix == 0` + `revoked_at_millis_unix == None`)
//     are NEVER swept (they have no expiry).
//
// Scope of this mission:
//   - `GcPolicy` struct (retention_days + max_sweep_count)
//   - `sweep_eligible` predicate (pure function, easy to unit test)
//   - `GcReport` summary (swept_count, preserved_count)
//   - TV F2: 31-day-old revoked record → swept; 29-day-old → preserved
//
// Out of mission scope:
//   - The `HolderRegistry::set_retention_days` trait extension (deferred to
//     a follow-up to avoid wire-breaking changes; sweeps are policy-driven
//     for now)
//   - Storage adapter implementation (mission 0862 substrate)

/// Default retention window: 30 days (RFC-0957-A1 §F2).
pub const DEFAULT_RETENTION_DAYS: u32 = 30;

/// Maximum records swept per sweep call (bounded to prevent OOM on large
/// registries). Operators can re-run if exceeded.
pub const DEFAULT_MAX_SWEEP_COUNT: usize = 10_000;

/// GC policy — retention + sweep bound.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GcPolicy {
    /// Retention in days; rows older than this AND eligible are swept.
    pub retention_days: u32,
    /// Maximum rows swept per call.
    pub max_sweep_count: usize,
}

impl Default for GcPolicy {
    fn default() -> Self {
        Self {
            retention_days: DEFAULT_RETENTION_DAYS,
            max_sweep_count: DEFAULT_MAX_SWEEP_COUNT,
        }
    }
}

/// Minimal projection of a `HolderRecord` for GC decisions. The full
/// `HolderRecord` lives in `quota-router-storage` (mission 0957-c); we
/// extract the GC-relevant fields to avoid the dep inversion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GcCandidate {
    pub cap_root_hash: [u8; 32],
    /// `ttl_millis_unix` — 0 means perpetual (never swept on TTL).
    pub ttl_millis_unix: u64,
    /// `revoked_at_millis_unix` — None means not revoked.
    pub revoked_at_millis_unix: Option<u64>,
}

/// Result of a GC sweep.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GcReport {
    /// Number of records swept.
    pub swept_count: usize,
    /// Number of records preserved (eligible but within retention window).
    pub preserved_count: usize,
    /// Number of records skipped because they're perpetual.
    pub skipped_perpetual_count: usize,
}

/// Convert retention_days to milliseconds.
fn retention_millis(policy: &GcPolicy) -> u64 {
    u64::from(policy.retention_days).saturating_mul(86_400_000)
}

/// Determine whether a `GcCandidate` is eligible for sweep under the given
/// policy at `now_millis_unix`.
///
/// Returns `true` iff the candidate is:
///   1. Revoked AND older than retention window, OR
///   2. Expired (ttl < now) AND older than retention window.
pub fn sweep_eligible(candidate: &GcCandidate, policy: &GcPolicy, now_millis_unix: u64) -> bool {
    let ret_ms = retention_millis(policy);

    // Perpetual records are NEVER swept.
    if candidate.ttl_millis_unix == 0 && candidate.revoked_at_millis_unix.is_none() {
        return false;
    }

    // Revoked records: check retention window since revocation.
    if let Some(revoked_at) = candidate.revoked_at_millis_unix {
        return now_millis_unix.saturating_sub(revoked_at) >= ret_ms;
    }

    // Expired (non-perpetual) records: check retention window since expiry.
    // TTL=0 + revoked=None is perpetual (handled above); otherwise TTL=0
    // means "no expiry timestamp" and the record cannot be age-swept.
    if candidate.ttl_millis_unix > 0 && now_millis_unix >= candidate.ttl_millis_unix {
        let expired_at = candidate.ttl_millis_unix;
        return now_millis_unix.saturating_sub(expired_at) >= ret_ms;
    }

    false
}

/// Sweep a slice of candidates, returning a `GcReport` and the indices of
/// the swept records.
///
/// The caller is responsible for actually removing the records from the
/// registry; this function is a pure decision oracle.
pub fn sweep_candidates(
    candidates: &[GcCandidate],
    policy: &GcPolicy,
    now_millis_unix: u64,
) -> (GcReport, Vec<usize>) {
    let mut swept_indices = Vec::new();
    let mut swept_count = 0;
    let mut preserved_count = 0;
    let mut skipped_perpetual_count = 0;

    for (idx, c) in candidates.iter().enumerate() {
        if c.ttl_millis_unix == 0 && c.revoked_at_millis_unix.is_none() {
            skipped_perpetual_count += 1;
            continue;
        }
        if sweep_eligible(c, policy, now_millis_unix) {
            if swept_count < policy.max_sweep_count {
                swept_indices.push(idx);
                swept_count += 1;
            }
        } else {
            preserved_count += 1;
        }
    }

    (
        GcReport {
            swept_count,
            preserved_count,
            skipped_perpetual_count,
        },
        swept_indices,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn revoked_at(ts: u64) -> GcCandidate {
        GcCandidate {
            cap_root_hash: [0x33; 32],
            ttl_millis_unix: 1_700_000_000_000,
            revoked_at_millis_unix: Some(ts),
        }
    }

    #[test]
    fn revoked_31_days_old_is_eligible() {
        // TV F2: revoked 31 days ago → eligible for sweep.
        let now = 1_700_000_000_000;
        let revoked_31d_ago = now - 31 * 86_400_000;
        let c = revoked_at(revoked_31d_ago);
        assert!(sweep_eligible(&c, &GcPolicy::default(), now));
    }

    #[test]
    fn revoked_29_days_old_is_preserved() {
        // TV F2: revoked 29 days ago → preserved.
        let now = 1_700_000_000_000;
        let revoked_29d_ago = now - 29 * 86_400_000;
        let c = revoked_at(revoked_29d_ago);
        assert!(!sweep_eligible(&c, &GcPolicy::default(), now));
    }

    #[test]
    fn perpetual_record_never_swept() {
        let c = GcCandidate {
            cap_root_hash: [0x11; 32],
            ttl_millis_unix: 0,
            revoked_at_millis_unix: None,
        };
        assert!(!sweep_eligible(&c, &GcPolicy::default(), u64::MAX));
    }

    #[test]
    fn expired_record_swept_after_retention() {
        let policy = GcPolicy::default();
        let now = 1_700_000_000_000;
        let c = GcCandidate {
            cap_root_hash: [0x22; 32],
            ttl_millis_unix: now - 31 * 86_400_000, // expired 31d ago
            revoked_at_millis_unix: None,
        };
        assert!(sweep_eligible(&c, &policy, now));
    }

    #[test]
    fn sweep_candidates_respects_max_count() {
        let policy = GcPolicy {
            retention_days: 1,
            max_sweep_count: 5,
        };
        let now = 1_700_000_000_000;
        let mut candidates = Vec::new();
        for i in 0..5u8 {
            candidates.push(revoked_at(now - 2 * 86_400_000));
            candidates.last_mut().unwrap().cap_root_hash = [i; 32];
        }
        let (report, indices) = sweep_candidates(&candidates, &policy, now);
        assert_eq!(report.swept_count, 5);
        assert_eq!(report.preserved_count, 0);
        assert_eq!(indices.len(), 5);
    }

    #[test]
    fn sweep_candidates_caps_overflow() {
        // 10 candidates, max_sweep_count = 5 → 5 swept + 5 unaccounted (overflow).
        let policy = GcPolicy {
            retention_days: 1,
            max_sweep_count: 5,
        };
        let now = 1_700_000_000_000;
        let mut candidates = Vec::new();
        for i in 0..10u8 {
            candidates.push(revoked_at(now - 2 * 86_400_000));
            candidates.last_mut().unwrap().cap_root_hash = [i; 32];
        }
        let (report, _indices) = sweep_candidates(&candidates, &policy, now);
        // Sweep budget exhausted at 5; remaining 5 are NOT preserved (they
        // were eligible). Operators must re-run.
        assert_eq!(report.swept_count, 5);
        assert_eq!(report.preserved_count, 0);
    }

    #[test]
    fn custom_retention_days() {
        let policy = GcPolicy {
            retention_days: 7,
            max_sweep_count: 100,
        };
        let now = 1_700_000_000_000;
        // 8 days old → eligible under 7-day policy.
        let c = revoked_at(now - 8 * 86_400_000);
        assert!(sweep_eligible(&c, &policy, now));
        // 6 days old → preserved under 7-day policy.
        let c = revoked_at(now - 6 * 86_400_000);
        assert!(!sweep_eligible(&c, &policy, now));
    }

    #[test]
    fn default_policy_is_30_days() {
        assert_eq!(GcPolicy::default().retention_days, DEFAULT_RETENTION_DAYS);
        assert_eq!(DEFAULT_RETENTION_DAYS, 30);
    }
}
