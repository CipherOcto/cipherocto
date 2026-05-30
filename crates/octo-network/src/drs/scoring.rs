//! Route scoring (RFC-0856 Section 6)

use crate::drs::error::DrsError;
use crate::drs::route::DeterministicRoute;

/// Scoring weights for route selection (RFC-0856 Section 6.1).
///
/// All weights are micro-units (0-1_000_000). Total must equal 1_000_000.
/// `activation_epoch` ensures weight changes take effect at a deterministic future epoch.
#[derive(Debug, Clone)]
pub struct ScoringWeights {
    pub trust_weight: u64,
    pub bandwidth_weight: u64,
    pub latency_weight: u64,
    pub censorship_weight: u64,
    pub cost_weight: u64,
    /// Epoch at which these weights become active (RFC-0856 Section 6.1)
    pub activation_epoch: u64,
}

impl ScoringWeights {
    /// Create balanced default weights (sum = 1_000_000).
    pub fn balanced() -> Self {
        Self {
            trust_weight: 300_000,
            bandwidth_weight: 250_000,
            latency_weight: 200_000,
            censorship_weight: 150_000,
            cost_weight: 100_000,
            activation_epoch: 0,
        }
    }

    /// Verify weights are valid: all non-zero, sum = 1_000_000.
    pub fn validate(&self) -> Result<(), DrsError> {
        let weights = [
            ("trust_weight", self.trust_weight),
            ("bandwidth_weight", self.bandwidth_weight),
            ("latency_weight", self.latency_weight),
            ("censorship_weight", self.censorship_weight),
            ("cost_weight", self.cost_weight),
        ];
        for (name, w) in &weights {
            if *w == 0 {
                return Err(DrsError::InvalidWeights {
                    field: format!("{} must be non-zero", name),
                });
            }
        }
        let sum = self
            .trust_weight
            .saturating_add(self.bandwidth_weight)
            .saturating_add(self.latency_weight)
            .saturating_add(self.censorship_weight)
            .saturating_add(self.cost_weight);
        if sum != 1_000_000 {
            return Err(DrsError::InvalidWeights {
                field: format!("weights sum to {} (expected 1_000_000)", sum),
            });
        }
        Ok(())
    }
}

/// Compute route score using saturating arithmetic (RFC-0856 Section 6).
///
/// score = trust_component + bandwidth_component + latency_component
///       + censorship_component - cost_component
///
/// Validates weights before scoring. All arithmetic is u64 saturating to prevent overflow.
pub fn compute_route_score(
    route: &DeterministicRoute,
    weights: &ScoringWeights,
) -> Result<u64, DrsError> {
    weights.validate()?;
    let trust_component = route.trust_score.saturating_mul(weights.trust_weight);
    let bandwidth_component =
        (route.bandwidth_class as u64).saturating_mul(weights.bandwidth_weight);
    let latency_component = (route.latency_class as u64).saturating_mul(weights.latency_weight);
    let censorship_component =
        (route.censorship_resistance_class as u64).saturating_mul(weights.censorship_weight);
    let cost_component = route.route_cost.saturating_mul(weights.cost_weight);

    Ok(trust_component
        .saturating_add(bandwidth_component)
        .saturating_add(latency_component)
        .saturating_add(censorship_component)
        .saturating_sub(cost_component))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_route(trust: u64, bw: u16, lat: u16, censor: u16, cost: u64) -> DeterministicRoute {
        DeterministicRoute {
            route_id: [0u8; 32],
            source_gateway: [0u8; 32],
            destination_gateway: [0u8; 32],
            next_hop: [0u8; 32],
            transport_vector_root: [0u8; 32],
            trust_score: trust,
            bandwidth_class: bw,
            latency_class: lat,
            censorship_resistance_class: censor,
            route_cost: cost,
            route_epoch: 0,
            valid_until_epoch: 0,
            ttl_hops: 10,
            signature: [0u8; 64],
        }
    }

    #[test]
    fn test_scoring_weights_balanced() {
        let w = ScoringWeights::balanced();
        assert_eq!(w.trust_weight, 300_000);
        assert_eq!(w.cost_weight, 100_000);
        assert!(w.validate().is_ok());
    }

    #[test]
    fn test_scoring_weights_invalid_sum() {
        let w = ScoringWeights {
            trust_weight: 500_000,
            bandwidth_weight: 500_000,
            latency_weight: 500_000,
            censorship_weight: 500_000,
            cost_weight: 500_000,
            activation_epoch: 0,
        };
        assert!(w.validate().is_err());
    }

    #[test]
    fn test_scoring_weights_zero_weight_rejected() {
        let w = ScoringWeights {
            trust_weight: 0,
            bandwidth_weight: 250_000,
            latency_weight: 250_000,
            censorship_weight: 250_000,
            cost_weight: 250_000,
            activation_epoch: 0,
        };
        let err = w.validate().unwrap_err();
        assert!(format!("{}", err).contains("non-zero"));
    }

    #[test]
    fn test_compute_route_score_basic() {
        let route = make_route(1000, 100, 50, 200, 10);
        let weights = ScoringWeights::balanced();
        let score = compute_route_score(&route, &weights).unwrap();
        // trust: 1000*300000=300_000_000, bw: 100*250000=25_000_000,
        // lat: 50*200000=10_000_000, censor: 200*150000=30_000_000,
        // cost: 10*100000=1_000_000
        // total: 300_000_000+25_000_000+10_000_000+30_000_000-1_000_000 = 364_000_000
        assert_eq!(score, 364_000_000);
    }

    #[test]
    fn test_compute_route_score_saturating() {
        let route = make_route(u64::MAX, u16::MAX, u16::MAX, u16::MAX, 0);
        let weights = ScoringWeights::balanced();
        // Should not panic due to saturating arithmetic
        let score = compute_route_score(&route, &weights).unwrap();
        assert!(score > 0);
    }

    #[test]
    fn test_compute_route_score_higher_trust_wins() {
        let weights = ScoringWeights::balanced();
        let r1 = make_route(1000, 100, 50, 200, 10);
        let r2 = make_route(2000, 100, 50, 200, 10);
        let s1 = compute_route_score(&r1, &weights).unwrap();
        let s2 = compute_route_score(&r2, &weights).unwrap();
        assert!(s2 > s1);
    }

    #[test]
    fn test_compute_route_score_invalid_weights() {
        let route = make_route(1000, 100, 50, 200, 10);
        let weights = ScoringWeights {
            trust_weight: 0,
            bandwidth_weight: 250_000,
            latency_weight: 250_000,
            censorship_weight: 250_000,
            cost_weight: 250_000,
            activation_epoch: 0,
        };
        assert!(compute_route_score(&route, &weights).is_err());
    }
}
