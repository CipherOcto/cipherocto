//! Six per-policy-kind traits + supporting RFC-defined types (RFC-0967-A1 v1.9.2).
//!
//! Layer A substrate — frozen; semver-major only.

use crate::domain_separators::blake3_prefix;
use crate::kind_uuid_registry::KIND_NAMESPACE_STRINGS;

/// Execution class (RFC-0008 §Data Structures discriminant: `A = 0x00, B = 0x01, C = 0x02`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExecutionClass {
    /// Class A: deterministic, substrate-validated (no ZK proof required).
    A,
    /// Class B: consensus-path; requires ZK envelope marker at `proof[16..20]`.
    B,
    /// Class C: registration-time rejected per RFC-0967-A1 §3.
    C,
}

impl ExecutionClass {
    pub fn as_byte(&self) -> u8 {
        match self {
            ExecutionClass::A => 0x00,
            ExecutionClass::B => 0x01,
            ExecutionClass::C => 0x02,
        }
    }

    pub fn from_byte(b: u8) -> Option<Self> {
        match b {
            0x00 => Some(ExecutionClass::A),
            0x01 => Some(ExecutionClass::B),
            0x02 => Some(ExecutionClass::C),
            _ => None,
        }
    }

    /// Canonical TEXT form per RFC-0967-A1 §2.4 `execution_class`
    /// column (single ASCII char: "A" / "B" / "C").
    ///
    /// R5 fix D3: substrate persists `execution_class` as TEXT, not
    /// INTEGER. The byte representation (`as_byte`) is preserved for
    /// wire-format interop (RFC-0008 §Data Structures discriminant),
    /// but storage column bindings MUST go through this method so
    /// the substrate's TEXT type-check accepts the value.
    pub fn as_text(&self) -> &'static str {
        match self {
            ExecutionClass::A => "A",
            ExecutionClass::B => "B",
            ExecutionClass::C => "C",
        }
    }
}

/// ChainNamespace 1-byte variant (RFC-0967-A1 v1.9.2 + RFC-0010 §Data Model).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChainNamespace {
    /// 0x01 Rfc — RFC-allocated namespace (e.g. CIPHEROCTO_MAINNET).
    Rfc,
    /// 0x02 User — corporate/private chain.
    User,
}

impl ChainNamespace {
    pub fn as_byte(&self) -> u8 {
        match self {
            ChainNamespace::Rfc => 0x01,
            ChainNamespace::User => 0x02,
        }
    }
}

/// CapabilityKind placeholder (RFC-defined pending substrate landing per RFC-0967-A1 §2.1).
/// RFC-0957-A1 defines `HolderKind` (V1/ZKBearing/Bearer/HopCapability); `CapabilityKind`
/// is a placeholder until substrate catches up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CapabilityKind {
    /// Single-key signature (Ed25519).
    SingleKey,
    /// Multi-signature scheme (k-of-n).
    Multisig,
    /// Bearer capability (RFC-0957 macaroon).
    Bearer,
    /// ZK-bearing capability (Class B).
    ZkBearing,
    /// Hop capability (delegation chain).
    Hop,
}

/// AuthorityError — mint authorization failure.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AuthorityError {
    #[error("class B authority policy requires ZK envelope marker at proof[16..20]")]
    ClassBRequiresZkProof,
    #[error("signature verification failed")]
    SignatureInvalid,
    #[error("unauthorized: caller does not hold required capability")]
    Unauthorized,
}

/// MembershipError — vault creation gate failure.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MembershipError {
    #[error("class B membership policy requires ZK envelope marker at proof[16..20]")]
    ClassBRequiresZkProof,
    #[error("DID attestation missing or invalid")]
    DidAttestationInvalid,
    #[error("membership proof insufficient: {0}")]
    InsufficientProof(String),
}

/// InteropError — cross-chain transfer validation failure.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InteropError {
    #[error("class B interop policy requires ZK envelope marker at env.carry_proof[16..20]")]
    ClassBRequiresZkProof,
    #[error("no compatible interop selector candidate")]
    NoCompatibleCandidate,
    #[error("state drift detected (TOCTOU race bounded by snapshot hash mismatch)")]
    StateDrift,
    #[error("interop rejection: {0}")]
    Rejection(String),
}

