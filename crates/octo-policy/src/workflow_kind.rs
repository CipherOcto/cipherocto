//! Example `WorkflowKind` implementations (RFC-0967-A1 v1.9.2 §2.1).
//!
//! Layer A substrate — frozen; semver-major only.
//!
//! Provides a minimal `VaultCreation` workflow that demonstrates the
//! `WorkflowKind` trait with primitive-type proof parameter (per
// R8 fix F-R8-WFCOMPOSITE-NO-PROOF-PARAM — no `ctx: &WorkflowContext`
//! phantom carrying proof bytes).

use crate::domain_separators::blake3_prefix;
use crate::kind_uuid_registry::kind_uuid_from_namespace;
use crate::policy_kinds::{
    AuditField, AuditPolicy, AuditVariant, AuthorityError, BurnError, CapabilityKind,
    ChainNamespace, ExecutionClass, InteropError, InteropOutcome, InteropSelector,
    InteropSelectorChoice, MembershipError, SelectorContext, SettlementEnvelope, SettlementError,
    SubjectProvisionRequest, UserInfoQuery, UserInfoResponse, UserUpdateRequest,
    VaultCreationRequest, WorkflowError, WorkflowKind, ZK_ENVELOPE_MARKER,
};

pub const VAULT_CREATION_NS: &str = "octo/workflow/vault-creation/v1";
pub const SUBJECT_PROVISION_NS: &str = "octo/workflow/subject-provision/v1";
pub const USER_INFO_READ_NS: &str = "octo/workflow/user-info-read/v1";
pub const USER_UPDATE_NS: &str = "octo/workflow/user-update/v1";

/// `VaultCreationWorkflow` — primitive proof: &[u8] (no phantom WorkflowContext).
#[derive(Debug, Clone)]
pub struct VaultCreationWorkflow {
    pub kind_uuid: u128,
    pub policy_hash: [u8; 32],
    pub body: Vec<u8>,
    pub provisioning_api_kind_uuid: u128,
    pub provisioning_api_body: Vec<u8>,
}

impl VaultCreationWorkflow {
    /// Construct a fresh workflow kind from the canonical body bytes.
    pub fn new(body: Vec<u8>) -> Self {
        let policy_hash = blake3_prefix::derive_policy_hash(&body);
        let provisioning_api_body = b"provisioning_api_v1".to_vec();
        Self {
            kind_uuid: kind_uuid_from_namespace(VAULT_CREATION_NS),
            policy_hash,
            body,
            provisioning_api_kind_uuid: kind_uuid_from_namespace(SUBJECT_PROVISION_NS),
            provisioning_api_body,
        }
    }
}

impl Default for VaultCreationWorkflow {
    fn default() -> Self {
        Self::new(b"vault_creation_v1".to_vec())
    }
}

impl WorkflowKind for VaultCreationWorkflow {
    fn kind_uuid(&self) -> u128 {
        self.kind_uuid
    }
    fn policy_hash(&self) -> [u8; 32] {
        self.policy_hash
    }
    fn body(&self) -> &[u8] {
        &self.body
    }
    fn execution_class(&self) -> ExecutionClass {
        ExecutionClass::A
    }
    fn provisioning_api_kind_uuid(&self) -> u128 {
        self.provisioning_api_kind_uuid
    }
    fn provisioning_api_body(&self) -> &[u8] {
        &self.provisioning_api_body
    }

    fn validate_vault_creation(
        &self,
        _req: &VaultCreationRequest,
        proof: &[u8],
    ) -> Result<(), WorkflowError> {
        // Class A: no proof required, but accept non-empty proof for B-compat path.
        let _ = proof;
        Ok(())
    }

    fn provision_subject(
        &self,
        req: &SubjectProvisionRequest,
        proof: &[u8],
    ) -> Result<(), WorkflowError> {
        // R12 fresh fix F-R8-WFCOMPOSITE-NO-PROOF-PARAM: proof is &[u8] primitive.
        if req.membership_proof.len() < 32 {
            return Err(WorkflowError::SubjectProvisionFailed(
                "membership_proof < 32 bytes".into(),
            ));
        }
        // Surface error if class B marker present but execution class is A.
        if proof.len() >= 20 && proof[16..20] == ZK_ENVELOPE_MARKER {
            return Err(WorkflowError::ClassBRequiresZkProof);
        }
        Ok(())
    }

    fn read_user_info(
        &self,
        _query: &UserInfoQuery,
        _proof: &[u8],
    ) -> Result<UserInfoResponse, WorkflowError> {
        Err(WorkflowError::UserInfoNotFound)
    }

