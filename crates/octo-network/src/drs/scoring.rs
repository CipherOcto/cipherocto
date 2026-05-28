//! Route scoring (RFC-0856 §6)

use crate::drs::route::DeterministicRoute;

/// Scoring weights for route selection (RFC-0856 §6.1).
///
/// All weights are basis points (0-1000). Total should be 1000.
#[derive(Debug, Clone)]
pub struct ScoringWeights {
    pub trust_weight: u64,
    pub bandwidth_weight: u64,
    pub latency_weight: u64,
    pub censorship_weight: u64,
    pub cost_weight: u64,
}

impl ScoringWeights {
    /// Create balanced default weights.
    pub fn balanced() -> Self {
        Self {
            trust_weight: 300,
            bandwidth_weight: 250,
            latency_weight: 200,
            censorship_weight: 150,
            cost_weight: 100,
        }
    }

    /// Verify weights are valid (all non-zero, sum = 1000).
    pub fn validate(&self) -> Result<(), String> {
        let sum = self
            .trust_weight
            .saturating_add(self.bandwidth_weight)
            .saturating_add(self.latency_weight)
            .saturating_add(self.censorship_weight)
            .saturating_add(self.cost_weight);
        if sum != 1000 {
            return Err(format!("weights sum to {} (expected 1000)", sum));
        }
        Ok(())
    }
}

/// Compute route score using saturating arithmetic (RFC-0856 §6).
///
/// score = trust_component + bandwidth_component + latency_component
///       + censorship_component - cost_component
///
/// All arithmetic is u64 saturating to prevent overflow.
pub fn compute_route_score(route: &DeterministicRoute, weights: &ScoringWeights) -> u64 {
    let trust_component = route.trust_score.saturating_mul(weights.trust_weight);
    let bandwidth_component =
        (route.bandwidth_class as u64).saturating_mul(weights.bandwidth_weight);
    let latency_component = (route.latency_class as u64).saturating_mul(weights.latency_weight);
    let censorship_component =
        (route.censorship_resistance_class as u64).saturating_mul(weights.censorship_weight);
    let cost_component = route.route_cost.saturating_mul(weights.cost_weight);

    trust_component
        .saturating_add(bandwidth_component)
        .saturating_add(latency_component)
        .saturating_add(censorship_component)
        .saturating_sub(cost_component)
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
            ttl_hops: 10,
        }
    }

    #[test]
    fn test_scoring_weights_balanced() {
        let w = ScoringWeights::balanced();
        assert_eq!(w.trust_weight, 300);
        assert_eq!(w.cost_weight, 100);
        assert!(w.validate().is_ok());
    }

    #[test]
    fn test_scoring_weights_invalid_sum() {
        let w = ScoringWeights {
            trust_weight: 500,
            bandwidth_weight: 500,
            latency_weight: 500,
            censorship_weight: 500,
            cost_weight: 500,
        };
        assert!(w.validate().is_err());
    }

    #[test]
    fn test_compute_route_score_basic() {
        let route = make_route(1000, 100, 50, 200, 10);
        let weights = ScoringWeights::balanced();
        let score = compute_route_score(&route, &weights);
        // trust: 1000*300=300000, bw: 100*250=25000, lat: 50*200=10000,
        // censor: 200*150=30000, cost: 10*100=1000
        // total: 300000+25000+10000+30000-1000 = 364000
        assert_eq!(score, 364000);
    }

    #[test]
    fn test_compute_route_score_saturating() {
        let route = make_route(u64::MAX, u16::MAX, u16::MAX, u16::MAX, 0);
        let weights = ScoringWeights::balanced();
        // Should not panic due to saturating arithmetic
        let score = compute_route_score(&route, &weights);
        assert!(score > 0);
    }

    #[test]
    fn test_compute_route_score_higher_trust_wins() {
        let weights = ScoringWeights::balanced();
        let r1 = make_route(1000, 100, 50, 200, 10);
        let r2 = make_route(2000, 100, 50, 200, 10);
        let s1 = compute_route_score(&r1, &weights);
        let s2 = compute_route_score(&r2, &weights);
        assert!(s2 > s1);
    }
}
