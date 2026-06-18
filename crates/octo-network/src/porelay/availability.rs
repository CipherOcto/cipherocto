//! Availability Proof (RFC-0860 §3.2)

use serde::{Deserialize, Serialize};

/// Availability Proof — proves gateway was online during a time window.
///
/// 7 fields per RFC-0860 §3.2.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[repr(C)]
pub struct AvailabilityProof {
    /// Gateway being attested
    pub gateway_id: [u8; 32],
    /// Time window start (epoch number)
    pub window_start: u64,
    /// Time window end (epoch number)
    pub window_end: u64,
    /// Number of heartbeats sent in this window
    pub heartbeat_count: u32,
    /// Merkle root of heartbeat hashes in this window
    pub heartbeat_root: [u8; 32],
    /// Number of distinct peers contacted
    pub peer_diversity: u16,
    /// Ed25519 signature over all above fields (canonical serialization)
    #[serde(with = "serde_bytes")]
    pub signature: [u8; 64],
}

/// Default heartbeat interval in seconds
pub const HEARTBEAT_INTERVAL: u64 = 30;

/// Availability threshold for "highly available" (basis points)
pub const HIGH_AVAILABILITY_THRESHOLD: u16 = 950;

impl AvailabilityProof {
    /// Compute availability score (basis points, 0-1000)
    /// availability_score = heartbeat_count * 1000 / expected_heartbeat_count
    pub fn availability_score(&self) -> u16 {
        let window_duration = self.window_end.saturating_sub(self.window_start);
        if window_duration == 0 {
            return 0;
        }
        let expected = window_duration / HEARTBEAT_INTERVAL;
        if expected == 0 {
            return 0;
        }
        let score = (self.heartbeat_count as u64).saturating_mul(1000) / expected;
        score.min(1000) as u16
    }

    /// Check if gateway is highly available (>= 950 basis points)
    pub fn is_highly_available(&self) -> bool {
        self.availability_score() >= HIGH_AVAILABILITY_THRESHOLD
    }

    /// Compute canonical signing bytes
    pub fn to_signing_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(32 + 8 + 8 + 4 + 32 + 2);
        buf.extend_from_slice(&self.gateway_id);
        buf.extend_from_slice(&self.window_start.to_be_bytes());
        buf.extend_from_slice(&self.window_end.to_be_bytes());
        buf.extend_from_slice(&self.heartbeat_count.to_be_bytes());
        buf.extend_from_slice(&self.heartbeat_root);
        buf.extend_from_slice(&self.peer_diversity.to_be_bytes());
        buf
    }

    /// Verify the Ed25519 signature.
    pub fn verify_signature(&self, public_key: &[u8; 32]) -> bool {
        use ed25519_dalek::{Signature, Verifier, VerifyingKey};
        let vk = match VerifyingKey::from_bytes(public_key) {
            Ok(vk) => vk,
            Err(_) => return false,
        };
        let sig = Signature::from_bytes(&self.signature);
        vk.verify(&self.to_signing_bytes(), &sig).is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_availability_score_full() {
        let proof = AvailabilityProof {
            gateway_id: [0u8; 32],
            window_start: 0,
            window_end: 3000, // 100 heartbeats expected
            heartbeat_count: 100,
            heartbeat_root: [0u8; 32],
            peer_diversity: 10,
            signature: [0u8; 64],
        };
        assert_eq!(proof.availability_score(), 1000);
        assert!(proof.is_highly_available());
    }

    #[test]
    fn test_availability_score_partial() {
        let proof = AvailabilityProof {
            gateway_id: [0u8; 32],
            window_start: 0,
            window_end: 3000,
            heartbeat_count: 90,
            heartbeat_root: [0u8; 32],
            peer_diversity: 5,
            signature: [0u8; 64],
        };
        assert_eq!(proof.availability_score(), 900);
        assert!(!proof.is_highly_available());
    }

    #[test]
    fn test_availability_score_zero_window() {
        let proof = AvailabilityProof {
            gateway_id: [0u8; 32],
            window_start: 100,
            window_end: 100,
            heartbeat_count: 0,
            heartbeat_root: [0u8; 32],
            peer_diversity: 0,
            signature: [0u8; 64],
        };
        assert_eq!(proof.availability_score(), 0);
    }

    #[test]
    fn test_signing_bytes_size() {
        let proof = AvailabilityProof {
            gateway_id: [0u8; 32],
            window_start: 0,
            window_end: 3000,
            heartbeat_count: 100,
            heartbeat_root: [0u8; 32],
            peer_diversity: 10,
            signature: [0u8; 64],
        };
        assert_eq!(proof.to_signing_bytes().len(), 86);
    }
}
