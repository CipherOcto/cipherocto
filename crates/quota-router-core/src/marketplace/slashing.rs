//! Slashing — provider stake penalty on SLA miss (RFC-0900 §Slashing Model).
//!
//! Default rules per RFC-0900:
//!
//! | Rule                           | Value |
//! |--------------------------------|-------|
//! | First-offense penalty          | 10%   |
//! | Offense escalation multiplier  | 1.5   |
//! | Permanent ban threshold        | 50% of stake |
//!
//! `slash()` returns the amount actually deducted from the provider's
//! stake. The caller is responsible for emitting the on-chain /
//! settlement-side effect; this module only computes the penalty and
//! tracks per-provider offense counts.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Slashing reason classification (RFC-0900 §Dispute Evidence Challenge).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SlashReason {
    /// Provider exceeded latency SLA (timeout).
    Timeout,
    /// Provider returned a 5xx / non-2xx response.
    ProviderError,
    /// Provider latency exceeded configured max.
    LatencyHigh,
    /// Response was garbage (manual review path; rare on-chain).
    GarbageResponse,
    /// Provider failed to return any response.
    FailedResponse,
}

impl SlashReason {
    /// Verifiability weight — RFC-0900 §Dispute Evidence Challenge:
    /// automatic reasons (Timeout / ProviderError / LatencyHigh /
    /// FailedResponse) weight 1.0; manual-review reason
    /// (GarbageResponse) weights 0.5 to reflect trust discount.
    #[must_use]
    pub fn verifiability(self) -> f64 {
        match self {
            Self::Timeout | Self::ProviderError | Self::LatencyHigh | Self::FailedResponse => 1.0,
            Self::GarbageResponse => 0.5,
        }
    }
}

/// Slashing rules (RFC-0900 §Slashing Model defaults).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SlashingRules {
    /// Penalty fraction applied to `stake` on the first offense (0.0-1.0).
    pub first_offense_penalty: f64,
    /// Multiplier applied to the running penalty on each subsequent
    /// offense for the same provider.
    pub offense_multiplier: f64,
    /// Permanent ban threshold expressed as cumulative fraction of stake
    /// lost (0.0-1.0).
    pub permanent_ban_at: f64,
    /// Maximum miss rate (0.0-1.0) below which no slashing occurs. Default
    /// 0.0 (every miss slashes); some deployments use a tolerance band.
    pub miss_rate_tolerance: f64,
}

impl Default for SlashingRules {
    fn default() -> Self {
        Self {
            first_offense_penalty: 0.10,
            offense_multiplier: 1.5,
            permanent_ban_at: 0.50,
            miss_rate_tolerance: 0.0,
        }
    }
}

/// Per-provider state tracked by the slashing ledger.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderStake {
    pub provider_id: String,
    /// Current stake remaining (micro-OCTO-W).
    pub stake_micro_octo_w: u128,
    /// Initial stake at registration (micro-OCTO-W).
    pub initial_stake_micro_octo_w: u128,
    /// Number of slashes applied so far.
    pub offense_count: u32,
    /// Cumulative fraction of initial stake lost (0.0-1.0).
    pub cumulative_loss_pct: f64,
}

impl ProviderStake {
    /// True if the provider has been permanently banned (cumulative
    /// loss ≥ permanent ban threshold).
    #[must_use]
    pub fn is_banned(&self, rules: &SlashingRules) -> bool {
        self.cumulative_loss_pct >= rules.permanent_ban_at
    }
}

/// Outcome of a `slash()` call.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SlashOutcome {
    pub provider_id: String,
    pub reason: SlashReason,
    pub amount_micro_octo_w: u128,
    pub new_stake_micro_octo_w: u128,
    pub cumulative_loss_pct: f64,
    pub banned: bool,
}

/// Slashing errors.
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum SlashError {
    #[error("provider `{0}` not registered")]
    UnknownProvider(String),
    #[error("provider `{provider_id}` is permanently banned (cumulative_loss_pct_e6={cumulative_loss_pct_bits})")]
    BannedProvider {
        provider_id: String,
        /// `cumulative_loss_pct * 1_000_000`, rounded.
        cumulative_loss_pct_bits: u64,
    },
    #[error("miss rate e6={miss_rate_bits} below tolerance e6={tolerance_bits}")]
    BelowTolerance {
        /// `miss_rate * 1_000_000`, rounded.
        miss_rate_bits: u64,
        /// `tolerance * 1_000_000`, rounded.
        tolerance_bits: u64,
    },
}

