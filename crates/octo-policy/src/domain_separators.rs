//! Canonical `octo/` BLAKE3 domain separators (RFC-0967-A1 v1.9.2 + RFC-0008 §Domain Separators).
//!
//! Layer A substrate — frozen; semver-major only.
//!
//! All hash derivations in the policy crate MUST go through `blake3_prefix`
//! to ensure the `octo/` canonical prefix per CLAUDE.md §Architectural
//! Principles (no parallel abstractions). AUDIT_VARIANT_HASH_DOMAIN was
//! previously `cipherocto/audit/v1/`; per F-R12-DSEP-CIPHEROCTO-TO-OCTO
//! the canonical prefix is `octo/`.

use blake3::Hasher;

/// Canonical `octo/` prefix for all BLAKE3-derived hashes (RFC-0008 §Domain Separators).
pub const OCTO_PREFIX: &str = "octo/";

/// AUDIT_VARIANT_HASH_DOMAIN — variant assignment per chain_id.
///
/// Migration: previously `cipherocto/audit/v1/`; canonical per
/// F-R12-DSEP-CIPHEROCTO-TO-OCTO = `octo/audit/ab/v1/`.
pub const AUDIT_VARIANT_HASH_DOMAIN: &str = "octo/audit/ab/v1/";

/// Policy hash domain prefix — used for `policy_hash = BLAKE3(prefix || body)[..32]`.
pub const POLICY_HASH_DOMAIN: &str = "octo/policy/hash/v1/";

/// InteropSelector domain prefix.
pub const INTEROP_SELECTOR_DOMAIN: &str = "octo/interop/selector/v1/";

/// Vault creation domain prefix.
pub const VAULT_CREATION_DOMAIN: &str = "octo/vault/creation/v1/";

/// Subject provision domain prefix.
pub const SUBJECT_PROVISION_DOMAIN: &str = "octo/subject/provision/v1/";

/// User info read domain prefix.
pub const USER_INFO_READ_DOMAIN: &str = "octo/user/info/read/v1/";

/// User update domain prefix.
pub const USER_UPDATE_DOMAIN: &str = "octo/user/update/v1/";

/// Settlement envelope domain prefix.
pub const SETTLEMENT_ENVELOPE_DOMAIN: &str = "octo/settlement/envelope/v1/";

/// Domain-prefixed BLAKE3 hashers (RFC-0008 §Domain Separators).
pub mod blake3_prefix {
    #[allow(clippy::wildcard_imports)]
    use super::*;

