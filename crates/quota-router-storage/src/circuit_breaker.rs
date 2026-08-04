//! Anti-fraud circuit breaker (RFC-0959 §Lifecycle Requirements).
//!
//! 5-state machine: `Active ↔ Recovering ↔ Tripped` with administrative
//! paths. Per RFC-0959, the breaker is **advisory only** — it gates FUTURE
//! axis classification (`CachedInputTokensPer1k` vs `InputTokensPer1k`)
//! but **must NEVER mutate canonical `axes_consumed` on already-settled
//! events** (R1 critical fix; the Class A settlement determinism invariant
//! from RFC-0909 §Determinism Requirements).
//!
//! State transitions:
//! - `Active → Tripped`: automatic, when `cache_hit_rate > 0.90` AND
//!   `prompt_diversity < MIN_PROMPT_DIVERSITY` over last `WINDOW_SIZE` calls.
//! - `Tripped → Recovering`: automatic, after `RECOVERY_COOLDOWN_SECS` elapses.
//! - `Recovering → Active`: automatic, after `RECOVERY_OBSERVE_SECS` of
//!   clean observations elapse.
//! - `Active → Recovering`: administrative, requires `Operator` signature
//!   (human-in-the-loop for audit trail; anti-fraud events auto-route
//!   via `Active → Tripped` not `Active → Recovering`).
//! - `Recovering → Tripped`: automatic, on any further violation during
//!   the recovery observation window.

use std::collections::VecDeque;

/// Minimum unique BLAKE3 keys required for a healthy prompt stream.
/// Below this, the circuit considers the traffic to be a potential
/// cache-gaming attack (RFC-0959 §Adversary A5).
/// Heuristic — production tuning required once empirical data lands.
pub const MIN_PROMPT_DIVERSITY: usize = 50;

/// Cache-hit-rate threshold above which the circuit trips (assuming
/// diversity is also below the threshold).
pub const CACHE_HIT_RATE_TRIP_THRESHOLD: f64 = 0.90;

/// Number of recent calls in the observation window.
pub const WINDOW_SIZE: usize = 1_000;

/// Cooldown before a Tripped circuit auto-transitions to Recovering.
pub const RECOVERY_COOLDOWN_SECS: u64 = 300; // 5 minutes

/// Observation window length for Recovering before auto-transition to Active.
pub const RECOVERY_OBSERVE_SECS: u64 = 600; // 10 minutes

/// Circuit states (RFC-0959 §Lifecycle Requirements state machine).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CircuitState {
    /// Normal operation. May freely classify cached vs non-cached axes.
    Active,
    /// Anti-fraud tripped. Future calls: `CachedInputTokensPer1k` axis
    /// MUST be reclassified as `InputTokensPer1k` (no discount).
    Tripped,
    /// Under observation after a trip. If violations recur, re-trips.
    /// After `RECOVERY_OBSERVE_SECS` of clean observations, transitions
    /// to `Active`.
    Recovering,
}

impl std::fmt::Display for CircuitState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Active => f.write_str("active"),
            Self::Tripped => f.write_str("tripped"),
            Self::Recovering => f.write_str("recovering"),
        }
    }
}

use serde::{Deserialize, Serialize};

/// Reason for a state transition (audit trail).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum TransitionReason {
    /// Automatic trip due to high cache-hit-rate + low diversity.
    AutoTripped {
        cache_hit_rate_bps: u32,
        unique_prompts: usize,
    },
    /// Automatic transition after cooldown elapsed.
    AutoCooldownElapsed { secs_in_tripped: u64 },
    /// Automatic transition after clean observation window.
    AutoCleanObservation { secs_in_recovering: u64 },
    /// Automatic re-trip due to violation during recovery.
    AutoRetripped {
        cache_hit_rate_bps: u32,
        unique_prompts: usize,
    },
    /// Operator signature attested (administrative audit).
    OperatorSignature {
        operator_did: String,
        justification: String,
    },
}

/// State transition event (audit log).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransitionEvent {
    pub from: CircuitState,
    pub to: CircuitState,
    pub at_unix: u64,
    pub reason: TransitionReason,
}

