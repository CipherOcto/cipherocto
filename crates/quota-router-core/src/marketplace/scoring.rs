//! Provider scoring, reputation registry, and latency-aware ranking (RFC-0900 Gap 7).
//!
//! Two surfaces:
//!
//! - `ProviderScore` + `ProviderReputationRegistry` — EWMA-tracked success
//!   rate + observed latency per asker (provider). The registry gates a
//!   configurable **circuit-breaker**: when an asker's recorded success
//!   rate falls below `min_reputation`, the asker is excluded from
//!   `Marketplace::cheapest()` lookups (RFC-0968).
//!
//! - `LatencyRanking` — composite score (weighted blend of normalized
//!   price + latency). `cheapest()` is the default price-only ranking;
//!   `prefer_latency()` weights latency higher so a faster provider beats
//!   a cheaper-but-slower one.

#![allow(clippy::cast_precision_loss)]

use parking_lot::Mutex;
use std::collections::HashMap;

/// Exponential moving average weight for new observations.
///
/// α = 0.3 means a single new observation shifts the EWMA by 30% of the
/// gap between current value and the new observation. After ~10 samples
/// the influence of the seed (1.0 success, 0 ms latency) drops below 3%.
const EWMA_ALPHA: f64 = 0.3;

/// Reputation record for one provider (asker).
///
/// The success rate is an EWMA over `[0.0, 1.0]` (1.0 = all observations
/// successful). `latency_ms` is the EWMA over observed wall-clock
/// latencies. `samples` is the running observation count (used by the
/// circuit-breaker to skip providers with no reputation data).
#[derive(Debug, Clone, PartialEq)]
pub struct ProviderScore {
    pub asker_did: String,
    pub success_rate: f64,
    pub latency_ms: u64,
    pub samples: u64,
}

impl ProviderScore {
    /// Construct a fresh score seeded at "perfect reputation".
    #[must_use]
    pub fn new(asker_did: impl Into<String>) -> Self {
        Self {
            asker_did: asker_did.into(),
            success_rate: 1.0,
            latency_ms: 0,
            samples: 0,
        }
    }

    /// True iff the score meets the circuit-breaker threshold.
    ///
    /// `threshold <= 0.0` disables filtering (every score passes).
    #[must_use]
    pub fn is_above_threshold(&self, threshold: f64) -> bool {
        threshold <= 0.0 || self.success_rate >= threshold
    }
}

/// Thread-safe registry of provider reputation scores + circuit-breaker.
///
/// When `min_reputation` is set to a positive value, providers whose
/// registered score falls below it are excluded from `Marketplace::cheapest`.
/// When `min_reputation <= 0.0` (the default), the registry is a passive
/// observer and never excludes anyone — preserving the pre-Gap-7 behavior.
pub struct ProviderReputationRegistry {
    inner: Mutex<HashMap<String, ProviderScore>>,
    min_reputation: Mutex<f64>,
}

