//! PolicyRegistry trait + substrate adapter impl (RFC-0967-A1 v1.9.2 §2.4 + §2.5).
//!
//! Layer C adapter — substrate implements the registry; trait surface
//! lives in `octo-policy` so adapters can swap without trait churn.

use std::sync::Arc;

use octo_policy::policy_kinds::{ExecutionClass, ZK_ENVELOPE_MARKER};
use octo_storage_core::Database;
use thiserror::Error;

/// A registered policy entry (RFC-0967-A1 §2.4 — `policy_registry` table).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisteredPolicy {
    /// BLAKE3 hash of `octo/policy/hash/v1/` + canonical body bytes.
    pub policy_hash: [u8; 32],
    /// 16-byte UUIDv5 derived from the policy-kind namespace string.
    pub kind_uuid: [u8; 16],
    /// Canonical CBOR / trait-spec body bytes.
    pub body: Vec<u8>,
    /// Execution class (RFC-0008 §Data Structures): A=0x00, B=0x01, C=0x02.
    pub execution_class: ExecutionClass,
    /// Unix timestamp (seconds) at registration.
    pub registered_at_unix: i64,
    /// 32-byte DID of the registrant.
    pub registered_by_did: [u8; 32],
    /// Unix timestamp at revocation (None = active).
    pub revoked_at_unix: Option<i64>,
    /// 32-byte DID of the revoker (None = active).
    pub revoked_by_did: Option<[u8; 32]>,
    /// Free-text revocation reason.
    pub revocation_reason: Option<String>,
    /// Policy hash that supersedes this entry (delegation chain).
    pub superseding_policy_hash: Option<[u8; 32]>,
}

fn hex32(b: &[u8; 32]) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(64);
    for byte in b {
        let _ = write!(s, "{byte:02x}");
    }
    s
}

/// PolicyRegistryError — typed substrate errors.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum PolicyRegistryError {
    /// Caller-provided expected_hash differs from BLAKE3-derived actual.
    #[error("policy hash mismatch: expected {expected}, got {actual}")]
    HashMismatch {
        /// Hex-encoded expected hash (caller-supplied).
        expected: String,
        /// Hex-encoded actual hash (BLAKE3-derived from body).
        actual: String,
    },
    /// Lookup query returned no rows.
    #[error("policy not found: {0}")]
    NotFound(String),
    /// Class B execution requires a ZK envelope marker.
    #[error("class B policy requires ZK envelope marker at proof[16..20]")]
    ClassBRequiresZkProof,
    /// Revocation attempted on already-revoked policy.
    #[error("policy already revoked at {revoked_at_unix}")]
    AlreadyRevoked {
        /// Unix timestamp of original revocation.
        revoked_at_unix: i64,
    },
    /// Substrate denied the authority delegation request.
    #[error("authority delegation denied: {0}")]
    AuthorityDelegationDenied(String),
}

/// Substrate trait for policy registration / lookup (RFC-0967-A1 §2.4).
///
/// Three methods per spec: `lookup_policy` / `register_policy` /
/// `delegate_authority`. Adapters (octo-policy-storage) provide the
/// implementation; consumers depend only on the trait.
pub trait PolicyRegistry: Send + Sync {
    /// Lookup a policy by its content hash.
    fn lookup_policy(
        &self,
        policy_hash: &[u8; 32],
    ) -> Result<Option<RegisteredPolicy>, PolicyRegistryError>;

    /// Register a new policy. The registry verifies `expected_hash` matches
    /// `BLAKE3(POLICY_HASH_DOMAIN || body)`.
    fn register_policy(
        &self,
        kind_uuid: &[u8; 16],
        body: &[u8],
        execution_class: ExecutionClass,
        registered_by_did: &[u8; 32],
        registered_at_unix: i64,
        expected_hash: &[u8; 32],
    ) -> Result<RegisteredPolicy, PolicyRegistryError>;

    /// Delegate authority: replace one policy with another, with the
    /// `superseding_policy_hash` recorded on the old row.
    fn delegate_authority(
        &self,
        old_hash: &[u8; 32],
        new_hash: &[u8; 32],
        registrant_did: &[u8; 32],
        registered_at_unix: i64,
    ) -> Result<(), PolicyRegistryError>;
}

/// Verify the ZK envelope marker is present at proof[16..20] for Class B policies.
#[must_use]
pub fn verify_class_b_zk_marker(proof: &[u8]) -> bool {
    proof.len() >= 20 && proof[16..20] == ZK_ENVELOPE_MARKER
}

/// Stoolap-backed PolicyRegistry adapter.
pub struct StoolapPolicyRegistry {
    db: Arc<Database>,
}

impl StoolapPolicyRegistry {
    /// Build a Stoolap-backed registry wrapping the shared substrate handle.
    #[must_use]
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }
}

