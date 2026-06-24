//! Sync peer scoring (adapted from DRS RFC-0856 for sync-specific signals).
//!
//! The DRS (Deterministic Route Selection) scoring formula from RFC-0856 §6.1:
//! ```text
//! score = (trust × trust_w) + (bandwidth × bw_w) + (latency × lat_w)
//!       + (censorship × censor_w) - (cost × cost_w)
//! ```
//!
//! The sync engine lacks network-level data (latency classes, bandwidth classes),
//! so we adapt the formula using sync-available signals:
//!
//! ```text
//! sync_score = (freshness × freshness_w) + (liveness × liveness_w)
//!            + (reliability × reliability_w) - (penalty × penalty_w)
//! ```
//!
//! All weights are governance-controlled constants (not runtime-configurable).
//! All arithmetic is u64 saturating (no floating point, per RFC-0862 §Determinism).

use crate::state::SyncLifecycle;
use crate::types::Lsn;

/// Scoring weights for sync peer selection.
///
/// Weights must sum to 1,000,000 (1M basis points) per DRS convention.
/// These are governance-controlled constants, not runtime-configurable.
#[derive(Debug, Clone)]
pub struct ScoringWeights {
    /// Weight for LSN freshness (lower LSN = better catch-up target).
    /// Higher weight = prefer peers that are further behind.
    pub freshness: u64,
    /// Weight for peer liveness (Streaming > Connecting > Suspect).
    /// Higher weight = prefer active peers more strongly.
    pub liveness: u64,
    /// Weight for heartbeat reliability (recent heartbeat = more reliable).
    /// Higher weight = prefer peers with recent heartbeats.
    pub reliability: u64,
    /// Weight for diversity penalty (same node_id prefix = penalty).
    /// Higher weight = stronger diversity enforcement.
    pub diversity: u64,
}

impl ScoringWeights {
    /// Default balanced weights (sum = 1,000,000).
    ///
    /// - freshness: 400,000 — primary signal for catch-up gossip
    /// - liveness: 300,000 — strong preference for active peers
    /// - reliability: 200,000 — moderate preference for reliable peers
    /// - diversity: 100,000 — light diversity enforcement
    pub fn balanced() -> Self {
        Self {
            freshness: 400_000,
            liveness: 300_000,
            reliability: 200_000,
            diversity: 100_000,
        }
    }

    /// Compute the total weight (should be 1,000,000).
    pub fn total(&self) -> u64 {
        self.freshness
            .saturating_add(self.liveness)
            .saturating_add(self.reliability)
            .saturating_add(self.diversity)
    }
}

impl Default for ScoringWeights {
    fn default() -> Self {
        Self::balanced()
    }
}

/// Liveness score for a peer's lifecycle state.
///
/// Higher score = more liveness preferred. Per DRS convention, scores are
/// u64 values in the 0-1M range.
pub fn liveness_score(state: SyncLifecycle) -> u64 {
    match state {
        SyncLifecycle::Streaming => 1_000_000,
        SyncLifecycle::Connecting => 500_000,
        SyncLifecycle::Authenticating => 400_000,
        SyncLifecycle::Suspect => 100_000,
        SyncLifecycle::Reconnecting => 50_000,
        SyncLifecycle::Init => 0,
        SyncLifecycle::Terminated => 0,
    }
}

/// Freshness score based on LSN watermark delta.
///
/// Lower LSN = peer is further behind = better catch-up target.
/// Score is normalized to 0-1M range where:
/// - LSN 0 (fully behind) = 1,000,000 (best target)
/// - LSN = local_lsn (fully caught up) = 0 (worst target)
///
/// If `local_lsn` is 0, all peers get max score (no delta info).
pub fn freshness_score(peer_lsn: Lsn, local_lsn: Lsn) -> u64 {
    if local_lsn == 0 {
        return 1_000_000;
    }
    if peer_lsn >= local_lsn {
        return 0;
    }
    // Scale: (local_lsn - peer_lsn) / local_lsn * 1M
    let delta = local_lsn.saturating_sub(peer_lsn);
    // Use u128 to avoid overflow in multiplication
    let score = (delta as u128) * 1_000_000u128 / (local_lsn as u128);
    score as u64
}

