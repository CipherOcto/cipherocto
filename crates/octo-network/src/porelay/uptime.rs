//! Uptime Proof (RFC-0860 §3.4)

use serde::{Deserialize, Serialize};

mod serde_signature {
    use serde::{Deserializer, Serializer};

    pub fn serialize<S: Serializer>(sig: &[u8; 64], s: S) -> Result<S::Ok, S::Error> {
        serde_bytes::serialize(sig.as_ref(), s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<[u8; 64], D::Error> {
        let v: Vec<u8> = serde_bytes::deserialize(d)?;
        if v.len() != 64 {
            return Err(serde::de::Error::invalid_length(v.len(), &"64 bytes"));
        }
        let mut arr = [0u8; 64];
        arr.copy_from_slice(&v);
        Ok(arr)
    }
}

/// Uptime Proof — proves continuous gateway operation over an extended period.
///
/// 7 fields per RFC-0860 §3.4.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UptimeProof {
    /// Gateway being attested
    pub gateway_id: [u8; 32],
    /// Start of uptime period (epoch)
    pub start_epoch: u64,
    /// Current epoch (end of attested period)
    pub current_epoch: u64,
    /// Number of windows with availability_score >= 950
    pub compliant_windows: u32,
    /// Total number of windows in period
    pub total_windows: u32,
    /// Merkle root of AvailabilityProof commitments
    pub availability_root: [u8; 32],
    /// Ed25519 signature
    #[serde(with = "serde_signature")]
    pub signature: [u8; 64],
}

impl UptimeProof {
    /// Compute uptime score (basis points, 0-1000)
    /// uptime_score = compliant_windows * 1000 / total_windows
    pub fn uptime_score(&self) -> u16 {
        if self.total_windows == 0 {
            return 0;
        }
        let score =
            (self.compliant_windows as u64).saturating_mul(1000) / (self.total_windows as u64);
        score.min(1000) as u16
    }

    /// Compute canonical signing bytes
    pub fn to_signing_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(32 + 8 + 8 + 4 + 4 + 32);
        buf.extend_from_slice(&self.gateway_id);
        buf.extend_from_slice(&self.start_epoch.to_be_bytes());
        buf.extend_from_slice(&self.current_epoch.to_be_bytes());
        buf.extend_from_slice(&self.compliant_windows.to_be_bytes());
        buf.extend_from_slice(&self.total_windows.to_be_bytes());
        buf.extend_from_slice(&self.availability_root);
        buf
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uptime_score_perfect() {
        let proof = UptimeProof {
            gateway_id: [0u8; 32],
            start_epoch: 0,
            current_epoch: 100,
            compliant_windows: 100,
            total_windows: 100,
            availability_root: [0u8; 32],
            signature: [0u8; 64],
        };
        assert_eq!(proof.uptime_score(), 1000);
    }

    #[test]
    fn test_uptime_score_partial() {
        let proof = UptimeProof {
            gateway_id: [0u8; 32],
            start_epoch: 0,
            current_epoch: 100,
            compliant_windows: 95,
            total_windows: 100,
            availability_root: [0u8; 32],
            signature: [0u8; 64],
        };
        assert_eq!(proof.uptime_score(), 950);
    }

    #[test]
    fn test_uptime_score_zero_windows() {
        let proof = UptimeProof {
            gateway_id: [0u8; 32],
            start_epoch: 0,
            current_epoch: 0,
            compliant_windows: 0,
            total_windows: 0,
            availability_root: [0u8; 32],
            signature: [0u8; 64],
        };
        assert_eq!(proof.uptime_score(), 0);
    }

    #[test]
    fn test_signing_bytes_size() {
        let proof = UptimeProof {
            gateway_id: [0u8; 32],
            start_epoch: 0,
            current_epoch: 100,
            compliant_windows: 95,
            total_windows: 100,
            availability_root: [0u8; 32],
            signature: [0u8; 64],
        };
        assert_eq!(proof.to_signing_bytes().len(), 88);
    }
}