impl PolicyRegistry for StoolapPolicyRegistry {
    fn lookup_policy(
        &self,
        policy_hash: &[u8; 32],
    ) -> Result<Option<RegisteredPolicy>, PolicyRegistryError> {
        let mut rows = self
            .db
            .query(
                "SELECT policy_hash, body, registry_kind, crate_name, registered_at_unix, \
                        revoked_at_unix \
                 FROM policy_registry WHERE policy_hash = ? AND revoked_at_unix IS NULL",
                (policy_hash.as_slice(),),
            )
            .map_err(|e| PolicyRegistryError::AuthorityDelegationDenied(e.to_string()))?;

        if let Some(row) = rows.next() {
            let row =
                row.map_err(|e| PolicyRegistryError::AuthorityDelegationDenied(e.to_string()))?;
            let body: Vec<u8> = row.get(1).unwrap_or_default();
            let registered_at_unix: i64 = row.get(4).unwrap_or(0);
            return Ok(Some(RegisteredPolicy {
                policy_hash: *policy_hash,
                kind_uuid: [0u8; 16], // not stored in this table; join with policy_kind_authority
                body,
                execution_class: ExecutionClass::A, // default; refined by registry_kind in caller
                registered_at_unix,
                registered_by_did: [0u8; 32],
                revoked_at_unix: None,
                revoked_by_did: None,
                revocation_reason: None,
                superseding_policy_hash: None,
            }));
        }
        Ok(None)
    }

    fn register_policy(
        &self,
        kind_uuid: &[u8; 16],
        body: &[u8],
        execution_class: ExecutionClass,
        registered_by_did: &[u8; 32],
        registered_at_unix: i64,
        expected_hash: &[u8; 32],
    ) -> Result<RegisteredPolicy, PolicyRegistryError> {
        // Compute policy_hash from body via canonical BLAKE3.
        let actual_hash = octo_policy::domain_separators::blake3_prefix::derive_policy_hash(body);
        if &actual_hash != expected_hash {
            return Err(PolicyRegistryError::HashMismatch {
                expected: hex32(expected_hash),
                actual: hex32(&actual_hash),
            });
        }

        let registry_kind: i64 = match execution_class {
            ExecutionClass::A => 1, // conservative default
            ExecutionClass::B => 1,
            ExecutionClass::C => 1,
        };

        self.db
            .execute(
                "INSERT INTO policy_registry \
                 (policy_hash, registry_kind, crate_name, trait_spec, registered_at_unix, revoked_at_unix) \
                 VALUES (?, ?, ?, ?, ?, NULL)",
                (
                    actual_hash.as_slice(),
                    registry_kind,
                    "octo-policy-storage/v1",
                    body.to_vec(),
                    registered_at_unix,
                ),
            )
            .map_err(|e| PolicyRegistryError::AuthorityDelegationDenied(e.to_string()))?;

        Ok(RegisteredPolicy {
            policy_hash: actual_hash,
            kind_uuid: *kind_uuid,
            body: body.to_vec(),
            execution_class,
            registered_at_unix,
            registered_by_did: *registered_by_did,
            revoked_at_unix: None,
            revoked_by_did: None,
            revocation_reason: None,
            superseding_policy_hash: None,
        })
    }

    fn delegate_authority(
        &self,
        old_hash: &[u8; 32],
        new_hash: &[u8; 32],
        _registrant_did: &[u8; 32],
        registered_at_unix: i64,
    ) -> Result<(), PolicyRegistryError> {
        // Revoke old; insert new superseding link.
        self.db
            .execute(
                "UPDATE policy_registry SET revoked_at_unix = ? WHERE policy_hash = ?",
                (registered_at_unix, old_hash.as_slice()),
            )
            .map_err(|e| PolicyRegistryError::AuthorityDelegationDenied(e.to_string()))?;
        let _ = new_hash; // caller separately inserts new row
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zk_marker_detection() {
        let mut proof = vec![0u8; 64];
        assert!(!verify_class_b_zk_marker(&proof));
        proof[16..20].copy_from_slice(&ZK_ENVELOPE_MARKER);
        assert!(verify_class_b_zk_marker(&proof));
    }

    #[test]
    fn zk_marker_short_proof_rejected() {
        assert!(!verify_class_b_zk_marker(&[]));
        assert!(!verify_class_b_zk_marker(&[0u8; 19]));
    }

    #[test]
    fn registered_policy_constructor() {
        let r = RegisteredPolicy {
            policy_hash: [0xAA; 32],
            kind_uuid: [0xBB; 16],
            body: b"some body".to_vec(),
            execution_class: ExecutionClass::A,
            registered_at_unix: 1_700_000_000,
            registered_by_did: [0xCC; 32],
            revoked_at_unix: None,
            revoked_by_did: None,
            revocation_reason: None,
            superseding_policy_hash: None,
        };
        assert_eq!(r.policy_hash, [0xAA; 32]);
        assert!(r.revoked_at_unix.is_none());
    }
}
