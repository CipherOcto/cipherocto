//! Fee Model — RFC-0857 Economic Analysis
//!
//! intent_fee = base_fee × intent_type_multiplier × (1 + priority_premium)
//! priority_premium max 2.0 (200% uplift)
//! Fee distribution: 70/10/10/5/5

use crate::dom::intent::OverlayIntent;

/// Base fee in OCTO units.
pub const BASE_FEE: u64 = 1;

/// Maximum priority premium (200% uplift = 2.0x).
pub const MAX_PRIORITY_PREMIUM: u64 = 20_000; // basis points (2.0 = 20000bp)

/// Intent type multiplier per execution class (BASE_FEE=1, so Archive=1 gives 0.5x base).
pub fn intent_type_multiplier(execution_class: u16) -> u64 {
    match execution_class {
        0x0000 => 20, // CriticalConsensus (10x base)
        0x0001 => 16, // Consensus (8x base)
        0x0002 => 12, // MissionCritical (6x base)
        0x0003 => 8,  // Economic (4x base)
        0x0004 => 4,  // Standard (2x base)
        0x0005 => 2,  // Bulk (1x base)
        0x0006 => 1,  // Archive (0.5x — 1 * 1 = 1 OCTO)
        _ => 2,
    }
}

/// Compute intent fee (RFC-0857 Economic Analysis).
///
/// Returns fee in OCTO units. All arithmetic is integer-only (Class A).
pub fn compute_intent_fee(intent: &OverlayIntent, priority_premium_bp: u64) -> u64 {
    let multiplier = intent_type_multiplier(intent.execution_class);
    let premium = priority_premium_bp.min(MAX_PRIORITY_PREMIUM);
    // fee = base * multiplier * (10000 + premium) / 10000
    BASE_FEE
        .saturating_mul(multiplier)
        .saturating_mul(10_000u64.saturating_add(premium))
        .saturating_div(10_000)
}

/// Fee distribution shares (RFC-0857 Economic Analysis).
pub struct FeeDistribution {
    pub relay_prover: u64, // 70%
    pub orchestrator: u64, // 10%
    pub treasury: u64,     // 10%
    pub burn: u64,         // 5%
    pub governance: u64,   // 5%
}

/// Distribute fee according to the 70/10/10/5/5 model.
pub fn distribute_fee(total_fee: u64) -> FeeDistribution {
    FeeDistribution {
        relay_prover: total_fee.saturating_mul(70).saturating_div(100),
        orchestrator: total_fee.saturating_mul(10).saturating_div(100),
        treasury: total_fee.saturating_mul(10).saturating_div(100),
        burn: total_fee.saturating_mul(5).saturating_div(100),
        governance: total_fee.saturating_mul(5).saturating_div(100),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dom::intent::ExecutionClass;

    fn make_intent(class: ExecutionClass) -> OverlayIntent {
        OverlayIntent {
            intent_id: [0u8; 32],
            intent_type: 0x0001,
            mission_id: [0u8; 32],
            sender_id: [0u8; 32],
            sequence: 1,
            logical_timestamp: 100,
            expiration: 200,
            payload_root: [0u8; 32],
            economic_weight: 100,
            execution_class: class as u16,
            signature: [0u8; 64],
        }
    }

    #[test]
    fn test_fee_critical_consensus() {
        let intent = make_intent(ExecutionClass::CriticalConsensus);
        let fee = compute_intent_fee(&intent, 0);
        assert_eq!(fee, 20); // 1 * 20 * 1.0
    }

    #[test]
    fn test_fee_standard_no_premium() {
        let intent = make_intent(ExecutionClass::Standard);
        let fee = compute_intent_fee(&intent, 0);
        assert_eq!(fee, 4); // 1 * 4 * 1.0
    }

    #[test]
    fn test_fee_with_premium() {
        let intent = make_intent(ExecutionClass::Standard);
        let fee = compute_intent_fee(&intent, 1000); // 10% premium
                                                     // 1 * 4 * 11000 / 10000 = 44000 / 10000 = 4 (integer truncation)
        assert_eq!(fee, 4);
    }

    #[test]
    fn test_fee_premium_clamped() {
        let intent = make_intent(ExecutionClass::Standard);
        let fee = compute_intent_fee(&intent, 25000); // 250% premium (clamped to 200% cap)
                                                      // 1 * 4 * 30000 / 10000 = 120000 / 10000 = 12
        assert_eq!(fee, 12);
    }

    #[test]
    fn test_fee_distribution() {
        let dist = distribute_fee(100);
        assert_eq!(dist.relay_prover, 70);
        assert_eq!(dist.orchestrator, 10);
        assert_eq!(dist.treasury, 10);
        assert_eq!(dist.burn, 5);
        assert_eq!(dist.governance, 5);
    }

    #[test]
    fn test_fee_multiplier_values() {
        assert_eq!(intent_type_multiplier(0x0000), 20); // CriticalConsensus
        assert_eq!(intent_type_multiplier(0x0001), 16); // Consensus
        assert_eq!(intent_type_multiplier(0x0003), 8); // Economic
        assert_eq!(intent_type_multiplier(0x0005), 2); // Bulk
    }
}