/// BurnError — burn unlock window validation failure.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BurnError {
    #[error("unlock_at_unix {requested} is outside window [{min}, {max}]")]
    UnlockOutOfWindow { requested: i64, min: i64, max: i64 },
    #[error("chain namespace {0:?} not in policy allowed_chain_namespaces")]
    ChainNamespaceNotAllowed(ChainNamespace),
}

/// WorkflowError — vault provisioning workflow failure.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WorkflowError {
    #[error("class B workflow requires ZK envelope marker at proof[16..20]")]
    ClassBRequiresZkProof,
    #[error("composite workflow depth exceeded MAX_COMPOSITE_DEPTH = 4")]
    CompositeWorkflowDepthExceeded,
    #[error("vault creation rejected: {0}")]
    VaultCreationRejected(String),
    #[error("subject provision failed: {0}")]
    SubjectProvisionFailed(String),
    #[error("user info not found")]
    UserInfoNotFound,
    #[error("user update rejected: {0}")]
    UserUpdateRejected(String),
}

/// SettlementError — settlement envelope application failure.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SettlementError {
    #[error("state drift detected (TOCTOU race bounded by snapshot hash mismatch)")]
    StateDrift,
    #[error("settlement rejected: {0}")]
    Rejection(String),
}

/// AuditField — event emission field selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AuditField {
    VaultId,
    ChainId,
    OwnerDid,
    AmountDqaMicros,
    SettlementHash,
    AskerDid,
    HolderDid,
    PolicyHash,
    Timestamp,
}

/// AuditVariant — A/B kind-specific hash-derived variant assignment (V=2 typical).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AuditVariant {
    A,
    B,
}

impl AuditVariant {
    /// Derive AuditVariant from chain_id via BLAKE3("octo/audit/ab/v1/" || chain_id_bytes)[0] % V.
    pub fn from_chain_id(chain_id: &[u8; 32], variant_cardinality: u8) -> Self {
        debug_assert!(variant_cardinality >= 2);
        let hash = blake3_prefix::derive_audit_variant(chain_id, variant_cardinality as u32);
        match hash % variant_cardinality as u64 {
            0 => AuditVariant::A,
            _ => AuditVariant::B,
        }
    }
}

/// VaultCreationRequest — RFC-defined request type (RFC-0967-A1 §2.1 WorkflowKind).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultCreationRequest {
    pub owner_did: [u8; 32],
    pub chain_id: [u8; 32],
    pub asset_id: [u8; 32],
    pub initial_balance_dqa_micros: i64,
}

/// SubjectProvisionRequest — RFC-defined request type (RFC-0967-A1 §2.1 WorkflowKind).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubjectProvisionRequest {
    pub subject_did: [u8; 32],
    pub chain_id: [u8; 32],
    pub owner_did: [u8; 32],
    pub membership_proof: Vec<u8>,
}

/// UserInfoQuery — RFC-defined query type (RFC-0967-A1 §2.1 WorkflowKind).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserInfoQuery {
    pub user_did: [u8; 32],
    pub chain_id: [u8; 32],
}

/// UserInfoResponse — RFC-defined response type (RFC-0967-A1 §2.1 WorkflowKind).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserInfoResponse {
    pub user_did: [u8; 32],
    pub chain_id: [u8; 32],
    pub metadata: Vec<u8>,
}

/// UserUpdateRequest — RFC-defined request type (RFC-0967-A1 §2.1 WorkflowKind).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserUpdateRequest {
    pub user_did: [u8; 32],
    pub chain_id: [u8; 32],
    pub metadata: Vec<u8>,
}