/// Slashing ledger (in-memory; production backed by stoolap).
#[derive(Debug, Default, Clone)]
pub struct SlashingLedger {
    stakes: HashMap<String, ProviderStake>,
    rules: SlashingRules,
}

impl SlashingLedger {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_rules(rules: SlashingRules) -> Self {
        Self {
            stakes: HashMap::new(),
            rules,
        }
    }

    /// Register a provider with an initial stake. Idempotent on existing
    /// providers (returns the existing stake).
    pub fn register(
        &mut self,
        provider_id: impl Into<String>,
        initial_stake_micro_octo_w: u128,
    ) -> &ProviderStake {
        let provider_id = provider_id.into();
        self.stakes
            .entry(provider_id.clone())
            .or_insert_with(|| ProviderStake {
                provider_id: provider_id.clone(),
                stake_micro_octo_w: initial_stake_micro_octo_w,
                initial_stake_micro_octo_w,
                offense_count: 0,
                cumulative_loss_pct: 0.0,
            })
    }

    /// Current rules.
    #[must_use]
    pub fn rules(&self) -> &SlashingRules {
        &self.rules
    }

    /// Get the current stake state for a provider.
    #[must_use]
    pub fn stake(&self, provider_id: &str) -> Option<&ProviderStake> {
        self.stakes.get(provider_id)
    }

    /// Apply a slash to a provider for `reason`. Penalty = `stake *
    /// miss_rate * current_offense_penalty * verifiability`.
    ///
    /// - `miss_rate`: SLA miss rate in [0.0, 1.0].
    /// - `current_offense_penalty`: the rule's penalty fraction at the
    ///   provider's next offense (e.g., `first_offense_penalty *
    ///   multiplier^offense_count`).
    ///
    /// The function enforces `miss_rate >= rules.miss_rate_tolerance`
    /// and refuses to slash a permanently-banned provider.
    /// # Errors
    /// Returns `SlashError::UnknownProvider` if `provider_id` is not
    /// registered. Returns `SlashError::BannedProvider` if the provider
    /// already crossed the permanent-ban threshold. Returns
    /// `SlashError::BelowTolerance` if miss_rate is below the configured
    /// tolerance band.
    pub fn slash(
        &mut self,
        provider_id: &str,
        reason: SlashReason,
        miss_rate: f64,
    ) -> Result<SlashOutcome, SlashError> {
        if miss_rate < self.rules.miss_rate_tolerance {
            return Err(SlashError::BelowTolerance {
                miss_rate_bits: (miss_rate * 1_000_000.0).round() as u64,
                tolerance_bits: (self.rules.miss_rate_tolerance * 1_000_000.0).round() as u64,
            });
        }
        // Compute penalty fraction.
        let rules = self.rules;
        let offense_penalty = penalty_for_offense(
            rules.first_offense_penalty,
            rules.offense_multiplier,
            self.stake(provider_id)
                .ok_or_else(|| SlashError::UnknownProvider(provider_id.to_owned()))?
                .offense_count,
        );
        let pct = offense_penalty * reason.verifiability() * miss_rate.clamp(0.0, 1.0);
        self.apply_penalty(provider_id, reason, pct, rules)
    }

    /// Slash by an explicit penalty fraction (bypass escalation).
    /// Used by external arbitration paths that have computed their own
    /// penalty based on evidence severity.
    /// # Errors
    /// Returns `SlashError::UnknownProvider` if `provider_id` is not
    /// registered. Returns `SlashError::BannedProvider` if the provider
    /// already crossed the permanent-ban threshold.
    pub fn slash_with_pct(
        &mut self,
        provider_id: &str,
        reason: SlashReason,
        penalty_pct: f64,
    ) -> Result<SlashOutcome, SlashError> {
        let rules = self.rules;
        self.apply_penalty(provider_id, reason, penalty_pct.clamp(0.0, 1.0), rules)
    }