/// Reliability score based on heartbeat recency.
///
/// More recent heartbeat = more reliable. Score decays linearly over
/// `heartbeat_window_secs` (default: 30s = 6 heartbeat intervals).
pub fn reliability_score(
    last_heartbeat_unix: u64,
    now_unix_secs: u64,
    heartbeat_window_secs: u64,
) -> u64 {
    if last_heartbeat_unix == 0 {
        return 0;
    }
    let elapsed = now_unix_secs.saturating_sub(last_heartbeat_unix);
    if elapsed >= heartbeat_window_secs {
        return 0;
    }
    // Linear decay: 1M at t=0, 0 at t=window
    let remaining = heartbeat_window_secs.saturating_sub(elapsed);
    ((remaining as u128) * 1_000_000u128 / (heartbeat_window_secs as u128)) as u64
}

/// Compute a composite peer score.
///
/// Returns a u64 score in the 0-1M range. Higher = better gossip target.
pub fn compute_score(
    peer_lsn: Lsn,
    local_lsn: Lsn,
    state: SyncLifecycle,
    last_heartbeat_unix: u64,
    now_unix_secs: u64,
    weights: &ScoringWeights,
) -> u64 {
    let fresh = freshness_score(peer_lsn, local_lsn);
    let live = liveness_score(state);
    let rel = reliability_score(
        last_heartbeat_unix,
        now_unix_secs,
        weights.heartbeat_window_secs(),
    );

    // Composite score (all terms are 0-1M, weights sum to 1M)
    // Result is in 0-1M range (u64 saturating)
    let score = (fresh.saturating_mul(weights.freshness) / 1_000_000)
        .saturating_add(live.saturating_mul(weights.liveness) / 1_000_000)
        .saturating_add(rel.saturating_mul(weights.reliability) / 1_000_000);

    score.min(1_000_000)
}

impl ScoringWeights {
    /// Heartbeat window in seconds (for reliability decay).
    /// Default: 30s (6 heartbeat intervals at 5s each).
    pub fn heartbeat_window_secs(&self) -> u64 {
        30
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn balanced_weights_sum_to_1m() {
        let w = ScoringWeights::balanced();
        assert_eq!(w.total(), 1_000_000);
    }

    #[test]
    fn liveness_streaming_is_max() {
        assert_eq!(liveness_score(SyncLifecycle::Streaming), 1_000_000);
    }

    #[test]
    fn liveness_terminated_is_zero() {
        assert_eq!(liveness_score(SyncLifecycle::Terminated), 0);
    }

    #[test]
    fn freshness_fully_behind_is_max() {
        assert_eq!(freshness_score(0, 100), 1_000_000);
    }

    #[test]
    fn freshness_fully_caught_up_is_zero() {
        assert_eq!(freshness_score(100, 100), 0);
    }

    #[test]
    fn freshness_half_behind_is_half() {
        let score = freshness_score(50, 100);
        assert!((490_000..=510_000).contains(&score), "got {}", score);
    }

    #[test]
    fn freshness_zero_local_lsn() {
        assert_eq!(freshness_score(0, 0), 1_000_000);
    }

    #[test]
    fn reliability_no_heartbeat_is_zero() {
        assert_eq!(reliability_score(0, 100, 30), 0);
    }

    #[test]
    fn reliability_recent_heartbeat_is_high() {
        let score = reliability_score(95, 100, 30);
        assert!(score > 800_000, "got {}", score);
    }

    #[test]
    fn reliability_expired_heartbeat_is_zero() {
        assert_eq!(reliability_score(50, 100, 30), 0);
    }

    #[test]
    fn compute_score_streaming_behind_is_high() {
        let score = compute_score(
            10,
            100,
            SyncLifecycle::Streaming,
            95,
            100,
            &ScoringWeights::balanced(),
        );
        assert!(score > 500_000, "got {}", score);
    }

    #[test]
    fn compute_score_caught_up_terminated_is_zero() {
        let score = compute_score(
            100,
            100,
            SyncLifecycle::Terminated,
            0,
            100,
            &ScoringWeights::balanced(),
        );
        assert_eq!(score, 0);
    }

    #[test]
    fn compute_score_respects_weights() {
        // All weight on freshness
        let w = ScoringWeights {
            freshness: 1_000_000,
            liveness: 0,
            reliability: 0,
            diversity: 0,
        };
        let score = compute_score(0, 100, SyncLifecycle::Terminated, 0, 100, &w);
        // Only freshness contributes (Terminated has liveness=0)
        assert!(score > 900_000, "got {}", score);
    }
}
