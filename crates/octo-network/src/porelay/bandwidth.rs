//! Bandwidth Proof (RFC-0860 §3.3)

use serde::{Deserialize, Serialize};

/// Bandwidth Proof — proves the volume of data a gateway relayed.
///
/// 9 fields per RFC-0860 §3.3.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[repr(C)]
pub struct BandwidthProof {
    /// Gateway being attested
    pub gateway_id: [u8; 32],
    /// Time window (epoch range)
    pub window_start: u64,
    pub window_end: u64,
    /// Number of envelopes relayed
    pub envelope_count: u64,
    /// Total bytes relayed (sum of canonical envelope sizes)
    pub bytes_relayed: u64,
    /// Number of distinct source peers served
    pub source_diversity: u32,
    /// Number of distinct destinations served
    pub destination_diversity: u32,
    /// Merkle root of (envelope_hash, byte_count) pairs
    pub relay_merkle_root: [u8; 32],
    /// Ed25519 signature over all above fields
    pub signature: Vec<u8>,
}

impl BandwidthProof {
    /// Compute canonical signing bytes
    pub fn to_signing_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(32 + 8 + 8 + 8 + 8 + 4 + 4 + 32);
        buf.extend_from_slice(&self.gateway_id);
        buf.extend_from_slice(&self.window_start.to_be_bytes());
        buf.extend_from_slice(&self.window_end.to_be_bytes());
        buf.extend_from_slice(&self.envelope_count.to_be_bytes());
        buf.extend_from_slice(&self.bytes_relayed.to_be_bytes());
        buf.extend_from_slice(&self.source_diversity.to_be_bytes());
        buf.extend_from_slice(&self.destination_diversity.to_be_bytes());
        buf.extend_from_slice(&self.relay_merkle_root);
        buf
    }

    /// Compute bandwidth efficiency score (bytes per envelope, 0-1000 scale)
    pub fn efficiency_score(&self) -> u16 {
        if self.envelope_count == 0 {
            return 0;
        }
        let avg_size = self.bytes_relayed / self.envelope_count;
        // Normalize: 1KB per envelope = 1000
        let score = avg_size.saturating_mul(1000) / 1024;
        score.min(1000) as u16
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signing_bytes_size() {
        let proof = BandwidthProof {
            gateway_id: [0u8; 32],
            window_start: 0,
            window_end: 100,
            envelope_count: 50,
            bytes_relayed: 51200,
            source_diversity: 5,
            destination_diversity: 3,
            relay_merkle_root: [0u8; 32],
            signature: vec![0u8; 64],
        };
        assert_eq!(proof.to_signing_bytes().len(), 104);
    }

    #[test]
    fn test_efficiency_score_1kb() {
        let proof = BandwidthProof {
            gateway_id: [0u8; 32],
            window_start: 0,
            window_end: 100,
            envelope_count: 100,
            bytes_relayed: 102400, // 1KB per envelope
            source_diversity: 5,
            destination_diversity: 3,
            relay_merkle_root: [0u8; 32],
            signature: vec![0u8; 64],
        };
        assert_eq!(proof.efficiency_score(), 1000);
    }

    #[test]
    fn test_efficiency_score_zero_envelopes() {
        let proof = BandwidthProof {
            gateway_id: [0u8; 32],
            window_start: 0,
            window_end: 100,
            envelope_count: 0,
            bytes_relayed: 0,
            source_diversity: 0,
            destination_diversity: 0,
            relay_merkle_root: [0u8; 32],
            signature: vec![0u8; 64],
        };
        assert_eq!(proof.efficiency_score(), 0);
    }
}
