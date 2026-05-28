//! Trust scoring (RFC-0856 §9)

/// Trust score factors (RFC-0856 §9.1).
///
/// All fields are u64 for Class A compliance.
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

/// Compute trust score (RFC-0856 §9.1).
///
/// Uses saturating arithmetic. proof_of_relay capped at 1000.
/// stake_weight capped at median_stake * 10.
/// Total capped at 1_000_000.
pub fn compute_trust_score(factors: &TrustScore, median_stake: u64) -> u64 {
    let uptime = factors.historical_uptime;
    // Cap attestations at 1000 (design constant)
    let relay = factors.proof_of_relay.min(1000).saturating_mul(1000);
    // Cap stake_weight to prevent centralization
    let stake_cap = median_stake.saturating_mul(10);
    let stake_capped = factors.stake_weight.min(stake_cap);
    let stake = stake_capped / 1000;
    let mission = factors.mission_trust;
    let consensus = factors.consensus_participation;

    uptime
        .saturating_add(relay)
        .saturating_add(stake)
        .saturating_add(mission)
        .saturating_add(consensus)
        .min(1_000_000)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trust_score_zero() {
        let ts = TrustScore::zero();
        assert_eq!(ts.historical_uptime, 0);
        assert_eq!(compute_trust_score(&ts, 1000), 0);
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
        let score = compute_trust_score(&ts, 100_000); // high median so cap doesn't kick in
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
        let score = compute_trust_score(&ts, 1000);
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
        let score = compute_trust_score(&ts, median_stake);
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
        let score = compute_trust_score(&ts, 100_000);
        assert!(score <= 1_000_000);
    }
}