    fn update_user(&self, _req: &UserUpdateRequest, _proof: &[u8]) -> Result<(), WorkflowError> {
        Err(WorkflowError::UserUpdateRejected("not implemented".into()))
    }
}

/// `FullAuditPolicy` — emit all 9 fields.
#[derive(Debug, Clone, Default)]
pub struct FullAuditPolicy;

// Aligned to RFC-0967-A1 §2.6 audit "testnet-verbose" entry per F9 drift fix.
const FULL_AUDIT_NS: &str = "octo/audit/testnet/v1";

impl FullAuditPolicy {
    pub fn new() -> Self {
        Self
    }
}

impl AuditPolicy for FullAuditPolicy {
    fn kind_uuid(&self) -> u128 {
        kind_uuid_from_namespace(FULL_AUDIT_NS)
    }
    fn policy_hash(&self) -> [u8; 32] {
        blake3_prefix::derive_policy_hash(FULL_AUDIT_NS.as_bytes())
    }
    fn body(&self) -> &[u8] {
        FULL_AUDIT_NS.as_bytes()
    }
    fn execution_class(&self) -> ExecutionClass {
        ExecutionClass::A
    }
    fn emit_fields(&self) -> &'static [AuditField] {
        &[
            AuditField::VaultId,
            AuditField::ChainId,
            AuditField::OwnerDid,
            AuditField::AmountDqaMicros,
            AuditField::SettlementHash,
            AuditField::AskerDid,
            AuditField::HolderDid,
            AuditField::PolicyHash,
            AuditField::Timestamp,
        ]
    }
    fn variant_assignment(&self, chain_id: &[u8; 32]) -> AuditVariant {
        AuditVariant::from_chain_id(chain_id, 2)
    }
}

/// `TimedUnlockBurnPolicy` — Class A burn policy with 1-hour unlock window.
#[derive(Debug, Clone, Default)]
pub struct TimedUnlockBurnPolicy;

// Aligned to RFC-0967-A1 §2.6 burn "time-locked" entry per F9 drift fix.
const TIMED_UNLOCK_BURN_NS: &str = "octo/burn/timelock/v1";

impl TimedUnlockBurnPolicy {
    pub fn new() -> Self {
        Self
    }
}

impl crate::policy_kinds::BurnPolicy for TimedUnlockBurnPolicy {
    fn kind_uuid(&self) -> u128 {
        kind_uuid_from_namespace(TIMED_UNLOCK_BURN_NS)
    }
    fn policy_hash(&self) -> [u8; 32] {
        blake3_prefix::derive_policy_hash(TIMED_UNLOCK_BURN_NS.as_bytes())
    }
    fn body(&self) -> &[u8] {
        TIMED_UNLOCK_BURN_NS.as_bytes()
    }
    fn execution_class(&self) -> ExecutionClass {
        ExecutionClass::A
    }
    fn allowed_chain_namespaces(&self) -> &'static [ChainNamespace] {
        &[ChainNamespace::Rfc, ChainNamespace::User]
    }
    fn validate_unlock_window(
        &self,
        unlock_at_unix: i64,
        window_basis: i64,
    ) -> Result<(), BurnError> {
        // Window: [window_basis, window_basis + 3600]
        let min = window_basis;
        let max = window_basis + 3600;
        if unlock_at_unix < min || unlock_at_unix > max {
            return Err(BurnError::UnlockOutOfWindow {
                requested: unlock_at_unix,
                min,
                max,
            });
        }
        Ok(())
    }
    fn requires_capability(&self) -> Option<CapabilityKind> {
        Some(CapabilityKind::SingleKey)
    }
}

/// `DidAttestationMembershipPolicy` — Class A membership gate.
#[derive(Debug, Clone, Default)]
pub struct DidAttestationMembershipPolicy;

// Aligned to RFC-0967-A1 §2.6 membership "didattestation" entry per F9
// drift fix (RFC §2.6 uses no-kebab `didattestation`, not `did-attestation`).
const DID_ATTESTATION_NS: &str = "octo/membership/didattestation/v1";

impl crate::policy_kinds::MembershipPolicy for DidAttestationMembershipPolicy {
    fn kind_uuid(&self) -> u128 {
        kind_uuid_from_namespace(DID_ATTESTATION_NS)
    }
    fn policy_hash(&self) -> [u8; 32] {
        blake3_prefix::derive_policy_hash(DID_ATTESTATION_NS.as_bytes())
    }
    fn body(&self) -> &[u8] {
        DID_ATTESTATION_NS.as_bytes()
    }
    fn execution_class(&self) -> ExecutionClass {
        ExecutionClass::A
    }
    fn validate(&self, proof: &[u8]) -> Result<(), MembershipError> {
        if proof.len() < 32 {
            return Err(MembershipError::DidAttestationInvalid);
        }
        Ok(())
    }
}

