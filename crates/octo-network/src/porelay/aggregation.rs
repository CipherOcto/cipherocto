//! Recursive Proof Aggregation (RFC-0860 §5)

use serde::{Deserialize, Serialize};

/// Aggregation hierarchy levels
pub const LEVEL_LEAF: u8 = 0;
pub const LEVEL_WINDOW: u8 = 1;
pub const LEVEL_REGIONAL: u8 = 2;
pub const LEVEL_GLOBAL: u8 = 3;

/// Aggregated Relay Proof — compresses multiple proofs via recursive aggregation.
///
/// 10 fields per RFC-0860 §5.2.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[repr(C)]
pub struct AggregatedRelayProof {
    /// Aggregation level (0 = leaf, 3 = global)
    pub level: u8,
    /// Epoch this aggregation covers
    pub epoch: u64,
    /// Scope identifier (gateway_id for L1, region_id for L2, network_id for L3)
    pub scope: [u8; 32],
    /// Number of individual proofs aggregated
    pub proof_count: u32,
    /// Total envelopes relayed across all aggregated proofs
    pub total_envelopes: u64,
    /// Total bytes relayed
    pub total_bytes: u64,
    /// Average availability score (basis points)
    pub average_availability: u16,
    /// Merkle root of child proofs
    pub children_root: [u8; 32],
    /// STARK proof (via RFC-0854 DPS) proving all children are valid
    pub proof_blob: Vec<u8>,
    /// Ed25519 signature by aggregator
    pub signature: Vec<u8>,
}

impl AggregatedRelayProof {
    /// Check if this is a leaf-level proof
    pub fn is_leaf(&self) -> bool {
        self.level == LEVEL_LEAF
    }

    /// Check if this is a global proof
    pub fn is_global(&self) -> bool {
        self.level == LEVEL_GLOBAL
    }

    /// Compute canonical signing bytes
    pub fn to_signing_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.push(self.level);
        buf.extend_from_slice(&self.epoch.to_be_bytes());
        buf.extend_from_slice(&self.scope);
        buf.extend_from_slice(&self.proof_count.to_be_bytes());
        buf.extend_from_slice(&self.total_envelopes.to_be_bytes());
        buf.extend_from_slice(&self.total_bytes.to_be_bytes());
        buf.extend_from_slice(&self.average_availability.to_be_bytes());
        buf.extend_from_slice(&self.children_root);
        buf
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_aggregate(level: u8, proof_count: u32) -> AggregatedRelayProof {
        AggregatedRelayProof {
            level,
            epoch: 1,
            scope: [0u8; 32],
            proof_count,
            total_envelopes: proof_count as u64 * 100,
            total_bytes: proof_count as u64 * 102400,
            average_availability: 950,
            children_root: [0u8; 32],
            proof_blob: vec![],
            signature: vec![0u8; 64],
        }
    }

    #[test]
    fn test_leaf_level() {
        let agg = make_aggregate(LEVEL_LEAF, 10);
        assert!(agg.is_leaf());
        assert!(!agg.is_global());
    }

    #[test]
    fn test_global_level() {
        let agg = make_aggregate(LEVEL_GLOBAL, 1000);
        assert!(!agg.is_leaf());
        assert!(agg.is_global());
    }

    #[test]
    fn test_signing_bytes_size() {
        let agg = make_aggregate(1, 100);
        // 1 + 8 + 32 + 4 + 8 + 8 + 2 + 32 = 95
        assert_eq!(agg.to_signing_bytes().len(), 95);
    }

    #[test]
    fn test_aggregate_metrics() {
        let agg = make_aggregate(LEVEL_WINDOW, 50);
        assert_eq!(agg.total_envelopes, 5000);
        assert_eq!(agg.total_bytes, 5_120_000);
        assert_eq!(agg.average_availability, 950);
    }
}
