use serde::{Deserialize, Serialize};

/// User identity in the CipherOcto network.
///
/// Aligns with RFC-0102 (Wallet Cryptography) and RFC-0850 (DOT) identity model.
/// The `public_key` is a 32-byte Ed25519 public key.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Identity {
    /// Unique identity identifier (hex-encoded or derived hash)
    pub id: String,
    /// Ed25519 public key (32 bytes, hex-encoded for serialization)
    pub public_key: [u8; 32],
}

impl Identity {
    /// Create a new identity with a placeholder public key.
    pub fn new(id: String) -> Self {
        Self {
            id,
            public_key: [0u8; 32], // Placeholder — must be set before use
        }
    }

    /// Create a new identity with an explicit public key.
    pub fn with_key(id: String, public_key: [u8; 32]) -> Self {
        Self { id, public_key }
    }

    /// Check if the identity has been initialized with a real key.
    pub fn has_key(&self) -> bool {
        self.public_key != [0u8; 32]
    }
}
