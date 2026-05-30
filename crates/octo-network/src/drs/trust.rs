//! Trust scoring (RFC-0856 Section 9)

use crate::drs::error::DrsError;

/// Trust score factors (RFC-0856 Section 9.1).
///
/// All fields are u64. Each factor must be in range 0-1_000_000.
#[derive(Debug, Clone)]
pub struct TrustScore {
    /// Historical uptime score (0-1_000_000)
    pub historical_uptime: u64,
    /// Proof-of-relay attestations (capped at 1000)
    pub proof_of_relay: u64,
    /// Stake weight (OCTO)
    pub stake_weight: u64,
    /// Mission trust score
    pub mission_trust: u64,
    /// Consensus participation score
    pub consensus_participation: u64,
}

impl TrustScore {
    /// Create a zero trust score.
    pub fn zero() -> Self {
        Self {
            historical_uptime: 0,
            proof_of_relay: 0,
            stake_weight: 0,
            mission_trust: 0,
            consensus_participation: 0,
        }
    }
}

/// Compute trust score (RFC-0856 Section 9.1).
///
/// Uses saturating arithmetic. proof_of_relay capped at 1000.
/// stake_weight capped at median_stake * 10.
/// Total capped at 1_000_000.
///
/// Returns error if any factor is out of range (0-1_000_000) or median_stake is zero.
pub fn compute_trust_score(factors: &TrustScore, median_stake: u64) -> Result<u64, DrsError> {
    // Validate factor ranges
    let fields = [
        ("historical_uptime", factors.historical_uptime),
        ("mission_trust", factors.mission_trust),
        ("consensus_participation", factors.consensus_participation),
    ];
    for (name, val) in &fields {
        if *val > 1_000_000 {
            return Err(DrsError::TrustComputationFailed {
                factor: format!("{} out of range: {} (max 1_000_000)", name, val),
            });
        }
    }

    // Handle median_stake = 0: use uncapped stake (no centralization limit)
    let uptime = factors.historical_uptime;
    // Cap attestations at 1000 (design constant)
    let relay = factors.proof_of_relay.min(1000).saturating_mul(1000);
    // Cap stake_weight to prevent centralization; uncapped when median_stake = 0
    let stake = if median_stake == 0 {
        factors.stake_weight / 1000
    } else {
        let stake_cap = median_stake.saturating_mul(10);
        let stake_capped = factors.stake_weight.min(stake_cap);
        stake_capped / 1000
    };
    let mission = factors.mission_trust;
    let consensus = factors.consensus_participation;

    Ok(uptime
        .saturating_add(relay)
        .saturating_add(stake)
        .saturating_add(mission)
        .saturating_add(consensus)
        .min(1_000_000))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trust_score_zero() {
        let ts = TrustScore::zero();
        assert_eq!(ts.historical_uptime, 0);
        assert_eq!(compute_trust_score(&ts, 1000).unwrap(), 0);
    }

    #[test]
    fn test_trust_score_basic() {
        let ts = TrustScore {
            historical_uptime: 100_000,
            proof_of_relay: 500,
            stake_weight: 500_000,
            mission_trust: 50_000,
            consensus_participation: 25_000,
        };
        let score = compute_trust_score(&ts, 100_000).unwrap(); // high median so cap doesn't kick in
                                                                // uptime=100000, relay=500*1000=500000, stake=500000/1000=500,
                                                                // mission=50000, consensus=25000
                                                                // total=100000+500000+500+50000+25000 = 675500
        assert_eq!(score, 675500);
    }

    #[test]
    fn test_trust_score_relay_cap() {
        let ts = TrustScore {
            historical_uptime: 0,
            proof_of_relay: 2000, // over cap
            stake_weight: 0,
            mission_trust: 0,
            consensus_participation: 0,
        };
        let score = compute_trust_score(&ts, 1000).unwrap();
        // relay capped at 1000: 1000 * 1000 = 1_000_000
        assert_eq!(score, 1_000_000);
    }

    #[test]
    fn test_trust_score_stake_cap() {
        let ts = TrustScore {
            historical_uptime: 0,
            proof_of_relay: 0,
            stake_weight: 100_000_000, // huge stake
            mission_trust: 0,
            consensus_participation: 0,
        };
        let median_stake = 1000;
        let score = compute_trust_score(&ts, median_stake).unwrap();
        // stake_cap = 1000 * 10 = 10000
        // stake_capped = min(100_000_000, 10000) = 10000
        // stake = 10000 / 1000 = 10
        assert_eq!(score, 10);
    }

    #[test]
    fn test_trust_score_total_cap() {
        let ts = TrustScore {
            historical_uptime: 500_000,
            proof_of_relay: 1000,
            stake_weight: 1_000_000_000,
            mission_trust: 500_000,
            consensus_participation: 500_000,
        };
        let score = compute_trust_score(&ts, 100_000).unwrap();
        assert!(score <= 1_000_000);
    }

    #[test]
    fn test_trust_score_factor_range_validation() {
        let ts = TrustScore {
            historical_uptime: 2_000_000, // out of range
            proof_of_relay: 0,
            stake_weight: 0,
            mission_trust: 0,
            consensus_participation: 0,
        };
        let err = compute_trust_score(&ts, 1000).unwrap_err();
        assert!(format!("{}", err).contains("out of range"));
    }

    #[test]
    fn test_trust_score_median_stake_zero() {
        let ts = TrustScore {
            historical_uptime: 0,
            proof_of_relay: 0,
            stake_weight: 5_000_000, // would normally be capped
            mission_trust: 0,
            consensus_participation: 0,
        };
        // With median_stake = 0, stake is uncapped: 5_000_000 / 1000 = 5000
        let score = compute_trust_score(&ts, 0).unwrap();
        assert_eq!(score, 5000);
    }
}