/// `SingleKeyAuthorityPolicy` — Class A authority policy (mint authorization).
#[derive(Debug, Clone, Default)]
pub struct SingleKeyAuthorityPolicy;

const SINGLE_KEY_AUTHORITY_NS: &str = "octo/auth/singlekey/v1";

impl crate::policy_kinds::AuthorityPolicy for SingleKeyAuthorityPolicy {
    fn kind_uuid(&self) -> u128 {
        kind_uuid_from_namespace(SINGLE_KEY_AUTHORITY_NS)
    }
    fn policy_hash(&self) -> [u8; 32] {
        blake3_prefix::derive_policy_hash(SINGLE_KEY_AUTHORITY_NS.as_bytes())
    }
    fn body(&self) -> &[u8] {
        SINGLE_KEY_AUTHORITY_NS.as_bytes()
    }
    fn execution_class(&self) -> ExecutionClass {
        ExecutionClass::A
    }
    fn validate(&self, proof: &[u8]) -> Result<(), AuthorityError> {
        if proof.len() < 64 {
            return Err(AuthorityError::SignatureInvalid);
        }
        Ok(())
    }
}

/// `PrimaryOrSecondaryInteropPolicy` — Class A interop policy with selector.
#[derive(Debug, Clone, Default)]
pub struct PrimaryOrSecondaryInteropPolicy;

pub const PRIMARY_OR_SECONDARY_NS: &str = "octo/interop/primary-or-secondary/v1";
pub const BYAMOUNT_SELECTOR_NS: &str = "octo/selector/byamount/v1";

#[derive(Debug, Clone, Default)]
pub struct ByAmountSelector;

