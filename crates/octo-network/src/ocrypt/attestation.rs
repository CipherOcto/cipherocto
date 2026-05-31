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

/// Propagation target for attestations and revocations.
///
/// Indicates the recommended network layer for propagating
/// attestation or revocation messages.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PropagationTarget {
    /// Propagate via Global Data Plane (data distribution).
    Gdp,
    /// Propagate via DGP gossip protocol (fast propagation).
    Dgp,
    /// Direct peer-to-peer propagation (point-to-point).
    Direct,
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

    /// Get the recommended propagation target for this attestation.
    ///
    /// Attestations should be propagated via GDP for reliable data distribution.
    pub fn propagation_hint(&self) -> PropagationTarget {
        PropagationTarget::Gdp
    }

    /// Verify that the attestation signature is valid for the given public key.
    ///
    /// Returns Ok(()) if valid, Err otherwise.
    pub fn verify_signature(
        &self,
        public_key: &[u8; 32],
    ) -> Result<(), crate::ocrypt::error::CryptoError> {
        use ed25519_dalek::{Signature, Verifier, VerifyingKey};
        let verifying_key = VerifyingKey::from_bytes(public_key)
            .map_err(|_| crate::ocrypt::error::CryptoError::InvalidSignature)?;
        let sig = Signature::from_bytes(&self.signature);
        verifying_key
            .verify(&self.to_signing_bytes(), &sig)
            .map_err(|_| crate::ocrypt::error::CryptoError::InvalidSignature)
    }
}

/// Attestation type constants
pub mod attestation_type {
    /// Capability attestation
    pub const CAPABILITY: u16 = 0x0001;
    /// Uptime attestation
    pub const UPTIME: u16 = 0x0002;
    /// Stake attestation
    pub const STAKE: u16 = 0x0003;
    /// Bandwidth attestation
    pub const BANDWIDTH: u16 = 0x0004;
}

/// Key rotation record — links old key to new key with backward compatibility.
///
/// Rotation protocol:
/// 1. Generate new keypair
/// 2. Sign `(old_public_key || new_public_key || rotation_epoch)` with OLD private key
/// 3. Sign `(old_public_key || new_public_key || rotation_epoch)` with NEW private key
/// 4. Publish rotation record
/// 5. Old key remains valid for grace period
#[derive(Clone, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct KeyRotation {
    /// Old public key being rotated out
    pub old_public_key: [u8; 32],
    /// New public key being rotated in
    pub new_public_key: [u8; 32],
    /// Epoch at which rotation takes effect
    pub rotation_epoch: u64,
    /// Signature by old key over (old_pk || new_pk || rotation_epoch)
    pub signature_by_old: [u8; 64],
    /// Signature by new key over (old_pk || new_pk || rotation_epoch)
    pub signature_by_new: [u8; 64],
}

impl KeyRotation {
    /// Create a new key rotation record (signatures must be set separately).
    pub fn new(old_public_key: [u8; 32], new_public_key: [u8; 32], rotation_epoch: u64) -> Self {
        Self {
            old_public_key,
            new_public_key,
            rotation_epoch,
            signature_by_old: [0u8; 64],
            signature_by_new: [0u8; 64],
        }
    }

    /// Compute signing bytes: old_public_key || new_public_key || rotation_epoch_be
    pub fn to_signing_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(32 + 32 + 8);
        bytes.extend_from_slice(&self.old_public_key);
        bytes.extend_from_slice(&self.new_public_key);
        bytes.extend_from_slice(&self.rotation_epoch.to_be_bytes());
        bytes
    }

    /// Verify both signatures (old key signs, new key signs).
    pub fn verify(&self) -> Result<(), crate::ocrypt::error::CryptoError> {
        use ed25519_dalek::{Signature, Verifier, VerifyingKey};
        let signing_bytes = self.to_signing_bytes();

        let old_key = VerifyingKey::from_bytes(&self.old_public_key)
            .map_err(|_| crate::ocrypt::error::CryptoError::InvalidSignature)?;
        let old_sig = Signature::from_bytes(&self.signature_by_old);
        old_key
            .verify(&signing_bytes, &old_sig)
            .map_err(|_| crate::ocrypt::error::CryptoError::InvalidSignature)?;

        let new_key = VerifyingKey::from_bytes(&self.new_public_key)
            .map_err(|_| crate::ocrypt::error::CryptoError::InvalidSignature)?;
        let new_sig = Signature::from_bytes(&self.signature_by_new);
        new_key
            .verify(&signing_bytes, &new_sig)
            .map_err(|_| crate::ocrypt::error::CryptoError::InvalidSignature)?;

        Ok(())
    }
}