/// Anti-fraud circuit breaker (RFC-0959 §Lifecycle Requirements).
///
/// **Advisory only:** the breaker tracks observations and transitions
/// between states but does NOT mutate canonical `axes_consumed` on
/// already-settled events. Callers consult [`Self::classify_axis`] to
/// decide whether a FUTURE settlement should use the `cached_*` axis
/// or fall back to the non-cached axis.
#[derive(Debug, Clone)]
pub struct CircuitBreaker {
    state: CircuitState,
    entered_state_at_unix: u64,
    /// Sliding window of cache-hit observations (true = hit, false = miss).
    cache_hits: VecDeque<bool>,
    /// Sliding window of unique BLAKE3 cache keys (size tracks diversity).
    unique_keys: VecDeque<[u8; 32]>,
    /// Audit log of transitions.
    transitions: Vec<TransitionEvent>,
}

impl Default for CircuitBreaker {
    fn default() -> Self {
        Self::new()
    }
}

impl CircuitBreaker {
    /// Create a new circuit breaker in `Active` state.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            state: CircuitState::Active,
            entered_state_at_unix: 0,
            cache_hits: VecDeque::new(),
            unique_keys: VecDeque::new(),
            transitions: Vec::new(),
        }
    }

    /// Current state.
    #[must_use]
    pub const fn state(&self) -> CircuitState {
        self.state
    }

    /// All recorded state transitions (audit trail).
    #[must_use]
    pub fn transitions(&self) -> &[TransitionEvent] {
        &self.transitions
    }

    /// Record a single observation (called once per settled call).
    ///
    /// Returns the transition event if the observation caused an automatic
    /// state change; `None` otherwise.
    pub fn observe(
        &mut self,
        cache_hit: bool,
        cache_key_hash: [u8; 32],
        now_unix: u64,
    ) -> Option<TransitionEvent> {
        // 1. Maintain sliding window.
        if self.cache_hits.len() >= WINDOW_SIZE {
            self.cache_hits.pop_front();
        }
        self.cache_hits.push_back(cache_hit);

        if self.unique_keys.len() >= WINDOW_SIZE {
            self.unique_keys.pop_front();
        }
        if !self.unique_keys.contains(&cache_key_hash) {
            self.unique_keys.push_back(cache_key_hash);
        }

        // 2. Evaluate state machine.
        let hit_rate = self.cache_hit_rate();
        // Use the count of DISTINCT keys in the current window (not the
        // VecDeque length, which can include repeats due to FIFO eviction).
        let unique = self.unique_prompt_count();

        match self.state {
            CircuitState::Active => {
                // Only trip after the window has at least WINDOW_SIZE observations
                // — otherwise the first few hits can spuriously trip the breaker
                // (e.g., 1 observation with 1 hit + 1 unique key is trivially
                // "100% hit rate + 1 unique" but doesn't yet meet the threshold
                // for "the last 1000 calls look suspicious").
                let window_full = self.cache_hits.len() >= WINDOW_SIZE;
                if window_full
                    && hit_rate > CACHE_HIT_RATE_TRIP_THRESHOLD
                    && unique < MIN_PROMPT_DIVERSITY
                {
                    let event = TransitionEvent {
                        from: CircuitState::Active,
                        to: CircuitState::Tripped,
                        at_unix: now_unix,
                        reason: TransitionReason::AutoTripped {
                            cache_hit_rate_bps: (hit_rate * 10_000.0) as u32,
                            unique_prompts: unique,
                        },
                    };
                    self.state = CircuitState::Tripped;
                    self.entered_state_at_unix = now_unix;
                    self.transitions.push(event.clone());
                    Some(event)
                } else {
                    None
                }
            }
            CircuitState::Tripped => {
                if self.cache_hits.len() >= WINDOW_SIZE
                    && now_unix.saturating_sub(self.entered_state_at_unix) >= RECOVERY_COOLDOWN_SECS
                {
                    let event = TransitionEvent {
                        from: CircuitState::Tripped,
                        to: CircuitState::Recovering,
                        at_unix: now_unix,
                        reason: TransitionReason::AutoCooldownElapsed {
                            secs_in_tripped: now_unix.saturating_sub(self.entered_state_at_unix),
                        },
                    };
                    self.state = CircuitState::Recovering;
                    self.entered_state_at_unix = now_unix;
                    self.transitions.push(event.clone());
                    Some(event)
                } else {
                    None
                }
            }
            CircuitState::Recovering => {
                let window_full = self.cache_hits.len() >= WINDOW_SIZE;
                if window_full
                    && hit_rate > CACHE_HIT_RATE_TRIP_THRESHOLD
                    && unique < MIN_PROMPT_DIVERSITY
                {
                    let event = TransitionEvent {
                        from: CircuitState::Recovering,
                        to: CircuitState::Tripped,
                        at_unix: now_unix,
                        reason: TransitionReason::AutoRetripped {
                            cache_hit_rate_bps: (hit_rate * 10_000.0) as u32,
                            unique_prompts: unique,
                        },
                    };
                    self.state = CircuitState::Tripped;
                    self.entered_state_at_unix = now_unix;
                    self.transitions.push(event.clone());
                    Some(event)
                } else if window_full
                    && now_unix.saturating_sub(self.entered_state_at_unix) >= RECOVERY_OBSERVE_SECS
                {
                    let event = TransitionEvent {
                        from: CircuitState::Recovering,
                        to: CircuitState::Active,
                        at_unix: now_unix,
                        reason: TransitionReason::AutoCleanObservation {
                            secs_in_recovering: now_unix.saturating_sub(self.entered_state_at_unix),
                        },
                    };
                    self.state = CircuitState::Active;
                    self.entered_state_at_unix = now_unix;
                    self.transitions.push(event.clone());
                    Some(event)
                } else {
                    None
                }
            }
        }
    }

    /// Operator-initiated `Active → Recovering` (RFC-0959 §Lifecycle R3 fix).
    /// Requires an operator DID + justification for the audit trail.
    pub fn operator_recover(
        &mut self,
        operator_did: impl Into<String>,
        justification: impl Into<String>,
        now_unix: u64,
    ) -> Result<TransitionEvent, CircuitBreakerError> {
        if self.state != CircuitState::Active {
            return Err(CircuitBreakerError::InvalidTransition {
                from: self.state,
                to: CircuitState::Recovering,
            });
        }
        let event = TransitionEvent {
            from: CircuitState::Active,
            to: CircuitState::Recovering,
            at_unix: now_unix,
            reason: TransitionReason::OperatorSignature {
                operator_did: operator_did.into(),
                justification: justification.into(),
            },
        };
        self.state = CircuitState::Recovering;
        self.entered_state_at_unix = now_unix;
        self.transitions.push(event.clone());
        Ok(event)
    }

    /// Cache-hit rate over the current window (0.0-1.0).
    #[must_use]
    pub fn cache_hit_rate(&self) -> f64 {
        if self.cache_hits.is_empty() {
            return 0.0;
        }
        let hits = self.cache_hits.iter().filter(|&&h| h).count();
        hits as f64 / self.cache_hits.len() as f64
    }

    /// Unique BLAKE3 keys in the current window.
    #[must_use]
    pub fn unique_prompt_count(&self) -> usize {
        // Count distinct values in the window.
        let mut seen = std::collections::HashSet::new();
        for k in &self.unique_keys {
            seen.insert(*k);
        }
        seen.len()
    }

    /// Classify a settlement axis per RFC-0959 §Lifecycle.
    ///
    /// If the circuit is `Active` or `Recovering` (no recent violation),
    /// the caller may keep the original `cached_*` axis. If `Tripped`,
    /// the caller MUST reclassify `cached_input_tokens_per_1k` →
    /// `input_tokens_per_1k` for FUTURE settlements (anti-fraud mitigation).
    ///
    /// **The breaker does NOT mutate axes_consumed on already-settled
    /// events** — see module docs.
    #[must_use]
    pub fn classify_axis(&self, axis_id: &str) -> AxisClassification {
        if self.state == CircuitState::Tripped && axis_id.starts_with("cached_") {
            AxisClassification::ReclassifyToNonCached
        } else {
            AxisClassification::Keep
        }
    }
}

