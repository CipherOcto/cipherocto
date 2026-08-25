//! `PolicyRegistry` trait + `RegisteredPolicy` + `PolicyRegistryError`
//! (RFC-0967-A1 v1.9.2 §2.4 + §2.5).
//!
//! Per R4 fix C1: trait surface lives in `octo-policy` (the owner
//! crate); adapters (e.g. `octo-policy-storage`) provide the
//! implementation. Consumers depend only on the trait — adapters
//! can swap without trait churn (Stable Abstractions Principle:
//! trait depends on stable substrate, not the reverse).
//!
//! Layer A trait surface — RFC-frozen, semver-major only.
//! Substrate-truth: trait additions are RFC-driven; new error
//! variants are non-breaking (consumer `match` is exhaustive by
//! `#[non_exhaustive]` future-proofing once the team approves).

use crate::policy_kinds::{ExecutionClass, ZK_ENVELOPE_MARKER};

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

/// `PolicyRegistryError` — typed substrate errors.
///
/// All variants are RFC-anchored:
/// - `HashMismatch`: Layer A primitive — expected_hash mismatch.
/// - `NotFound`: lookup miss (RFC-0967-A1 §2.5 fail-closed).
/// - `ClassBRequiresZkProof`: Class B without marker (advisory,
///   not enforced by `register_policy`'s current impl which uses
///   `InvalidClassBProof`; retained for forward compatibility).
/// - `InvalidClassBProof`: R4 fix D2 — Class B registration gate
///   rejects body bytes without the ZK envelope marker at
///   `body[16..20]`.
/// - `AlreadyRegistered`: R4 fix B2 — duplicate `policy_hash`
///   guard rejects the second insert.
/// - `NotRegistrant`: R4 fix B1 — `delegate_authority` rejects
///   when caller DID != original registrant DID.
/// - `AlreadyRevoked`: revocation attempted on already-revoked.
/// - `AuthorityDelegationDenied`: catch-all substrate error.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
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
    /// R4 fix D2: Class B registration gate. Body bytes lack
    /// the ZK envelope marker at `[16..20]`. The substrate
    /// rejects the insert before any row is written to either
    /// `policy_registry` or `policy_kind_authority` (B3 atomic
    /// guarantee).
    #[error("class B registration rejected: ZK envelope marker missing at body[16..20]")]
    InvalidClassBProof,
    /// R4 fix B2: duplicate `policy_hash` pre-check rejects the
    /// insert before any row is written. The Stoolap fork does
    /// NOT enforce BLOB(32) PRIMARY KEY uniqueness at the
    /// storage layer (substrate-truth 2026-08-24), so the
    /// application-layer guard is the load-bearing defense.
    #[error("policy_hash {0} is already registered")]
    AlreadyRegistered(String),
    /// R4 fix B1: `delegate_authority` rejected because the
    /// caller is not the original registrant of `old_hash`.
    /// Fail-closed.
    #[error("caller is not the original registrant of policy_hash {0}")]
    NotRegistrant(String),
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
/// `delegate_authority`. Adapters (e.g. `octo-policy-storage`)
/// provide the implementation; consumers depend only on the
/// trait.
///
/// Layer A trait surface — RFC-frozen. New methods require an
/// RFC amendment; signature changes to existing methods are
/// semver-major.
pub trait PolicyRegistry: Send + Sync {
    /// Lookup a policy by its content hash.
    fn lookup_policy(
        &self,
        policy_hash: &[u8; 32],
    ) -> Result<Option<RegisteredPolicy>, PolicyRegistryError>;

    /// Register a new policy. The registry verifies `expected_hash`
    /// matches `BLAKE3(POLICY_HASH_DOMAIN || body)`, then atomically
    /// writes BOTH `policy_registry` and `policy_kind_authority`
    /// rows in a single Stoolap transaction (R4 fix B3).
    ///
    /// `registrant_signature` is the Ed25519 signature over the
    /// canonical body bytes, recorded on `policy_kind_authority`
    /// per RFC-0967-A1 §2.4 row 4 (mirrors
    /// `ledger_chain_registry`'s `operator_signature` shape).
    fn register_policy(
        &self,
        kind_uuid: &[u8; 16],
        body: &[u8],
        execution_class: ExecutionClass,
        registered_by_did: &[u8; 32],
        registrant_signature: &[u8; 64],
        registered_at_unix: i64,
        expected_hash: &[u8; 32],
    ) -> Result<RegisteredPolicy, PolicyRegistryError>;

    /// Delegate authority: replace one policy with another, with
    /// the `superseding_policy_hash` recorded on the old row.
    ///
    /// R4 fix B1: `registrant_did` MUST equal the existing row's
    /// `registered_by_did`; otherwise the call fails closed with
    /// `Err(PolicyRegistryError::NotRegistrant)`.
    fn delegate_authority(
        &self,
        old_hash: &[u8; 32],
        new_hash: &[u8; 32],
        registrant_did: &[u8; 32],
        registered_at_unix: i64,
    ) -> Result<(), PolicyRegistryError>;
}

/// Verify the ZK envelope marker is present at proof[16..20] for
/// Class B policies. Per RFC-0967-A1 v1.9.2 §3 row 4 + RFC-0010
/// v1.9.2 §Class B consensus path: a Class B body MUST carry the
/// marker. The body bytes encode the marker at offset 16..20 (4
/// bytes; canonical `ZK_ENVELOPE_MARKER = [0x01, 0x7a, 0x6b, 0x00]`).
///
/// R4 fix D2: this function is wired into `register_policy` so
/// Class B bodies without the marker are rejected at the
/// registration gate (fail-closed). The marker is part of the
/// canonical body, not an out-of-band witness.
#[must_use]
pub fn verify_class_b_zk_marker(proof: &[u8]) -> bool {
    proof.len() >= 20 && proof[16..20] == ZK_ENVELOPE_MARKER
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

    #[test]
    fn policy_registry_error_display_contains_hex() {
        let e = PolicyRegistryError::AlreadyRegistered(
            "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff".to_owned(),
        );
        let s = format!("{e}");
        assert!(s.contains("policy_hash"));
        assert!(s.contains("already registered"));
    }

    #[test]
    fn policy_registry_error_not_registrant_display() {
        let e = PolicyRegistryError::NotRegistrant("deadbeef".repeat(8));
        let s = format!("{e}");
        assert!(s.contains("not the original registrant"));
    }
}