impl ProviderReputationRegistry {
    /// Empty registry, circuit-breaker disabled.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
            min_reputation: Mutex::new(0.0),
        }
    }

    /// Configure the circuit-breaker threshold (`success_rate`).
    ///
    /// Migrate to `ProviderReputationRegistryCompat::set_min_reputation`,
    /// which lives in `quota_router_core::marketplace::reputation_compat`
    /// — **not** in `octo_reputation`, as this note previously claimed.
    /// That contrast is the whole point of this deprecation: the two
    /// registries hold independent thresholds, so
    /// `Marketplace::set_min_reputation` writes both. Writing only one
    /// leaves the breaker inert on the other selection path.
    #[deprecated(
        since = "0.1.0",
        note = "DEPRECATED: replaced by 0968-b Phase C; route through \
                `quota_router_core::marketplace::reputation_compat::\
                 ProviderReputationRegistryCompat::set_min_reputation`."
    )]
    pub fn set_min_reputation(&self, min: f64) {
        *self.min_reputation.lock() = min;
    }

    /// Current threshold. `0.0` (or negative) means the circuit-breaker
    /// is disabled.
    #[must_use]
    pub fn min_reputation(&self) -> f64 {
        *self.min_reputation.lock()
    }

    /// Record a transaction outcome. Creates the score on first call.
    #[deprecated(
        since = "0.1.0",
        note = "DEPRECATED: replaced by 0968-b Phase C; route writes through \
                `ReputationStore::record_signal` from `octo_reputation`."
    )]
    pub fn record(&self, asker_did: &str, success: bool, latency_ms: u64) {
        let mut inner = self.inner.lock();
        let entry = inner
            .entry(asker_did.to_owned())
            .or_insert_with(|| ProviderScore::new(asker_did));
        let s = if success { 1.0 } else { 0.0 };
        if entry.samples == 0 {
            // First observation seeds the EWMA directly (no prior value to blend with).
            entry.success_rate = s;
            entry.latency_ms = latency_ms;
        } else {
            entry.success_rate = EWMA_ALPHA * s + (1.0 - EWMA_ALPHA) * entry.success_rate;
            entry.latency_ms = ((EWMA_ALPHA * latency_ms as f64
                + (1.0 - EWMA_ALPHA) * entry.latency_ms as f64)
                .round()) as u64;
        }
        entry.samples = entry.samples.saturating_add(1);
    }

    /// Manual score override (operator / test fixture).
    ///
    /// **There is no migration target for this one.** This note
    /// previously read "use `ProviderReputationRegistryCompat::set_score`
    /// from `octo_reputation`" — a method that does not exist on that
    /// type, or anywhere. `ProviderReputationRegistryCompat` has no score
    /// setter at all: its sole data write is `record_with_now`, which
    /// appends events, because the store is event-sourced. There is
    /// nowhere to put an override on the canonical surface.
    ///
    /// Consequence: an override set here is invisible to
    /// `read_reputation_async` and to the async circuit breaker. Pinned
    /// by `score_override_is_invisible_to_the_async_breaker` in
    /// `tests/marketplace_reputation_async.rs`; see deferred mission
    /// `marketplace-set-provider-score-has-no-async-counterpart`.
    #[deprecated(
        since = "0.1.0",
        note = "DEPRECATED: replaced by 0968-b Phase C; NO canonical \
                counterpart exists yet. `ProviderReputationRegistryCompat` \
                has no score setter — see the deferred mission."
    )]
    pub fn set_score(&self, score: ProviderScore) {
        self.inner.lock().insert(score.asker_did.clone(), score);
    }

    /// Look up a score.
    #[must_use]
    pub fn score(&self, asker_did: &str) -> Option<ProviderScore> {
        self.inner.lock().get(asker_did).cloned()
    }

    /// True if the circuit-breaker should exclude this asker.
    ///
    /// Unknown providers (no observations) are NOT excluded — we treat
    /// them as "perfect reputation" rather than "no reputation" so that
    /// a fresh marketplace can still route to anyone.
    #[must_use]
    pub fn is_excluded(&self, asker_did: &str) -> bool {
        let min = self.min_reputation();
        if min <= 0.0 {
            return false;
        }
        match self.inner.lock().get(asker_did) {
            Some(s) => !s.is_above_threshold(min),
            None => false,
        }
    }
}

impl Default for ProviderReputationRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Latency-aware ranking weights.
///
/// Lower composite score is better. `price_weight` and `latency_weight`
/// are blended over min-max-normalized values, so the absolute scales
/// (u128 cost vs u64 latency) do not bias the result.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LatencyRanking {
    pub price_weight: f64,
    pub latency_weight: f64,
}

impl LatencyRanking {
    /// Price-only ranking (cheapest wins, latency ignored).
    ///
    /// This is the legacy behavior preserved by the default
    /// `Marketplace::cheapest`. The latency weight is 0 so the
    /// latency component is multiplied out.
    #[must_use]
    pub const fn cheapest() -> Self {
        Self {
            price_weight: 1.0,
            latency_weight: 0.0,
        }
    }

    /// Latency-weighted ranking: lower latency wins, price is a
    /// tiebreaker.
    #[must_use]
    pub const fn prefer_latency() -> Self {
        Self {
            price_weight: 0.3,
            latency_weight: 0.7,
        }
    }

    /// Weighted blend of normalized price + latency. Lower = better.
    ///
    /// `price`/`latency_ms` are the candidate's values; `min_*`/`max_*`
    /// describe the candidate set used for normalization. When the
    /// range collapses (all candidates identical on an axis), that
    /// axis is pinned at 0.0 so the other axis alone determines the
    /// ranking. Callers should pass `price_weight = 1.0` and
    /// `latency_weight = 0.0` for the legacy price-only behavior
    /// (`Self::cheapest` does this).
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn composite(
        &self,
        price: u128,
        latency_ms: u64,
        min_price: u128,
        max_price: u128,
        min_latency: u64,
        max_latency: u64,
    ) -> f64 {
        let price_norm = if max_price > min_price {
            (price - min_price) as f64 / (max_price - min_price) as f64
        } else {
            0.0
        };
        let latency_norm = if max_latency > min_latency {
            (latency_ms - min_latency) as f64 / (max_latency - min_latency) as f64
        } else {
            0.0
        };
        price_norm * self.price_weight + latency_norm * self.latency_weight
    }
}

