//! CryptoSuiteId — algorithm agility (RFC-0853 §1.1)

/// Identifies the set of cryptographic algorithms in use.
///
/// Each field is a u16 ID referencing a specific algorithm.
/// This enables algorithm migration without protocol changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(C)]
pub struct CryptoSuiteId {
    /// Hash algorithm (e.g., BLAKE3-256 = 0x0001)
    pub hash_id: u16,
    /// Signature algorithm (e.g., Ed25519 = 0x0001)
    pub signature_id: u16,
    /// Key exchange algorithm (e.g., X25519 = 0x0001)
    pub kex_id: u16,
    /// AEAD algorithm (e.g., ChaCha20-Poly1305 = 0x0001)
    pub aead_id: u16,
    /// KDF algorithm (e.g., HKDF-BLAKE3 = 0x0001)
    pub kdf_id: u16,
}

/// Standard algorithm IDs
pub mod algorithms {
    /// BLAKE3-256 hash
    pub const HASH_BLAKE3_256: u16 = 0x0001;
    /// SHA-256 hash (compatibility)
    pub const HASH_SHA256: u16 = 0x0002;

    /// Ed25519 signatures
    pub const SIG_ED25519: u16 = 0x0001;

    /// X25519 key exchange
    pub const KEX_X25519: u16 = 0x0001;

    /// ChaCha20-Poly1305 AEAD
    pub const AEAD_CHACHA20_POLY1305: u16 = 0x0001;

    /// HKDF-BLAKE3 KDF
    pub const KDF_HKDF_BLAKE3: u16 = 0x0001;
}

/// Default crypto suite using recommended algorithms
pub const DEFAULT_SUITE: CryptoSuiteId = CryptoSuiteId {
    hash_id: algorithms::HASH_BLAKE3_256,
    signature_id: algorithms::SIG_ED25519,
    kex_id: algorithms::KEX_X25519,
    aead_id: algorithms::AEAD_CHACHA20_POLY1305,
    kdf_id: algorithms::KDF_HKDF_BLAKE3,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_suite() {
        let suite = DEFAULT_SUITE;
        assert_eq!(suite.hash_id, 0x0001);
        assert_eq!(suite.signature_id, 0x0001);
        assert_eq!(suite.kex_id, 0x0001);
        assert_eq!(suite.aead_id, 0x0001);
        assert_eq!(suite.kdf_id, 0x0001);
    }

    #[test]
    fn test_suite_equality() {
        let s1 = DEFAULT_SUITE;
        let s2 = DEFAULT_SUITE;
        assert_eq!(s1, s2);
    }

    #[test]
    fn test_suite_copy() {
        let s1 = DEFAULT_SUITE;
        let s2 = s1;
        assert_eq!(s1, s2);
    }
}