    /// Append `chain_id_bytes` to the canonical `octo/audit/ab/v1/` prefix and hash.
    ///
    /// Returns the first 8 bytes as u64 (mod `variant_cardinality`) for variant assignment.
    pub fn derive_audit_variant(chain_id: &[u8; 32], variant_cardinality: u32) -> u64 {
        debug_assert!(variant_cardinality >= 2, "variant_cardinality must be >= 2");
        let mut hasher = Hasher::new();
        hasher.update(AUDIT_VARIANT_HASH_DOMAIN.as_bytes());
        hasher.update(chain_id.as_slice());
        let hash = hasher.finalize();
        let bytes = hash.as_bytes();
        let n = u64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]);
        n % variant_cardinality as u64
    }

    /// `policy_hash = BLAKE3(POLICY_HASH_DOMAIN || body)[..32]`.
    pub fn derive_policy_hash(body: &[u8]) -> [u8; 32] {
        let mut hasher = Hasher::new();
        hasher.update(POLICY_HASH_DOMAIN.as_bytes());
        hasher.update(body);
        let hash = hasher.finalize();
        let bytes = hash.as_bytes();
        let mut out = [0u8; 32];
        out.copy_from_slice(&bytes[..32]);
        out
    }

    /// `interop_selector_hash = BLAKE3(INTEROP_SELECTOR_DOMAIN || body)[..32]`.
    pub fn derive_interop_selector_hash(body: &[u8]) -> [u8; 32] {
        let mut hasher = Hasher::new();
        hasher.update(INTEROP_SELECTOR_DOMAIN.as_bytes());
        hasher.update(body);
        let hash = hasher.finalize();
        let bytes = hash.as_bytes();
        let mut out = [0u8; 32];
        out.copy_from_slice(&bytes[..32]);
        out
    }

    /// `vault_creation_hash = BLAKE3(VAULT_CREATION_DOMAIN || req_cbor)[..32]`.
    pub fn derive_vault_creation_hash(body: &[u8]) -> [u8; 32] {
        let mut hasher = Hasher::new();
        hasher.update(VAULT_CREATION_DOMAIN.as_bytes());
        hasher.update(body);
        let hash = hasher.finalize();
        let bytes = hash.as_bytes();
        let mut out = [0u8; 32];
        out.copy_from_slice(&bytes[..32]);
        out
    }

    /// `subject_provision_hash = BLAKE3(SUBJECT_PROVISION_DOMAIN || body)[..32]`.
    pub fn derive_subject_provision_hash(body: &[u8]) -> [u8; 32] {
        let mut hasher = Hasher::new();
        hasher.update(SUBJECT_PROVISION_DOMAIN.as_bytes());
        hasher.update(body);
        let hash = hasher.finalize();
        let bytes = hash.as_bytes();
        let mut out = [0u8; 32];
        out.copy_from_slice(&bytes[..32]);
        out
    }

    /// `user_info_read_hash = BLAKE3(USER_INFO_READ_DOMAIN || body)[..32]`.
    pub fn derive_user_info_read_hash(body: &[u8]) -> [u8; 32] {
        let mut hasher = Hasher::new();
        hasher.update(USER_INFO_READ_DOMAIN.as_bytes());
        hasher.update(body);
        let hash = hasher.finalize();
        let bytes = hash.as_bytes();
        let mut out = [0u8; 32];
        out.copy_from_slice(&bytes[..32]);
        out
    }

    /// `user_update_hash = BLAKE3(USER_UPDATE_DOMAIN || body)[..32]`.
    pub fn derive_user_update_hash(body: &[u8]) -> [u8; 32] {
        let mut hasher = Hasher::new();
        hasher.update(USER_UPDATE_DOMAIN.as_bytes());
        hasher.update(body);
        let hash = hasher.finalize();
        let bytes = hash.as_bytes();
        let mut out = [0u8; 32];
        out.copy_from_slice(&bytes[..32]);
        out
    }

    /// `settlement_envelope_hash = BLAKE3(SETTLEMENT_ENVELOPE_DOMAIN || body)[..32]`.
    pub fn derive_settlement_envelope_hash(body: &[u8]) -> [u8; 32] {
        let mut hasher = Hasher::new();
        hasher.update(SETTLEMENT_ENVELOPE_DOMAIN.as_bytes());
        hasher.update(body);
        let hash = hasher.finalize();
        let bytes = hash.as_bytes();
        let mut out = [0u8; 32];
        out.copy_from_slice(&bytes[..32]);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audit_variant_hash_domain_uses_octo_prefix() {
        assert!(
            AUDIT_VARIANT_HASH_DOMAIN.starts_with(OCTO_PREFIX),
            "AUDIT_VARIANT_HASH_DOMAIN must use canonical octo/ prefix"
        );
        assert!(!AUDIT_VARIANT_HASH_DOMAIN.contains("cipherocto/"));
    }

    #[test]
    fn all_domains_use_octo_prefix() {
        for d in [
            AUDIT_VARIANT_HASH_DOMAIN,
            POLICY_HASH_DOMAIN,
            INTEROP_SELECTOR_DOMAIN,
            VAULT_CREATION_DOMAIN,
            SUBJECT_PROVISION_DOMAIN,
            USER_INFO_READ_DOMAIN,
            USER_UPDATE_DOMAIN,
            SETTLEMENT_ENVELOPE_DOMAIN,
        ] {
            assert!(
                d.starts_with(OCTO_PREFIX),
                "domain {d} must use octo/ prefix"
            );
        }
    }

    #[test]
    fn audit_variant_derivation_is_deterministic() {
        let chain_id = [0x42_u8; 32];
        let v1 = blake3_prefix::derive_audit_variant(&chain_id, 2);
        let v2 = blake3_prefix::derive_audit_variant(&chain_id, 2);
        assert_eq!(v1, v2);
    }

    #[test]
    fn policy_hash_is_32_bytes() {
        let h = blake3_prefix::derive_policy_hash(b"some body");
        assert_eq!(h.len(), 32);
    }

    #[test]
    fn all_hash_functions_return_32_bytes() {
        for h in [
            blake3_prefix::derive_interop_selector_hash(b"x"),
            blake3_prefix::derive_vault_creation_hash(b"x"),
            blake3_prefix::derive_subject_provision_hash(b"x"),
            blake3_prefix::derive_user_info_read_hash(b"x"),
            blake3_prefix::derive_user_update_hash(b"x"),
            blake3_prefix::derive_settlement_envelope_hash(b"x"),
        ] {
            assert_eq!(h.len(), 32);
        }
    }
}