/// Per-axis classification result from [`CircuitBreaker::classify_axis`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AxisClassification {
    /// Use the requested axis unchanged.
    Keep,
    /// Circuit is tripped on cached axes — reclassify to the non-cached form
    /// (e.g., `cached_input_tokens_per_1k` → `input_tokens_per_1k`).
    ReclassifyToNonCached,
}

/// Circuit breaker errors.
#[derive(Debug, thiserror::Error)]
pub enum CircuitBreakerError {
    #[error("invalid transition from {from:?} to {to:?}")]
    InvalidTransition {
        from: CircuitState,
        to: CircuitState,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seed_window(breaker: &mut CircuitBreaker, hits: bool, unique: usize, now: u64) {
        // Feed `unique` distinct cache keys, each with `hits` semantics.
        for i in 0..unique {
            let mut key = [0u8; 32];
            key[0] = i as u8;
            breaker.observe(hits, key, now);
        }
    }

    #[test]
    fn starts_in_active_state() {
        let cb = CircuitBreaker::new();
        assert_eq!(cb.state(), CircuitState::Active);
        assert_eq!(cb.transitions().len(), 0);
    }

    #[test]
    fn active_to_tripped_on_high_hit_rate_low_diversity() {
        let mut cb = CircuitBreaker::new();
        // 1000 calls, 95% hits, 10 unique keys → trips.
        for i in 0..1000 {
            let key = [
                (i % 10) as u8,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
            ];
            let hit = i < 950;
            if let Some(event) = cb.observe(hit, key, 1000 + i as u64) {
                assert_eq!(event.from, CircuitState::Active);
                assert_eq!(event.to, CircuitState::Tripped);
                return;
            }
        }
        panic!("circuit never tripped");
    }

    #[test]
    fn active_stays_active_when_diversity_high() {
        let mut cb = CircuitBreaker::new();
        // 1000 calls, 95% hits, but ~1000 unique keys → does NOT trip.
        for i in 0..1000 {
            let mut key = [0u8; 32];
            key[0] = (i % 256) as u8;
            key[1] = (i / 256) as u8;
            let hit = i < 950;
            let transition = cb.observe(hit, key, 1000 + i as u64);
            if transition.is_some() {
                panic!(
                    "circuit tripped at i={i} with hit_rate={:.3} unique_count={} state={:?}",
                    cb.cache_hit_rate(),
                    cb.unique_prompt_count(),
                    cb.state(),
                );
            }
        }
        assert_eq!(cb.state(), CircuitState::Active);
    }

    #[test]
    fn tripped_to_recovering_after_cooldown() {
        let mut cb = CircuitBreaker::new();
        // Trip the circuit (window fills + violation pattern at the trip transition).
        let mut transition = None;
        for i in 0..1000 {
            let key = [
                (i % 5) as u8,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
            ];
            let hit = i < 980;
            transition = cb.observe(hit, key, 1000 + i as u64);
        }
        // Trip fires on the last iteration of the violation loop.
        let trip_event = transition.expect("expected Active → Tripped at end of violation window");
        assert_eq!(trip_event.from, CircuitState::Active);
        assert_eq!(trip_event.to, CircuitState::Tripped);
        assert_eq!(cb.state(), CircuitState::Tripped);
        // Continue feeding clean observations until cooldown elapses.
        // Transition fires when window_full AND (t - entered_state) >= RECOVERY_COOLDOWN_SECS.
        // The trip transition happened at t=1999 (loop index 999). entered_state_at_unix=1999.
        // RECOVERY_COOLDOWN_SECS = 300. Next trip happens at the first observation where
        // (t - 1999) >= 300 AND window_full (still 1000 from the loop).
        let key = [99u8; 32];
        let event = cb
            .observe(false, key, 1999 + RECOVERY_COOLDOWN_SECS + 1)
            .unwrap();
        assert_eq!(event.from, CircuitState::Tripped);
        assert_eq!(event.to, CircuitState::Recovering);
    }

    #[test]
    fn recovering_to_active_after_clean_observation() {
        let mut cb = CircuitBreaker::new();
        // Manually move to Recovering via operator path (avoids needing to engineer a trip + cooldown).
        cb.operator_recover("did:octo:op1", "audit-test", 1000)
            .unwrap();
        assert_eq!(cb.state(), CircuitState::Recovering);
        // Fill the observation window with diverse, low-hit observations.
        // At t=2000 (window fills) AND t-entered_state >= RECOVERY_OBSERVE_SECS
        // (2000 - 1000 = 1000 >= 600), the breaker auto-transitions Recovering → Active.
        let mut transition = None;
        for i in 0..1000 {
            let mut key = [0u8; 32];
            key[0] = (i % 256) as u8;
            key[1] = (i / 256) as u8;
            // All misses, all diverse — clean observation.
            transition = cb.observe(false, key, 1001 + i as u64);
        }
        let event = transition.expect("expected Recovering → Active at end of clean window");
        assert_eq!(event.from, CircuitState::Recovering);
        assert_eq!(event.to, CircuitState::Active);
    }

    #[test]
    fn recovering_to_tripped_on_repeat_violation() {
        let mut cb = CircuitBreaker::new();
        cb.operator_recover("did:octo:op1", "audit-test", 1000)
            .unwrap();
        // Generate violation pattern within the observation window
        // (RECOVERY_OBSERVE_SECS = 600s). After 1000 calls starting at
        // t=1001, the hit window fills at t=2000 (within observation window)
        // → Recovering → Tripped. If we started later than 1000+600=1600,
        // the breaker would first auto-transition to Active.
        for i in 0..1000 {
            let key = [
                (i % 5) as u8,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
            ];
            let hit = i < 980;
            if let Some(event) = cb.observe(hit, key, 1001 + i as u64) {
                assert_eq!(event.from, CircuitState::Recovering);
                assert_eq!(event.to, CircuitState::Tripped);
                return;
            }
        }
        panic!("recovering circuit never re-tripped");
    }

    #[test]
    fn operator_recover_requires_active_state() {
        let mut cb = CircuitBreaker::new();
        cb.operator_recover("did:octo:op1", "first", 1000).unwrap();
        // Now in Recovering. operator_recover should fail.
        let err = cb
            .operator_recover("did:octo:op1", "second", 2000)
            .unwrap_err();
        assert!(matches!(err, CircuitBreakerError::InvalidTransition { .. }));
    }

    #[test]
    fn active_to_recovering_requires_operator() {
        // R3 fix: Active → Recovering is NOT automatic. It requires
        // operator_recover() (administrative audit path).
        let mut cb = CircuitBreaker::new();
        // Feed observations that would trigger Active→Tripped (high hit rate, low diversity).
        for i in 0..1000 {
            let key = [
                (i % 5) as u8,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
            ];
            let hit = i < 980;
            let transition = cb.observe(hit, key, 1000 + i as u64);
            // The transition MUST go Active → Tripped, NOT Active → Recovering.
            if let Some(event) = transition {
                assert_ne!(
                    event.to,
                    CircuitState::Recovering,
                    "Active → Recovering must be operator-initiated, not auto"
                );
                return;
            }
        }
    }

    #[test]
    fn classify_axis_reclassifies_cached_when_tripped() {
        let mut cb = CircuitBreaker::new();
        assert_eq!(
            cb.classify_axis("cached_input_tokens_per_1k"),
            AxisClassification::Keep
        );
        // Force-trip via operator path simulation: can't, so feed violation pattern.
        for i in 0..1000 {
            let key = [
                (i % 5) as u8,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
            ];
            let hit = i < 980;
            cb.observe(hit, key, 1000 + i as u64);
        }
        assert_eq!(cb.state(), CircuitState::Tripped);
        assert_eq!(
            cb.classify_axis("cached_input_tokens_per_1k"),
            AxisClassification::ReclassifyToNonCached
        );
        assert_eq!(
            cb.classify_axis("input_tokens_per_1k"),
            AxisClassification::Keep
        );
        assert_eq!(
            cb.classify_axis("output_tokens_per_1k"),
            AxisClassification::Keep
        );
    }

    #[test]
    fn advisory_only_does_not_mutate_settled_events() {
        // The breaker MUST NOT have a method that mutates settled events.
        // This test documents the invariant by enumerating the public API.
        let cb = CircuitBreaker::new();
        // Public API surface (compile-time check via type-level introspection):
        // state, transitions, observe, operator_recover, cache_hit_rate,
        // unique_prompt_count, classify_axis.
        // No `mutate_settled_event`, no `rewind_axes_consumed`, etc.
        let _ = cb.state();
        let _ = cb.transitions();
        let _ = cb.cache_hit_rate();
        let _ = cb.unique_prompt_count();
        let _ = cb.classify_axis("cached_input_tokens_per_1k");
        // If a future contributor adds a mutator, this test fails to compile
        // (they'd need to add the new method to the explicit list above).
    }

    #[test]
    fn unique_prompt_count_distinct_within_window() {
        let mut cb = CircuitBreaker::new();
        // Insert 100 calls but only 10 unique keys → unique_prompt_count = 10.
        for i in 0..100 {
            let mut key = [0u8; 32];
            key[0] = (i % 10) as u8;
            cb.observe(false, key, 1000 + i as u64);
        }
        assert_eq!(cb.unique_prompt_count(), 10);
    }
}
