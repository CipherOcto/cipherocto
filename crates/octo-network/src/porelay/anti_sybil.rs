//! Anti-Sybil Mechanisms (RFC-0860 §7)

use serde::{Deserialize, Serialize};

/// Minimum source diversity threshold
pub const MIN_SOURCE_DIVERSITY: u32 = 2;

/// Minimum destination diversity threshold
pub const MIN_DEST_DIVERSITY: u32 = 2;

/// Minimum peer diversity threshold
pub const MIN_PEER_DIVERSITY: u16 = 3;

/// Minimum OCTO-B stake for proof generation
pub const MINIMUM_STAKE: u64 = 1000;

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

/// Compute Sybil risk score based on diversity metrics.
/// Returns 0-1000 where 1000 = definite Sybil.
pub fn compute_sybil_risk(source_diversity: u32, dest_diversity: u32, peer_diversity: u16) -> u16 {
    let mut violations = 0u16;
    let mut total = 3u16;

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
}