    fn apply_penalty(
        &mut self,
        provider_id: &str,
        reason: SlashReason,
        pct: f64,
        rules: SlashingRules,
    ) -> Result<SlashOutcome, SlashError> {
        let stake = self
            .stakes
            .get_mut(provider_id)
            .ok_or_else(|| SlashError::UnknownProvider(provider_id.to_owned()))?;
        if stake.is_banned(&rules) {
            // Encode the percent as integer bits to keep SlashError Eq.
            let bits = (stake.cumulative_loss_pct * 1_000_000.0).round() as u64;
            return Err(SlashError::BannedProvider {
                provider_id: provider_id.to_owned(),
                cumulative_loss_pct_bits: bits,
            });
        }
        // Round 1 fix: compute `amount` in u128 to avoid the f64
        // mantissa-exhaustion precision loss that occurred for stakes
        // above 2^53 (≈ 9.0 × 10^15 micro-OCTO-W). The percent is
        // scaled to micro-percent (1e6) while still in [0, 1_000_000]
        // — well within f64 exact-integer range — so the cast is
        // exact. Then `(stake * pct_micro) / 1_000_000` stays in u128.
        let pct_micro = (pct.clamp(0.0, 1.0) * 1_000_000.0).round() as u128;
        let amount = stake
            .stake_micro_octo_w
            .checked_mul(pct_micro)
            .expect("stake * pct_micro overflows u128 — stake > u128::MAX / 1e6");
        let amount = amount / 1_000_000;
        // Cap deduction at remaining stake.
        let amount = amount.min(stake.stake_micro_octo_w);
        stake.stake_micro_octo_w -= amount;
        stake.offense_count += 1;
        let loss_delta = if stake.initial_stake_micro_octo_w == 0 {
            0.0
        } else {
            amount as f64 / stake.initial_stake_micro_octo_w as f64
        };
        stake.cumulative_loss_pct += loss_delta;
        let banned = stake.is_banned(&rules);
        Ok(SlashOutcome {
            provider_id: provider_id.to_owned(),
            reason,
            amount_micro_octo_w: amount,
            new_stake_micro_octo_w: stake.stake_micro_octo_w,
            cumulative_loss_pct: stake.cumulative_loss_pct,
            banned,
        })
    }
}