/// Resolve per-candidate latency for [`LatencyRanking::composite`],
/// together with the `(min_latency, max_latency)` normalization bounds.
///
/// A provider with **no** latency observation is deliberately *not*
/// encoded as `0` ms. Zero is the fastest value the scale admits, so
/// resolving absent data to zero lets an unmeasured provider score a
/// perfect latency component and outrank every measured one — which in
///verts the ranking: a provider that costs 90x more and has never been
///measured once wins, purely because it has no history. Measured before
///this fix, on both the sync and async paths.
///
/// Absent observations resolve to the **worst latency observed** among the
/// candidates, so missing data never wins and cold-start providers remain
/// selectable on price. This is the fail-closed choice: it is the only
/// encoding under which an empty reputation history is not an advantage.
/// The alternative readings — midpoint, or excluding unmeasured providers
/// outright — change cold-start behaviour and are recorded as an
/// RFC-owner question in deferred mission
/// `marketplace-unknown-latency-is-encoded-as-zero-ms`.
///
/// When no candidate has an observation at all, every latency resolves to
/// `0` with degenerate bounds, and the composite degrades to price-only.
#[must_use]
pub fn resolve_ranking_latencies(observed: &[Option<u64>]) -> (Vec<u64>, u64, u64) {
    let Some(worst_observed) = observed.iter().flatten().copied().max() else {
        return (vec![0; observed.len()], 0, 0);
    };
    let latencies: Vec<u64> = observed
        .iter()
        .map(|o| o.unwrap_or(worst_observed))
        .collect();
    let min_latency = latencies.iter().copied().min().unwrap_or(0);
    (latencies, min_latency, worst_observed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ewma_first_observation_seeds_value() {
        let r = ProviderReputationRegistry::new();
        r.record("p", true, 100);
        let s = r.score("p").unwrap();
        assert_eq!(s.success_rate, 1.0);
        assert_eq!(s.latency_ms, 100);
        assert_eq!(s.samples, 1);
    }

    #[test]
    fn ewma_failure_drives_success_rate_below_one() {
        let r = ProviderReputationRegistry::new();
        r.record("p", true, 50);
        r.record("p", true, 50);
        r.record("p", false, 50);
        let s = r.score("p").unwrap();
        assert!(s.success_rate < 1.0 && s.success_rate > 0.0);
        assert_eq!(s.samples, 3);
    }

    #[test]
    fn circuit_breaker_excludes_only_when_min_reputation_set() {
        let r = ProviderReputationRegistry::new();
        r.record("p", false, 100);
        r.record("p", false, 100);
        // Default threshold 0.0 → not excluded.
        assert!(!r.is_excluded("p"));
        // Now enable the breaker.
        r.set_min_reputation(0.5);
        assert!(r.is_excluded("p"));
        // Unknown provider → not excluded.
        assert!(!r.is_excluded("unknown"));
    }

    #[test]
    fn ranking_cheapest_cancels_latency_axis() {
        let r = LatencyRanking::cheapest();
        // Same price (min), different latency → identical composite.
        let a = r.composite(100, 50, 100, 200, 50, 200);
        let b = r.composite(100, 200, 100, 200, 50, 200);
        assert_eq!(a, b, "price-only ranking must ignore latency");
    }

    #[test]
    fn ranking_prefer_latency_normalizes_axes() {
        let r = LatencyRanking::prefer_latency();
        // slow: price_norm=0, latency_norm=1 → 0*0.3 + 1*0.7 = 0.7
        // fast: price_norm=1, latency_norm=0 → 1*0.3 + 0*0.7 = 0.3
        let slow = r.composite(10, 200, 10, 100, 50, 200);
        let fast = r.composite(100, 50, 10, 100, 50, 200);
        assert!(slow > fast, "slow={slow} fast={fast}");
        assert!((slow - 0.7).abs() < 1e-9);
        assert!((fast - 0.3).abs() < 1e-9);
    }

    #[test]
    fn ranking_composite_handles_degenerate_ranges() {
        let r = LatencyRanking::prefer_latency();
        // All candidates identical on both axes → both norms pinned at 0.
        assert_eq!(r.composite(50, 100, 50, 50, 100, 100), 0.0);
    }
}
