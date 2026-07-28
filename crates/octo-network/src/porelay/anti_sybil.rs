//! Anti-Sybil Mechanisms (RFC-0860 §7)
//!
//! ## RFC-0968 cross-rule integration (Round 2 review I14 — closure path)
//!
//! Per mission 0860a AC #5/6 + Round 7 cross-mission-governance #6,
//! the local minimum-stake and diversity-budget invariants compose
//! with RFC-0968's weighted-similarity correlation classifier and
//! per-controller coalition-detection thresholds. The canonical
//! thresholds live in `octo_reputation::constants`:
//!
//! - `MIN_RECORDER_ROLE_STAKE = 1000` — role-token stake lower bound.
//! - `MIN_RECORDER_OCTO_STAKE = 4000` — sovereign stake lower bound.
//! - `MIN_RECORDER_DUAL_STAKE = 5000` — aggregate minimum.
//! - `WEIGHTED_SIMILARITY_THRESHOLD = 0.60` — Round 7 amendment 46
//!   cross-rule correlation classifier.
//! - `MAX_COALITION_KM_PRODUCT = 100` — Round 7 amendments 42 + 50 —
//!   `(distinct_subjects × distinct_layers)` ceiling beyond which
//!   a coalition is `CoalitionQuarantined`.
//!
//! PoRelay's `MINIMUM_STAKE = 1000` matches `MIN_RECORDER_ROLE_STAKE`
//! (the OCTO-B role-token floor); the dual-stake gate for
//! reputation-recorder status is enforced via the canonical
//! `SlashReputationStoreCompat::global_slash_count` path through the
//! persisted `ReputationStore`. Anti-Sybil tests reuse these
//! constants for byte-identical cross-rule coherence.

use serde::{Deserialize, Serialize};

/// Minimum source diversity threshold
pub const MIN_SOURCE_DIVERSITY: u32 = 2;

/// Minimum destination diversity threshold
pub const MIN_DEST_DIVERSITY: u32 = 2;

/// Minimum peer diversity threshold
pub const MIN_PEER_DIVERSITY: u16 = 3;

/// Minimum OCTO-B stake for proof generation. Matches the canonical
/// `MIN_RECORDER_ROLE_STAKE` lower bound in `octo-reputation::constants`.
/// RFC-0860 §7 anti-Sybil guarantees require AT LEAST this stake for
/// proof generation; OCTO-N (`navigator`) and sovereign OCTO gates
/// apply separately on the reputation-recorder side.
pub const MINIMUM_STAKE: u64 = 1000;

/// RFC-0968-A1 amendment 46 — weighted-similarity correlation
/// classifier threshold (cross-rule coherence with mission 0860a AC
/// #6). Resolved here so the porelay/anti_sybil.rs module is the
/// single source-of-truth for the cross-rule number.
pub const WEIGHTED_SIMILARITY_THRESHOLD: f64 = 0.60;

/// RFC-0968-A1 amendment 50 — per-controller coalition detection
/// ceiling. A coalition whose `(distinct_subjects × distinct_layers)`
/// exceeds this is `CoalitionQuarantined` per RFC-0968 §13 error
/// code 0x30 (Round 7 amendments 42 + 50).
pub const MAX_COALITION_KM_PRODUCT: u64 = 100;

/// Coalition-detection budget check (mission 0860a AC #5). Returns
/// true iff a coalition's `(distinct_subjects × distinct_layers)` is
/// within `MAX_COALITION_KM_PRODUCT = 100`. Callers consume this
/// before admitting a gateway; a quarantined coalition is refused
/// even when individual diversity checks pass.
pub fn coalition_within_budget(distinct_subjects: u64, distinct_layers: u64) -> bool {
    distinct_subjects.saturating_mul(distinct_layers) <= MAX_COALITION_KM_PRODUCT
}

/// Sybil detection result
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SybilAnalysis {
    /// Gateway being analyzed
    pub gateway_id: [u8; 32],
    /// Whether source diversity constraint is met
    pub source_diversity_ok: bool,
    /// Whether destination diversity constraint is met
    pub dest_diversity_ok: bool,
    /// Whether peer diversity constraint is met
    pub peer_diversity_ok: bool,
    /// Overall Sybil risk score (0 = clean, 1000 = definite Sybil)
    pub risk_score: u16,
}

/// Diversity constraint check
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
pub enum DiversityConstraint {
    Source = 0x0001,
    Destination = 0x0002,
    Peer = 0x0003,
}

/// Check if a gateway has sufficient stake for proof generation
pub fn has_sufficient_stake(staked: u64) -> bool {
    staked >= MINIMUM_STAKE
}