/// Key revocation notice — published when a key is compromised.
///
/// Revocation protocol (RFC-0853 §12):
/// 1. Publish signed revocation: (compromised_key_id, revocation_epoch, successor_key_id, signature_by_successor)
/// 2. Grace period: 24 hours for peers to observe via GDP
/// 3. After grace period: all signatures by compromised key rejected
/// 4. Retroactive: signatures BEFORE revocation epoch remain valid
#[derive(Clone, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct RevocationNotice {
    /// ID of the compromised key
    pub compromised_key_id: [u8; 32],
    /// Epoch at which revocation takes effect
    pub revocation_epoch: u64,
    /// ID of the successor key (if any)
    pub successor_key_id: [u8; 32],
    /// Signature by successor key over the notice
    pub signature: [u8; 64],
}

impl RevocationNotice {
    /// Create a new revocation notice.
    pub fn new(
        compromised_key_id: [u8; 32],
        revocation_epoch: u64,
        successor_key_id: [u8; 32],
    ) -> Self {
        Self {
            compromised_key_id,
            revocation_epoch,
            successor_key_id,
            signature: [0u8; 64],
        }
    }

    /// Compute signing bytes: compromised_key_id || revocation_epoch_be || successor_key_id
    pub fn to_signing_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(32 + 8 + 32);
        bytes.extend_from_slice(&self.compromised_key_id);
        bytes.extend_from_slice(&self.revocation_epoch.to_be_bytes());
        bytes.extend_from_slice(&self.successor_key_id);
        bytes
    }

    /// Verify the revocation signature by the successor key.
    pub fn verify_signature(
        &self,
        successor_public_key: &[u8; 32],
    ) -> Result<(), crate::ocrypt::error::CryptoError> {
        use ed25519_dalek::{Signature, Verifier, VerifyingKey};
        let verifying_key = VerifyingKey::from_bytes(successor_public_key)
            .map_err(|_| crate::ocrypt::error::CryptoError::InvalidSignature)?;
        let sig = Signature::from_bytes(&self.signature);
        verifying_key
            .verify(&self.to_signing_bytes(), &sig)
            .map_err(|_| crate::ocrypt::error::CryptoError::InvalidSignature)
    }

    /// Get the recommended propagation target for this revocation.
    ///
    /// Revocations should always use DGP gossip for fast propagation across the network.
    pub fn propagation_hint(&self) -> PropagationTarget {
        PropagationTarget::Dgp
    }

    /// Check if a given epoch is within the grace period after revocation.
    pub fn is_in_grace_period(&self, current_epoch: u64, grace_period_epochs: u64) -> bool {
        current_epoch >= self.revocation_epoch
            && current_epoch < self.revocation_epoch.saturating_add(grace_period_epochs)
    }

    /// Check if a given epoch is after the grace period (key fully rejected).
    pub fn is_revoked(&self, current_epoch: u64, grace_period_epochs: u64) -> bool {
        current_epoch >= self.revocation_epoch.saturating_add(grace_period_epochs)
    }
}