/// SettlementEnvelope — RFC-defined wire form (RFC-0967-A1 §2.1 InteropPolicy).
///
/// Per R12 fresh fix F-R12-XR-CARRY-PROOF-PHANTOM-FIELD: `carry_proof` is a
/// sub-field of SettlementEnvelope defined in this RFC, NOT in RFC-0959.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettlementEnvelope {
    pub settlement_hash: [u8; 32],
    pub asker_did: [u8; 32],
    pub holder_did: [u8; 32],
    pub model: String,
    pub axes_consumed: i64,
    pub ask_id: [u8; 32],
    pub nonce: [u8; 32],
    pub timestamp_unix: i64,
    pub cost: i64,
    pub cost_vault_id: [u8; 32],
    pub chain_id: [u8; 32],
    /// ZK envelope proof (RFC-defined extension point per RFC-0967-A1 §2.1).
    /// Layout: [0..16] kind_uuid / [16..20] ZK envelope marker / [20..N] body.
    pub carry_proof: Vec<u8>,
}

/// ZK envelope marker — 4-byte magic at `proof[16..20]` (RFC-0967-A1 §2.1).
pub const ZK_ENVELOPE_MARKER: [u8; 4] = [0x01, 0x7a, 0x6b, 0x00];

/// SelectorContext — RFC-defined context for InteropSelector (RFC-0967-A1 §2.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectorContext<'a> {
    pub src_chain_id: &'a [u8; 32],
    pub dst_chain_id: &'a [u8; 32],
    pub amount_dqa_micros: i64,
    pub asset_namespace: &'a [u8],
    pub candidate_policies: &'a [u128],
}

/// InteropSelectorChoice — RFC-defined discriminator (RFC-0967-A1 §2.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InteropSelectorChoice {
    Primary,
    Secondary,
    Reject,
}

// ─────────────────────────────────────────────────────────────────────
// Six per-policy-kind traits (RFC-0967-A1 v1.9.2 §2.1)
// ─────────────────────────────────────────────────────────────────────

/// AuthorityPolicy — mint authorization (Class A or B-with-ZK-proof).
pub trait AuthorityPolicy: Send + Sync {
    fn kind_uuid(&self) -> u128;
    fn policy_hash(&self) -> [u8; 32];
    fn body(&self) -> &[u8];
    fn execution_class(&self) -> ExecutionClass;
    fn validate(&self, proof: &[u8]) -> Result<(), AuthorityError>;
}

/// MembershipPolicy — vault creation gate (Class A or B-with-ZK-proof).
pub trait MembershipPolicy: Send + Sync {
    fn kind_uuid(&self) -> u128;
    fn policy_hash(&self) -> [u8; 32];
    fn body(&self) -> &[u8];
    fn execution_class(&self) -> ExecutionClass;
    fn validate(&self, proof: &[u8]) -> Result<(), MembershipError>;
}

/// InteropPolicy — cross-chain transfer validation (Class A or B-with-ZK-proof).
pub trait InteropPolicy: Send + Sync {
    fn kind_uuid(&self) -> u128;
    fn policy_hash(&self) -> [u8; 32];
    fn body(&self) -> &[u8];
    fn execution_class(&self) -> ExecutionClass;
    fn selector(&self) -> &dyn InteropSelector;
    fn validate_transfer(
        &self,
        env: &SettlementEnvelope,
        src: &[u8; 32],
        dst: &[u8; 32],
    ) -> Result<Box<dyn InteropOutcome>, InteropError>;
}

/// InteropSelector — supporting trait object (RFC-0967-A1 §2.2).
pub trait InteropSelector: Send + Sync {
    fn select(&self, ctx: &SelectorContext) -> InteropSelectorChoice;
}

/// InteropOutcome — supporting trait object (RFC-0967-A1 §2.2).
pub trait InteropOutcome: Send + Sync {
    /// R6 fix F-R6-012: apply() re-validates by comparing current state hash
    /// to snapshot hash. On mismatch returns SettlementError::StateDrift.
    fn apply(
        &self,
        env: &mut SettlementEnvelope,
        state_snapshot: &[u8; 32],
    ) -> Result<(), SettlementError>;
}

/// BurnPolicy — burn timing + window + capability requirement (Class A).
pub trait BurnPolicy: Send + Sync {
    fn kind_uuid(&self) -> u128;
    fn policy_hash(&self) -> [u8; 32];
    fn body(&self) -> &[u8];
    fn execution_class(&self) -> ExecutionClass;
    fn allowed_chain_namespaces(&self) -> &'static [ChainNamespace];
    fn validate_unlock_window(
        &self,
        unlock_at_unix: i64,
        window_basis: i64,
    ) -> Result<(), BurnError>;
    fn requires_capability(&self) -> Option<CapabilityKind>;
}

