//! Overlay identity model (RFC-0853 §3)

use blake3;

/// Sovereign overlay identity — platform-independent cryptographic identity.
///
/// Identity MUST remain independent from Telegram accounts, Discord usernames,
/// Matrix IDs, IP addresses, DNS names, device identifiers.
///
/// `peer_id` = BLAKE3-256(public_key || identity_epoch || "ocrypt:identity:v1")
#[derive(Clone, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct OverlayIdentity {
    /// BLAKE3-256 derived identity hash
    pub peer_id: [u8; 32],
    /// Ed25519 public key (32 bytes)
    pub public_key: [u8; 32],
    /// Epoch when identity was created/rotated
    pub identity_epoch: u64,
    /// Merkle root of capabilities
    pub capabilities_root: [u8; 32],
    /// Ed25519 signature over the identity fields
    pub signature: [u8; 64],
}

impl OverlayIdentity {
    /// Domain separation string for identity derivation
    pub const IDENTITY_DOMAIN: &'static str = "ocrypt:identity:v1";

    /// Derive peer_id from public key and epoch.
    ///
    /// peer_id = BLAKE3-256(public_key || identity_epoch_be || "ocrypt:identity:v1")
    pub fn derive_peer_id(public_key: &[u8; 32], identity_epoch: u64) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(public_key);
        hasher.update(&identity_epoch.to_be_bytes());
        hasher.update(Self::IDENTITY_DOMAIN.as_bytes());
        *hasher.finalize().as_bytes()
    }

    /// Create a new overlay identity (unsigned).
    pub fn new(public_key: [u8; 32], identity_epoch: u64) -> Self {
        let peer_id = Self::derive_peer_id(&public_key, identity_epoch);
        Self {
            peer_id,
            public_key,
            identity_epoch,
            capabilities_root: [0u8; 32],
            signature: [0u8; 64],
        }
    }

    /// Set capabilities root.
    pub fn with_capabilities_root(mut self, root: [u8; 32]) -> Self {
        self.capabilities_root = root;
        self
    }

    /// Set signature.
    pub fn with_signature(mut self, signature: [u8; 64]) -> Self {
        self.signature = signature;
        self
    }

    /// Compute the signing bytes for this identity.
    /// = peer_id || public_key || identity_epoch_be || capabilities_root
    pub fn to_signing_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(32 + 32 + 8 + 32);
        bytes.extend_from_slice(&self.peer_id);
        bytes.extend_from_slice(&self.public_key);
        bytes.extend_from_slice(&self.identity_epoch.to_be_bytes());
        bytes.extend_from_slice(&self.capabilities_root);
        bytes
    }
}

/// Optional platform binding — links overlay identity to a platform account.
///
/// Bindings MUST NEVER become consensus authority.
#[derive(Clone, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct PlatformBinding {
    /// Platform type identifier (matches DOT PlatformType)
    pub platform_type: u16,
    /// BLAKE3-256 of external identifier
    pub external_identifier_hash: [u8; 32],
    /// Ed25519 signature proving ownership
    pub proof_signature: [u8; 64],
}

impl PlatformBinding {
    /// Create a new platform binding.
    pub fn new(platform_type: u16, external_identifier_hash: [u8; 32]) -> Self {
        Self {
            platform_type,
            external_identifier_hash,
            proof_signature: [0u8; 64],
        }
    }

    /// Compute signing bytes: platform_type_be || external_identifier_hash
    pub fn to_signing_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(2 + 32);
        bytes.extend_from_slice(&self.platform_type.to_be_bytes());
        bytes.extend_from_slice(&self.external_identifier_hash);
        bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_peer_id_deterministic() {
        let pk = [0x42u8; 32];
        let id1 = OverlayIdentity::derive_peer_id(&pk, 0);
        let id2 = OverlayIdentity::derive_peer_id(&pk, 0);
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_peer_id_different_keys() {
        let pk1 = [0x42u8; 32];
        let pk2 = [0x43u8; 32];
        let id1 = OverlayIdentity::derive_peer_id(&pk1, 0);
        let id2 = OverlayIdentity::derive_peer_id(&pk2, 0);
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_peer_id_different_epochs() {
        let pk = [0x42u8; 32];
        let id1 = OverlayIdentity::derive_peer_id(&pk, 0);
        let id2 = OverlayIdentity::derive_peer_id(&pk, 1);
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_identity_new() {
        let pk = [0x42u8; 32];
        let identity = OverlayIdentity::new(pk, 0);
        assert_eq!(identity.public_key, pk);
        assert_eq!(identity.identity_epoch, 0);
        let expected_id = OverlayIdentity::derive_peer_id(&pk, 0);
        assert_eq!(identity.peer_id, expected_id);
    }

    #[test]
    fn test_identity_builder() {
        let pk = [0x42u8; 32];
        let caps = [0x01u8; 32];
        let sig = [0xAAu8; 64];
        let identity = OverlayIdentity::new(pk, 0)
            .with_capabilities_root(caps)
            .with_signature(sig);
        assert_eq!(identity.capabilities_root, caps);
        assert_eq!(identity.signature, sig);
    }

    #[test]
    fn test_identity_signing_bytes() {
        let pk = [0x42u8; 32];
        let identity = OverlayIdentity::new(pk, 0);
        let bytes = identity.to_signing_bytes();
        // 32 + 32 + 8 + 32 = 104
        assert_eq!(bytes.len(), 104);
    }

    #[test]
    fn test_platform_binding() {
        let hash = [0x01u8; 32];
        let binding = PlatformBinding::new(0x0001, hash);
        assert_eq!(binding.platform_type, 0x0001);
        assert_eq!(binding.external_identifier_hash, hash);
        let bytes = binding.to_signing_bytes();
        assert_eq!(bytes.len(), 34); // 2 + 32
    }
}