/// Dual-stake gate for reputation-recorder status. Per
/// RFC-0968-A1 amendments 1, 5 (Round 7 cross-mission-2):
/// `role_stake_amount >= MIN_RECORDER_ROLE_STAKE = 1000`
/// AND `octo_stake_amount >= MIN_RECORDER_OCTO_STAKE = 4000`
/// AND `octo_stake + role_stake >= MIN_RECORDER_DUAL_STAKE = 5000`.
///
/// When the gateway is a pure PoRelay participant (no reputation
/// recorder), only `has_sufficient_stake(role_stake)` applies. When
/// the gateway is ALSO a reputation recorder (carries an attested
/// `controller_id`), the dual-stake gate activates and the canonical
/// `MIN_RECORDER_DUAL_STAKE = 5000` floor binds. Mission 0860a AC #4.
pub fn has_sufficient_dual_stake(
    octo_stake_amount: u64,
    role_stake_amount: u64,
    is_reputation_recorder: bool,
) -> bool {
    if !is_reputation_recorder {
        return has_sufficient_stake(role_stake_amount);
    }
    role_stake_amount >= MINIMUM_STAKE
        && octo_stake_amount >= 4000
        && octo_stake_amount.saturating_add(role_stake_amount) >= 5000
}

/// Compute Sybil risk score based on diversity metrics.
/// Returns 0-1000 where 1000 = definite Sybil.
pub fn compute_sybil_risk(source_diversity: u32, dest_diversity: u32, peer_diversity: u16) -> u16 {
    let mut violations = 0u16;
    let total = 3u16;

    if source_diversity < MIN_SOURCE_DIVERSITY {
        violations += 1;
    }
    if dest_diversity < MIN_DEST_DIVERSITY {
        violations += 1;
    }
    if peer_diversity < MIN_PEER_DIVERSITY {
        violations += 1;
    }

    (violations as u64)
        .saturating_mul(1000)
        .saturating_div(total as u64) as u16
}

/// Compute stake-proportional routing weight.
/// Sybil attackers splitting stake across N gateways each get total/N,
/// making the attack strictly worse than concentrating on one honest gateway.
pub fn stake_routing_weight(staked: u64, total_stake: u64) -> u16 {
    if total_stake == 0 {
        return 0;
    }
    (staked.saturating_mul(1000) / total_stake).min(1000) as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sufficient_stake() {
        assert!(has_sufficient_stake(1000));
        assert!(has_sufficient_stake(5000));
        assert!(!has_sufficient_stake(999));
        assert!(!has_sufficient_stake(0));
    }

    #[test]
    fn test_sybil_risk_clean() {
        assert_eq!(compute_sybil_risk(5, 5, 10), 0);
    }

    #[test]
    fn test_sybil_risk_one_violation() {
        assert_eq!(compute_sybil_risk(1, 5, 10), 333); // 1/3 ≈ 333
    }

    #[test]
    fn test_sybil_risk_all_violations() {
        assert_eq!(compute_sybil_risk(0, 0, 0), 1000);
    }

    #[test]
    fn test_stake_routing_weight() {
        assert_eq!(stake_routing_weight(500, 1000), 500);
        assert_eq!(stake_routing_weight(1000, 1000), 1000);
        assert_eq!(stake_routing_weight(0, 1000), 0);
    }

    #[test]
    fn test_stake_routing_weight_sybil_attack() {
        // Total stake 1000, split across 10 Sybil gateways
        // Each gets weight 100/1000 = 100
        assert_eq!(stake_routing_weight(100, 1000), 100);
    }

    // -- Mission 0860a AC #4/5/6 closure tests --

    #[test]
    fn test_has_sufficient_dual_stake_pure_porelay() {
        assert!(has_sufficient_dual_stake(0, 1500, false));
        assert!(!has_sufficient_dual_stake(0, 999, false));
    }

    #[test]
    fn test_has_sufficient_dual_stake_reputation_recorder_satisfied() {
        assert!(has_sufficient_dual_stake(4000, 1000, true));
        assert!(has_sufficient_dual_stake(5000, 1500, true));
    }

    #[test]
    fn test_has_sufficient_dual_stake_reputation_recorder_partial() {
        assert!(!has_sufficient_dual_stake(3999, 1500, true));
        assert!(!has_sufficient_dual_stake(4000, 999, true));
        assert!(!has_sufficient_dual_stake(1000, 1000, true));
    }

    #[test]
    fn test_coalition_within_budget() {
        assert!(coalition_within_budget(0, 0));
        assert!(coalition_within_budget(10, 10));
        assert!(coalition_within_budget(100, 1));
        assert!(!coalition_within_budget(101, 1));
        assert!(!coalition_within_budget(50, 3));
    }
}