fn penalty_for_offense(first: f64, multiplier: f64, offense_count: u32) -> f64 {
    let mult = multiplier.powi(offense_count as i32);
    (first * mult).min(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ledger_with(stake: u128) -> SlashingLedger {
        let mut l = SlashingLedger::new();
        l.register("alice", stake);
        l
    }

    #[test]
    fn register_creates_provider_stake() {
        let l = ledger_with(1_000_000);
        let s = l.stake("alice").unwrap();
        assert_eq!(s.stake_micro_octo_w, 1_000_000);
        assert_eq!(s.initial_stake_micro_octo_w, 1_000_000);
        assert_eq!(s.offense_count, 0);
        assert_eq!(s.cumulative_loss_pct, 0.0);
        assert!(!s.is_banned(l.rules()));
    }

    #[test]
    fn slash_deducts_stake_times_miss_rate_times_first_offense_penalty() {
        let mut l = ledger_with(1_000_000);
        // first_offense_penalty = 0.10, miss_rate = 1.0, Timeout (verifiability 1.0)
        // → amount = 1_000_000 * 0.10 * 1.0 * 1.0 = 100_000
        let out = l.slash("alice", SlashReason::Timeout, 1.0).unwrap();
        assert_eq!(out.amount_micro_octo_w, 100_000);
        assert_eq!(out.new_stake_micro_octo_w, 900_000);
        assert_eq!(out.cumulative_loss_pct, 0.10);
        assert!(!out.banned);
    }

    #[test]
    fn slash_scales_with_miss_rate() {
        let mut l = ledger_with(1_000_000);
        let out = l.slash("alice", SlashReason::ProviderError, 0.5).unwrap();
        // 0.10 * 0.5 = 0.05 → 50_000
        assert_eq!(out.amount_micro_octo_w, 50_000);
    }

    #[test]
    fn garbage_response_uses_half_verifiability() {
        let mut l = ledger_with(1_000_000);
        let out = l.slash("alice", SlashReason::GarbageResponse, 1.0).unwrap();
        // 0.10 * 1.0 * 0.5 = 0.05 → 50_000
        assert_eq!(out.amount_micro_octo_w, 50_000);
    }

    #[test]
    fn repeat_offenses_escalate_penalty() {
        let mut l = ledger_with(1_000_000);
        // 1st: 0.10 → 100_000
        let o1 = l.slash("alice", SlashReason::Timeout, 1.0).unwrap();
        assert_eq!(o1.amount_micro_octo_w, 100_000);
        // 2nd: 0.10 * 1.5 = 0.15 → 0.15 * 900_000 = 135_000
        let o2 = l.slash("alice", SlashReason::Timeout, 1.0).unwrap();
        assert_eq!(o2.amount_micro_octo_w, 135_000);
        // 3rd: 0.10 * 1.5^2 = 0.225 → 0.225 * 765_000 = 172_125
        let o3 = l.slash("alice", SlashReason::Timeout, 1.0).unwrap();
        assert_eq!(o3.amount_micro_octo_w, 172_125);
    }

    #[test]
    fn cumulative_loss_pct_tracks_initial_stake() {
        let mut l = ledger_with(1_000_000);
        l.slash("alice", SlashReason::Timeout, 1.0).unwrap();
        let s = l.stake("alice").unwrap();
        assert_eq!(s.cumulative_loss_pct, 0.10);
    }

    #[test]
    fn permanent_ban_at_50pct_loss() {
        let mut l = ledger_with(1_000_000);
        // 1st: 0.10 → 900_000 left, cumulative 0.10
        l.slash("alice", SlashReason::Timeout, 1.0).unwrap();
        // 2nd: 0.15 → 900_000 * 0.15 = 135_000; left 765_000, cumulative 0.235
        l.slash("alice", SlashReason::Timeout, 1.0).unwrap();
        // 3rd: 0.225 → 765_000 * 0.225 = 172_125; left 592_875, cumulative 0.407125
        l.slash("alice", SlashReason::Timeout, 1.0).unwrap();
        // 4th: 0.3375 → 592_875 * 0.3375 = 200_095.31 ≈ 200_095; left 392_780,
        // cumulative 0.6072 ≥ 0.50 → banned
        let out = l.slash("alice", SlashReason::Timeout, 1.0).unwrap();
        assert!(out.banned);
        assert!(l.stake("alice").unwrap().is_banned(l.rules()));
    }

    #[test]
    fn slashing_banned_provider_errors() {
        let mut l = ledger_with(1_000_000);
        // Drive alice to ban with a direct big penalty (arbitration path).
        l.slash_with_pct("alice", SlashReason::Timeout, 0.6)
            .unwrap();
        let err = l.slash("alice", SlashReason::Timeout, 1.0).unwrap_err();
        assert!(matches!(err, SlashError::BannedProvider { .. }));
    }

    #[test]
    fn slashing_unknown_provider_errors() {
        let mut l = SlashingLedger::new();
        let err = l.slash("ghost", SlashReason::Timeout, 1.0).unwrap_err();
        assert_eq!(err, SlashError::UnknownProvider("ghost".to_owned()));
    }

    #[test]
    fn miss_rate_below_tolerance_errors() {
        let mut l = SlashingLedger::with_rules(SlashingRules {
            miss_rate_tolerance: 0.05,
            ..SlashingRules::default()
        });
        l.register("alice", 1_000_000);
        let err = l.slash("alice", SlashReason::Timeout, 0.01).unwrap_err();
        assert!(matches!(err, SlashError::BelowTolerance { .. }));
    }

    #[test]
    fn slash_with_explicit_pct_bypasses_escalation() {
        let mut l = ledger_with(1_000_000);
        let out = l
            .slash_with_pct("alice", SlashReason::GarbageResponse, 0.25)
            .unwrap();
        // 0.25 of 1_000_000 = 250_000
        assert_eq!(out.amount_micro_octo_w, 250_000);
        assert_eq!(out.cumulative_loss_pct, 0.25);
    }

    #[test]
    fn slash_does_not_overdraft_remaining_stake() {
        let mut l = ledger_with(100_000);
        // Apply an oversized explicit penalty; should cap at remaining stake.
        let out = l
            .slash_with_pct("alice", SlashReason::Timeout, 1.5)
            .unwrap();
        assert_eq!(out.amount_micro_octo_w, 100_000);
        assert_eq!(out.new_stake_micro_octo_w, 0);
    }

    #[test]
    fn default_rules_match_rfc0900() {
        let rules = SlashingRules::default();
        assert!((rules.first_offense_penalty - 0.10).abs() < f64::EPSILON);
        assert!((rules.offense_multiplier - 1.5).abs() < f64::EPSILON);
        assert!((rules.permanent_ban_at - 0.50).abs() < f64::EPSILON);
    }
}
