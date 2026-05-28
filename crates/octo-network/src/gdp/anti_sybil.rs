//! GDP Anti-Sybil Mechanisms (RFC-0851 §11)
//!
//! Stake-gated discovery, diversity constraints, and Sybil cluster detection.
//! All arithmetic uses integer/saturating operations (RFC-0008 Class A).

use serde::{Deserialize, Serialize};

use super::types::DiscoveryScope;
use super::GdpError;

/// Minimum OCTO stake per scope (RFC-0851 §11.1)
pub fn stake_requirement(scope: &DiscoveryScope) -> StakeGate {
    match scope {
        DiscoveryScope::Local => StakeGate { octo: 0, octo_b: 0 },
        DiscoveryScope::Regional => StakeGate {
            octo: 500,
            octo_b: 50,
        },
        DiscoveryScope::Mission => StakeGate {
            octo: 1000,
            octo_b: 100,
        },
        DiscoveryScope::Global => StakeGate {
            octo: 1000,
            octo_b: 100,
        },
        DiscoveryScope::Private => StakeGate { octo: 0, octo_b: 0 },
        DiscoveryScope::Consensus => StakeGate {
            octo: 1000,
            octo_b: 200,
        },
    }
}

/// Stake gate for a discovery scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct StakeGate {
    pub octo: u64,
    pub octo_b: u64,
}

impl StakeGate {
    /// Check if a gateway has sufficient stake.
    pub fn is_sufficient(&self, have_octo: u64, have_octo_b: u64) -> bool {
        have_octo >= self.octo && have_octo_b >= self.octo_b
    }
}

/// Diversity score for eclipse attack resistance (RFC-0851 §11.2)
///
/// Formula: `diversity_score = transport_diversity * 3 + geographic_diversity * 2 + trust_diversity * 1`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiversityScore {
    /// Number of distinct transports (e.g., TCP, QUIC, WebSocket)
    pub transport_diversity: u32,
    /// Number of distinct geographic regions
    pub geographic_diversity: u32,
    /// Number of distinct trust sources
    pub trust_diversity: u32,
}

impl DiversityScore {
    /// Compute weighted diversity score.
    pub fn score(&self) -> u32 {
        self.transport_diversity.saturating_mul(3)
            + self.geographic_diversity.saturating_mul(2)
            + self.trust_diversity
    }

    /// Minimum diversity thresholds per scope (RFC-0851 §11.2).
    pub fn meets_minimum(&self, scope: &DiscoveryScope) -> bool {
        match scope {
            DiscoveryScope::Local => true, // No minimum
            DiscoveryScope::Regional => self.transport_diversity >= 2,
            DiscoveryScope::Global => {
                self.transport_diversity >= 3 && self.geographic_diversity >= 2
            }
            _ => true,
        }
    }

    /// Deprioritize non-compliant gateways (score = 0).
    pub fn effective_score(&self, scope: &DiscoveryScope) -> u32 {
        if self.meets_minimum(scope) {
            self.score()
        } else {
            0 // Deprioritized, not rejected
        }
    }
}

/// Minimum diversity thresholds per scope.
pub fn min_transport_diversity(scope: &DiscoveryScope) -> u32 {
    match scope {
        DiscoveryScope::Local => 0,
        DiscoveryScope::Regional => 2,
        DiscoveryScope::Global => 3,
        _ => 0,
    }
}

pub fn min_geographic_diversity(scope: &DiscoveryScope) -> u32 {
    match scope {
        DiscoveryScope::Global => 2,
        _ => 0,
    }
}

/// Sybil cluster detection via correlated behavior analysis (RFC-0851 §11.3)
///
/// Heuristic: gateways with identical advertisement hashes, sequential
/// gateway_ids, or identical network fingerprints are flagged as potential
/// Sybil clusters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SybilDetector {
    /// Known gateway advertisements (gateway_id -> advertisement_hash)
    pub advertisements: std::collections::BTreeMap<[u8; 32], [u8; 32]>,
    /// Detected clusters (cluster_id -> member gateway_ids)
    pub clusters: Vec<SybilCluster>,
}

/// A detected Sybil cluster.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SybilCluster {
    /// Unique cluster identifier
    pub cluster_id: u32,
    /// Gateway IDs in this cluster
    pub members: Vec<[u8; 32]>,
    /// Reason for detection
    pub reason: SybilReason,
}

/// Reason a cluster was flagged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u16)]
pub enum SybilReason {
    DuplicateAdvertisement = 0x0001,
    SequentialGatewayIds = 0x0002,
    IdenticalNetworkFingerprint = 0x0003,
}

impl SybilDetector {
    pub fn new() -> Self {
        Self {
            advertisements: std::collections::BTreeMap::new(),
            clusters: Vec::new(),
        }
    }

    /// Register a gateway advertisement. Returns true if duplicate detected.
    pub fn register(&mut self, gateway_id: [u8; 32], adv_hash: [u8; 32]) -> bool {
        // Check for duplicate advertisement hash
        let is_duplicate = self
            .advertisements
            .values()
            .any(|existing| *existing == adv_hash);

        self.advertisements.insert(gateway_id, adv_hash);
        is_duplicate
    }

    /// Detect sequential gateway IDs (BLAKE3-derived IDs should be random).
    /// Returns pairs of gateways with sequential IDs.
    pub fn detect_sequential_ids(&self) -> Vec<([u8; 32], [u8; 32])> {
        let mut sequential_pairs = Vec::new();
        let ids: Vec<[u8; 32]> = self.advertisements.keys().copied().collect();

        for i in 0..ids.len() {
            for j in (i + 1)..ids.len() {
                // Check if IDs differ only in the last 4 bytes (sequential)
                if ids[i][..28] == ids[j][..28] {
                    let a = u32::from_be_bytes(ids[i][28..32].try_into().unwrap());
                    let b = u32::from_be_bytes(ids[j][28..32].try_into().unwrap());
                    if a.abs_diff(b) <= 10 {
                        sequential_pairs.push((ids[i], ids[j]));
                    }
                }
            }
        }
        sequential_pairs
    }