/// WorkflowKind — vault provisioning workflow dispatch (Class A or B-with-ZK-proof).
///
/// R8 fix F-R8-WFCOMPOSITE-NO-PROOF-PARAM: primitive types only — `proof: &[u8]`
/// (no `ctx: &WorkflowContext` phantom carrying proof bytes).
pub trait WorkflowKind: Send + Sync {
    fn kind_uuid(&self) -> u128;
    fn policy_hash(&self) -> [u8; 32];
    fn body(&self) -> &[u8];
    fn execution_class(&self) -> ExecutionClass;
    fn provisioning_api_kind_uuid(&self) -> u128;
    fn provisioning_api_body(&self) -> &[u8];

    fn validate_vault_creation(
        &self,
        req: &VaultCreationRequest,
        proof: &[u8],
    ) -> Result<(), WorkflowError>;
    fn provision_subject(
        &self,
        req: &SubjectProvisionRequest,
        proof: &[u8],
    ) -> Result<(), WorkflowError>;
    fn read_user_info(
        &self,
        query: &UserInfoQuery,
        proof: &[u8],
    ) -> Result<UserInfoResponse, WorkflowError>;
    fn update_user(&self, req: &UserUpdateRequest, proof: &[u8]) -> Result<(), WorkflowError>;
}

/// AuditPolicy — event emission field selection + variant assignment.
pub trait AuditPolicy: Send + Sync {
    fn kind_uuid(&self) -> u128;
    fn policy_hash(&self) -> [u8; 32];
    fn body(&self) -> &[u8];
    fn execution_class(&self) -> ExecutionClass;
    fn emit_fields(&self) -> &'static [AuditField];
    fn variant_assignment(&self, chain_id: &[u8; 32]) -> AuditVariant;
}

/// Total count of per-policy-kind crates (per RFC-0967-A1 §2.6 + R6 fix F-R6-009).
pub const TOTAL_PER_POLICY_KIND: usize = 30;

/// Validate that the 30 per-policy-kind UUIDv5 namespace strings are present
/// in the kind_uuid_registry.
pub fn validate_kind_registry_complete() -> bool {
    KIND_NAMESPACE_STRINGS.len() == TOTAL_PER_POLICY_KIND
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn execution_class_byte_round_trip() {
        for c in [ExecutionClass::A, ExecutionClass::B, ExecutionClass::C] {
            assert_eq!(ExecutionClass::from_byte(c.as_byte()), Some(c));
        }
        assert_eq!(ExecutionClass::from_byte(0xFF), None);
    }

    #[test]
    fn chain_namespace_byte_round_trip() {
        for n in [ChainNamespace::Rfc, ChainNamespace::User] {
            assert_eq!(
                n.as_byte(),
                match n {
                    ChainNamespace::Rfc => 0x01,
                    ChainNamespace::User => 0x02,
                }
            );
        }
    }

    #[test]
    fn zk_envelope_marker_is_4_bytes() {
        assert_eq!(ZK_ENVELOPE_MARKER.len(), 4);
        assert_eq!(ZK_ENVELOPE_MARKER, [0x01, 0x7a, 0x6b, 0x00]);
    }

    #[test]
    fn total_per_policy_kind_is_30() {
        assert_eq!(TOTAL_PER_POLICY_KIND, 30);
    }

    #[test]
    fn kind_registry_has_30_namespaces() {
        assert_eq!(KIND_NAMESPACE_STRINGS.len(), 30);
        assert!(validate_kind_registry_complete());
    }

    #[test]
    fn audit_variant_derivation_is_deterministic() {
        let chain_id = [0xAA_u8; 32];
        let v1 = AuditVariant::from_chain_id(&chain_id, 2);
        let v2 = AuditVariant::from_chain_id(&chain_id, 2);
        assert_eq!(v1, v2, "AuditVariant derivation must be deterministic");
    }
}