impl InteropSelector for ByAmountSelector {
    fn select(&self, ctx: &SelectorContext) -> InteropSelectorChoice {
        // Prefer Primary for amounts > 1M DQA-micros, else Secondary.
        if ctx.amount_dqa_micros > 1_000_000 {
            InteropSelectorChoice::Primary
        } else {
            InteropSelectorChoice::Secondary
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct PassthroughOutcome;

impl InteropOutcome for PassthroughOutcome {
    fn apply(
        &self,
        _env: &mut SettlementEnvelope,
        _state_snapshot: &[u8; 32],
    ) -> Result<(), SettlementError> {
        Ok(())
    }
}

impl crate::policy_kinds::InteropPolicy for PrimaryOrSecondaryInteropPolicy {
    fn kind_uuid(&self) -> u128 {
        kind_uuid_from_namespace(PRIMARY_OR_SECONDARY_NS)
    }
    fn policy_hash(&self) -> [u8; 32] {
        blake3_prefix::derive_policy_hash(PRIMARY_OR_SECONDARY_NS.as_bytes())
    }
    fn body(&self) -> &[u8] {
        PRIMARY_OR_SECONDARY_NS.as_bytes()
    }
    fn execution_class(&self) -> ExecutionClass {
        ExecutionClass::A
    }
    fn selector(&self) -> &dyn InteropSelector {
        &ByAmountSelector
    }
    fn validate_transfer(
        &self,
        _env: &SettlementEnvelope,
        _src: &[u8; 32],
        _dst: &[u8; 32],
    ) -> Result<Box<dyn InteropOutcome>, InteropError> {
        Ok(Box::new(PassthroughOutcome))
    }
}

/// Reference selector kind_uuid for InteropSelector trait object (RFC-0967-A1 §2.6).
///
/// Per F8 fix: removed the `pub const BYAMOUNT_SELECTOR_KIND_UUID: u128 = 1`
/// placeholder; callers must invoke `kind_uuid_from_namespace(BYAMOUNT_SELECTOR_NS)`
/// directly. The derivation is deterministic and yields the value asserted
/// in `byamount_selector_kind_uuid_derivation` below.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy_kinds::{AuthorityPolicy, BurnPolicy, MembershipPolicy};

    #[test]
    fn vault_creation_workflow_default_works() {
        let wf = VaultCreationWorkflow::default();
        let req = VaultCreationRequest {
            owner_did: [0x01; 32],
            chain_id: [0x02; 32],
            asset_id: [0x03; 32],
            initial_balance_dqa_micros: 1000,
        };
        assert!(wf.validate_vault_creation(&req, b"some proof").is_ok());
    }

    #[test]
    fn provision_subject_rejects_short_proof() {
        let wf = VaultCreationWorkflow::default();
        let req = SubjectProvisionRequest {
            subject_did: [0x01; 32],
            chain_id: [0x02; 32],
            owner_did: [0x03; 32],
            membership_proof: vec![0xAA; 16],
        };
        assert!(wf.provision_subject(&req, &[]).is_err());
    }

    #[test]
    fn provision_subject_accepts_long_proof() {
        let wf = VaultCreationWorkflow::default();
        let req = SubjectProvisionRequest {
            subject_did: [0x01; 32],
            chain_id: [0x02; 32],
            owner_did: [0x03; 32],
            membership_proof: vec![0xAA; 64],
        };
        assert!(wf.provision_subject(&req, &[]).is_ok());
    }

    #[test]
    fn provision_subject_rejects_class_b_marker_on_class_a_workflow() {
        let wf = VaultCreationWorkflow::default();
        let req = SubjectProvisionRequest {
            subject_did: [0x01; 32],
            chain_id: [0x02; 32],
            owner_did: [0x03; 32],
            membership_proof: vec![0xAA; 64],
        };
        let mut proof = vec![0u8; 64];
        proof[16..20].copy_from_slice(&ZK_ENVELOPE_MARKER);
        assert!(matches!(
            wf.provision_subject(&req, &proof).unwrap_err(),
            WorkflowError::ClassBRequiresZkProof
        ));
    }

    #[test]
    fn full_audit_policy_emits_9_fields() {
        let p = FullAuditPolicy::new();
        assert_eq!(p.emit_fields().len(), 9);
    }

    #[test]
    fn timed_unlock_burn_validates_window() {
        let p = TimedUnlockBurnPolicy::new();
        let basis = 1_700_000_000;
        assert!(p.validate_unlock_window(basis + 60, basis).is_ok());
        assert!(p.validate_unlock_window(basis + 7200, basis).is_err());
    }

    #[test]
    fn did_attestation_membership_validates_proof() {
        let p = DidAttestationMembershipPolicy;
        assert!(p.validate(&vec![0u8; 64]).is_ok());
        assert!(p.validate(&[]).is_err());
    }

    #[test]
    fn single_key_authority_validates_proof() {
        let p = SingleKeyAuthorityPolicy;
        assert!(p.validate(&vec![0u8; 64]).is_ok());
        assert!(p.validate(&[]).is_err());
    }

    #[test]
    fn interop_selector_picks_primary_for_large_amount() {
        let selector = ByAmountSelector;
        let src = [0x01; 32];
        let dst = [0x02; 32];
        let candidates = [0u128; 0];
        let ctx = SelectorContext {
            src_chain_id: &src,
            dst_chain_id: &dst,
            amount_dqa_micros: 2_000_000,
            asset_namespace: b"octo",
            candidate_policies: &candidates,
        };
        assert_eq!(selector.select(&ctx), InteropSelectorChoice::Primary);
    }

    #[test]
    fn interop_selector_picks_secondary_for_small_amount() {
        let selector = ByAmountSelector;
        let src = [0x01; 32];
        let dst = [0x02; 32];
        let candidates = [0u128; 0];
        let ctx = SelectorContext {
            src_chain_id: &src,
            dst_chain_id: &dst,
            amount_dqa_micros: 500,
            asset_namespace: b"octo",
            candidate_policies: &candidates,
        };
        assert_eq!(selector.select(&ctx), InteropSelectorChoice::Secondary);
    }

    #[test]
    fn kind_uuids_are_deterministic() {
        let k1 = kind_uuid_from_namespace(VAULT_CREATION_NS);
        let k2 = kind_uuid_from_namespace(VAULT_CREATION_NS);
        assert_eq!(k1, k2);
    }

    #[test]
    fn kind_uuids_differ_across_namespaces() {
        let k1 = kind_uuid_from_namespace(VAULT_CREATION_NS);
        let k2 = kind_uuid_from_namespace(SUBJECT_PROVISION_NS);
        assert_ne!(k1, k2);
    }

    #[test]
    fn byamount_selector_kind_uuid_derivation() {
        let k = kind_uuid_from_namespace(BYAMOUNT_SELECTOR_NS);
        assert_ne!(k, 0);
    }

    #[test]
    fn user_info_read_and_update_namespaces_derive() {
        let a = kind_uuid_from_namespace(USER_INFO_READ_NS);
        let b = kind_uuid_from_namespace(USER_UPDATE_NS);
        assert_ne!(a, b);
    }
}