    /// Total number of registered gateways.
    pub fn gateway_count(&self) -> usize {
        self.advertisements.len()
    }
}

impl Default for SybilDetector {
    fn default() -> Self {
        Self::new()
    }
}

/// Compute BLAKE3-based network fingerprint for a gateway.
/// Used to detect identical network configurations.
pub fn compute_network_fingerprint(
    transport_types: &[u16],
    capabilities: u64,
    platform_mask: u64,
) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    for t in transport_types {
        hasher.update(&t.to_be_bytes());
    }
    hasher.update(&capabilities.to_be_bytes());
    hasher.update(&platform_mask.to_be_bytes());
    *hasher.finalize().as_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stake_gate_sufficient() {
        let gate = StakeGate {
            octo: 500,
            octo_b: 50,
        };
        assert!(gate.is_sufficient(500, 50));
        assert!(gate.is_sufficient(1000, 100));
        assert!(!gate.is_sufficient(499, 50));
        assert!(!gate.is_sufficient(500, 49));
    }

    #[test]
    fn test_stake_requirement_per_scope() {
        assert_eq!(stake_requirement(&DiscoveryScope::Local).octo, 0);
        assert_eq!(stake_requirement(&DiscoveryScope::Regional).octo, 500);
        assert_eq!(stake_requirement(&DiscoveryScope::Global).octo, 1000);
        assert_eq!(stake_requirement(&DiscoveryScope::Consensus).octo_b, 200);
    }

    #[test]
    fn test_diversity_score_formula() {
        let score = DiversityScore {
            transport_diversity: 3,
            geographic_diversity: 2,
            trust_diversity: 1,
        };
        // 3*3 + 2*2 + 1 = 9 + 4 + 1 = 14
        assert_eq!(score.score(), 14);
    }

    #[test]
    fn test_diversity_minimum_thresholds() {
        let regional_ok = DiversityScore {
            transport_diversity: 2,
            geographic_diversity: 1,
            trust_diversity: 1,
        };
        assert!(regional_ok.meets_minimum(&DiscoveryScope::Regional));

        let regional_bad = DiversityScore {
            transport_diversity: 1,
            geographic_diversity: 1,
            trust_diversity: 1,
        };
        assert!(!regional_bad.meets_minimum(&DiscoveryScope::Regional));
        assert_eq!(regional_bad.effective_score(&DiscoveryScope::Regional), 0);
    }

    #[test]
    fn test_diversity_global_thresholds() {
        let global_ok = DiversityScore {
            transport_diversity: 3,
            geographic_diversity: 2,
            trust_diversity: 1,
        };
        assert!(global_ok.meets_minimum(&DiscoveryScope::Global));

        let global_bad = DiversityScore {
            transport_diversity: 3,
            geographic_diversity: 1, // needs >= 2
            trust_diversity: 1,
        };
        assert!(!global_bad.meets_minimum(&DiscoveryScope::Global));
    }

    #[test]
    fn test_sybil_detector_duplicate_adv() {
        let mut detector = SybilDetector::new();
        let hash = [0xAAu8; 32];
        assert!(!detector.register([0x01u8; 32], hash));
        assert!(detector.register([0x02u8; 32], hash)); // duplicate
    }

    #[test]
    fn test_sybil_detector_sequential_ids() {
        let mut detector = SybilDetector::new();
        // Sequential IDs (differ only in last 4 bytes)
        let mut id1 = [0u8; 32];
        id1[28..32].copy_from_slice(&1u32.to_be_bytes());
        let mut id2 = [0u8; 32];
        id2[28..32].copy_from_slice(&2u32.to_be_bytes());
        let mut id3 = [0u8; 32];
        id3[28..32].copy_from_slice(&100u32.to_be_bytes()); // not sequential

        detector.register(id1, [0xAAu8; 32]);
        detector.register(id2, [0xBBu8; 32]);
        detector.register(id3, [0xCCu8; 32]);

        let pairs = detector.detect_sequential_ids();
        assert_eq!(pairs.len(), 1); // id1-id2 pair
    }

    #[test]
    fn test_sybil_detector_count() {
        let mut detector = SybilDetector::new();
        detector.register([0x01u8; 32], [0xAAu8; 32]);
        detector.register([0x02u8; 32], [0xBBu8; 32]);
        assert_eq!(detector.gateway_count(), 2);
    }

    #[test]
    fn test_network_fingerprint_deterministic() {
        let fp1 = compute_network_fingerprint(&[1, 2, 3], 0xFF, 0x0F);
        let fp2 = compute_network_fingerprint(&[1, 2, 3], 0xFF, 0x0F);
        assert_eq!(fp1, fp2);

        let fp3 = compute_network_fingerprint(&[1, 2, 4], 0xFF, 0x0F);
        assert_ne!(fp1, fp3);
    }

    #[test]
    fn test_min_diversity_thresholds() {
        assert_eq!(min_transport_diversity(&DiscoveryScope::Local), 0);
        assert_eq!(min_transport_diversity(&DiscoveryScope::Regional), 2);
        assert_eq!(min_transport_diversity(&DiscoveryScope::Global), 3);
        assert_eq!(min_geographic_diversity(&DiscoveryScope::Global), 2);
        assert_eq!(min_geographic_diversity(&DiscoveryScope::Regional), 0);
    }
}