/// Default grace period for key revocation: 24 hours in seconds.
pub const DEFAULT_REVOCATION_GRACE_PERIOD: u64 = 86400;

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

    #[test]
    fn test_key_rotation_signing_bytes_deterministic() {
        let rot = KeyRotation::new([0x01u8; 32], [0x02u8; 32], 500);
        let b1 = rot.to_signing_bytes();
        let b2 = rot.to_signing_bytes();
        assert_eq!(b1, b2);
    }

    #[test]
    fn test_key_rotation_signing_bytes_size() {
        let rot = KeyRotation::new([0x01u8; 32], [0x02u8; 32], 500);
        assert_eq!(rot.to_signing_bytes().len(), 72); // 32 + 32 + 8
    }

    #[test]
    fn test_key_rotation_with_signatures() {
        use ed25519_dalek::{Signer, SigningKey};
        let old_seed = [0x01u8; 32];
        let new_seed = [0x02u8; 32];
        let old_key = SigningKey::from_bytes(&old_seed);
        let new_key = SigningKey::from_bytes(&new_seed);

        let mut rot = KeyRotation::new(
            old_key.verifying_key().to_bytes(),
            new_key.verifying_key().to_bytes(),
            500,
        );
        let signing_bytes = rot.to_signing_bytes();
        rot.signature_by_old = old_key.sign(&signing_bytes).to_bytes();
        rot.signature_by_new = new_key.sign(&signing_bytes).to_bytes();

        assert!(rot.verify().is_ok());
    }

    #[test]
    fn test_key_rotation_verify_bad_old_signature() {
        use ed25519_dalek::{Signer, SigningKey};
        let old_key = SigningKey::from_bytes(&[0x01u8; 32]);
        let new_key = SigningKey::from_bytes(&[0x02u8; 32]);
        let wrong_key = SigningKey::from_bytes(&[0x03u8; 32]);

        let mut rot = KeyRotation::new(
            old_key.verifying_key().to_bytes(),
            new_key.verifying_key().to_bytes(),
            500,
        );
        let signing_bytes = rot.to_signing_bytes();
        rot.signature_by_old = wrong_key.sign(&signing_bytes).to_bytes(); // wrong key!
        rot.signature_by_new = new_key.sign(&signing_bytes).to_bytes();

        assert!(rot.verify().is_err());
    }

    #[test]
    fn test_revocation_notice_signing_bytes_deterministic() {
        let rev = RevocationNotice::new([0x01u8; 32], 1000, [0x02u8; 32]);
        let b1 = rev.to_signing_bytes();
        let b2 = rev.to_signing_bytes();
        assert_eq!(b1, b2);
    }

    #[test]
    fn test_revocation_notice_signing_bytes_size() {
        let rev = RevocationNotice::new([0x01u8; 32], 1000, [0x02u8; 32]);
        assert_eq!(rev.to_signing_bytes().len(), 72); // 32 + 8 + 32
    }

    #[test]
    fn test_revocation_grace_period() {
        let rev = RevocationNotice::new([0x01u8; 32], 1000, [0x02u8; 32]);
        // At epoch 500: before revocation
        assert!(!rev.is_in_grace_period(500, 100));
        assert!(!rev.is_revoked(500, 100));

        // At epoch 1000: revocation starts, in grace period
        assert!(rev.is_in_grace_period(1000, 100));
        assert!(!rev.is_revoked(1000, 100));

        // At epoch 1050: still in grace period
        assert!(rev.is_in_grace_period(1050, 100));
        assert!(!rev.is_revoked(1050, 100));

        // At epoch 1100: grace period ended, fully revoked
        assert!(!rev.is_in_grace_period(1100, 100));
        assert!(rev.is_revoked(1100, 100));
    }

    #[test]
    fn test_revocation_notice_with_signature() {
        use ed25519_dalek::{Signer, SigningKey};
        let successor_key = SigningKey::from_bytes(&[0x02u8; 32]);
        let mut rev =
            RevocationNotice::new([0x01u8; 32], 1000, successor_key.verifying_key().to_bytes());
        let signing_bytes = rev.to_signing_bytes();
        rev.signature = successor_key.sign(&signing_bytes).to_bytes();
        assert!(rev
            .verify_signature(&successor_key.verifying_key().to_bytes())
            .is_ok());
    }

    #[test]
    fn test_revocation_notice_bad_signature() {
        use ed25519_dalek::{Signer, SigningKey};
        let successor_key = SigningKey::from_bytes(&[0x02u8; 32]);
        let wrong_key = SigningKey::from_bytes(&[0x03u8; 32]);
        let mut rev =
            RevocationNotice::new([0x01u8; 32], 1000, successor_key.verifying_key().to_bytes());
        let signing_bytes = rev.to_signing_bytes();
        rev.signature = wrong_key.sign(&signing_bytes).to_bytes(); // wrong key!
        assert!(rev
            .verify_signature(&successor_key.verifying_key().to_bytes())
            .is_err());
    }

    #[test]
    fn test_attestation_type_constants() {
        assert_eq!(attestation_type::CAPABILITY, 0x0001);
        assert_eq!(attestation_type::UPTIME, 0x0002);
        assert_eq!(attestation_type::STAKE, 0x0003);
        assert_eq!(attestation_type::BANDWIDTH, 0x0004);
    }

    #[test]
    fn test_default_revocation_grace_period() {
        assert_eq!(DEFAULT_REVOCATION_GRACE_PERIOD, 86400); // 24 hours in seconds
    }

    #[test]
    fn test_propagation_target_variants() {
        // Verify enum variants exist and are distinct
        assert_ne!(PropagationTarget::Gdp, PropagationTarget::Dgp);
        assert_ne!(PropagationTarget::Dgp, PropagationTarget::Direct);
        assert_ne!(PropagationTarget::Gdp, PropagationTarget::Direct);
    }

    #[test]
    fn test_attestation_propagation_hint() {
        let att = GatewayAttestation::new([0x42u8; 32], 0x0001, [0x01u8; 32], 1000);
        assert_eq!(att.propagation_hint(), PropagationTarget::Gdp);
    }

    #[test]
    fn test_revocation_propagation_hint() {
        let rev = RevocationNotice::new([0x01u8; 32], 1000, [0x02u8; 32]);
        assert_eq!(rev.propagation_hint(), PropagationTarget::Dgp);
    }
}
