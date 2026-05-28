//! Gateway attestation (RFC-0853 §9)

use blake3;

/// Gateway attestation — proves gateway capabilities at a point in time.
#[derive(Clone, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct GatewayAttestation {
    /// Gateway identifier
    pub gateway_id: [u8; 32],
    /// Type of attestation (e.g., 0x0001 = capability, 0x0002 = uptime)
    pub attestation_type: u16,
    /// Merkle root of attestation payload
    pub payload_root: [u8; 32],
    /// Timestamp of attestation
    pub timestamp: u64,
    /// Ed25519 signature over the attestation
    pub signature: [u8; 64],
}

impl GatewayAttestation {
    /// Create a new unsigned attestation.
    pub fn new(
        gateway_id: [u8; 32],
        attestation_type: u16,
        payload_root: [u8; 32],
        timestamp: u64,
    ) -> Self {
        Self {
            gateway_id,
            attestation_type,
            payload_root,
            timestamp,
            signature: [0u8; 64],
        }
    }

    /// Set signature.
    pub fn with_signature(mut self, signature: [u8; 64]) -> Self {
        self.signature = signature;
        self
    }

    /// Compute signing bytes:
    /// gateway_id || attestation_type_be || payload_root || timestamp_be
    pub fn to_signing_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(32 + 2 + 32 + 8);
        bytes.extend_from_slice(&self.gateway_id);
        bytes.extend_from_slice(&self.attestation_type.to_be_bytes());
        bytes.extend_from_slice(&self.payload_root);
        bytes.extend_from_slice(&self.timestamp.to_be_bytes());
        bytes
    }

    /// Derive attestation hash = BLAKE3-256(signing_bytes).
    pub fn attestation_hash(&self) -> [u8; 32] {
        *blake3::hash(&self.to_signing_bytes()).as_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_attestation_new() {
        let gw_id = [0x42u8; 32];
        let payload = [0x01u8; 32];
        let att = GatewayAttestation::new(gw_id, 0x0001, payload, 1000);
        assert_eq!(att.gateway_id, gw_id);
        assert_eq!(att.attestation_type, 0x0001);
        assert_eq!(att.payload_root, payload);
        assert_eq!(att.timestamp, 1000);
    }

    #[test]
    fn test_attestation_signing_bytes_deterministic() {
        let att = GatewayAttestation::new([0x42u8; 32], 0x0001, [0x01u8; 32], 1000);
        let b1 = att.to_signing_bytes();
        let b2 = att.to_signing_bytes();
        assert_eq!(b1, b2);
    }

    #[test]
    fn test_attestation_signing_bytes_size() {
        let att = GatewayAttestation::new([0x42u8; 32], 0x0001, [0x01u8; 32], 1000);
        let bytes = att.to_signing_bytes();
        assert_eq!(bytes.len(), 74); // 32 + 2 + 32 + 8
    }

    #[test]
    fn test_attestation_hash_deterministic() {
        let att = GatewayAttestation::new([0x42u8; 32], 0x0001, [0x01u8; 32], 1000);
        let h1 = att.attestation_hash();
        let h2 = att.attestation_hash();
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_attestation_hash_different_timestamps() {
        let a1 = GatewayAttestation::new([0x42u8; 32], 0x0001, [0x01u8; 32], 1000);
        let a2 = GatewayAttestation::new([0x42u8; 32], 0x0001, [0x01u8; 32], 1001);
        assert_ne!(a1.attestation_hash(), a2.attestation_hash());
    }

    #[test]
    fn test_attestation_builder() {
        let sig = [0xAAu8; 64];
        let att =
            GatewayAttestation::new([0x42u8; 32], 0x0001, [0x01u8; 32], 1000).with_signature(sig);
        assert_eq!(att.signature, sig);
    }

    #[test]
    fn test_attestation_hash_size() {
        let att = GatewayAttestation::new([0x42u8; 32], 0x0001, [0x01u8; 32], 1000);
        assert_eq!(att.attestation_hash().len(), 32);
    }
}
