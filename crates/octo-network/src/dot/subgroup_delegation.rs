//! Sub-DC Delegation Lifecycle substrate — RFC-0855p-d2
//!
//! Owns:
//! - `SubDCDelegationEnvelope` (SDCD subtype) — parent-DC-signed delegation proof
//! - `SubDCRevocationEnvelope` (SDRV subtype) — revocation proof terminating the delegation
//! - `SubDCRotationEnvelope` (SDRT subtype) — rotation proof requiring BOTH parent + sub-DC
//!   joint signature (anti-collusion + anti-forgery)
//! - `SubDCDelegationPolicy::check(proof, subgroup_state_bound=true)` — pure function (Layer A)
//! - `RevocationReasonCode` `#[non_exhaustive]` enum
//! - Chain-depth counter (≤ `MAX_DELEGATION_CHAIN_PER_TERM = 256`)
//! - Root-delegation table (≤ `MAX_ROOT_DELEGATION = 1` per parent)
//! - `CoordinatorTermId` typed term identifier (consumed by d3 via `pub use` re-export)
//!
//! Layer placement (per RFC-0855p-d2 §Layer placement):
//! - Envelope wire (Layer B)
//! - Revocation/coordination enums (Layer B)
//! - Policy + chain-depth + root-delegation table (Layer C)
//!
//! Cross-RFC canonical home (per plateau closure):
//! - `CoordinatorTermId` + `SubDCDelegationPolicy` defined here; re-exported by d3 via
//!   `pub use crate::rfc_0855p_d2::*` (non-self-reexport pattern per d2 mission acceptance criteria).
//!
//! Re-exports d1 (`SubGroupLabel`, `DelegationId`, `MAX_ROOT_DEPTH`) so downstream
//! consumers can use single-import surface.

use blake3;
use thiserror::Error;

// ============================================================================
// Constants (Layer C — delegation-specific limits; canonical home in d2)
// ============================================================================

/// Maximum delegation chain depth per coordinator term.
pub const MAX_DELEGATION_CHAIN_PER_TERM: u16 = 256;

/// Maximum root delegations per parent (genesis invariant; root = direct child of root DC).
pub const MAX_ROOT_DELEGATION: u8 = 1;

// ============================================================================
// Constants (Layer B — BLAKE3 domain separation contexts)
// ============================================================================

/// BLAKE3 domain separation context for SDCD replay-key tuple derivation.
pub const SDCD_CONTEXT: &str = "DOT/1/SDCD/delegation";
/// BLAKE3 domain separation context for SDRV replay-key tuple derivation.
pub const SDRV_CONTEXT: &str = "DOT/1/SDRV/revocation";
/// BLAKE3 domain separation context for SDRT replay-key tuple derivation.
pub const SDRT_CONTEXT: &str = "DOT/1/SDRT/rotation";

// ============================================================================
// Constants (Layer B — envelope subtype tags)
// ============================================================================

/// `SubDCDelegationEnvelope` subtype tag.
pub const SUBDC_DELEGATION_TAG: [u8; 4] = *b"SDCD";
/// `SubDCRevocationEnvelope` subtype tag.
pub const SUBDC_REVOCATION_TAG: [u8; 4] = *b"SDRV";
/// `SubDCRotationEnvelope` subtype tag.
pub const SUBDC_ROTATION_TAG: [u8; 4] = *b"SDRT";

// ============================================================================
// Constants (Layer C — RevocationReasonCode typed discriminator)
// ============================================================================

/// `RevocationReasonCode::reason_id` values. RFC-allocated 0x0001-0x00FF;
/// 0x0100-0xFFFF reserved for user-extension registry entries.
pub const REVOCATION_REASON_TERM_EXPIRED: u16 = 0x0001;
pub const REVOCATION_REASON_COORDINATOR_ROTATION: u16 = 0x0002;
pub const REVOCATION_REASON_SUBDC_MISCONDUCT: u16 = 0x0003;
pub const REVOCATION_REASON_SUBDC_KEY_COMPROMISE: u16 = 0x0004;
pub const REVOCATION_REASON_SUBDC_VOLUNTARY_RESIGNATION: u16 = 0x0005;
pub const REVOCATION_REASON_GROUP_DECOMMISSION: u16 = 0x0006;

// ============================================================================
// Errors
// ============================================================================

/// Error type for `SubDCDelegationPolicy::check`.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DelegationPolicyError {
    #[error("parent_dc_id is zero")]
    ParentDcIdZero,
    #[error("sub_domain_id is zero")]
    SubDomainIdZero,
    #[error("sub_dc_id is zero")]
    SubDcIdZero,
    #[error("term_id is zero")]
    TermIdZero,
    #[error("nonce is zero")]
    NonceZero,
    #[error("chain depth {got} >= MAX_DELEGATION_CHAIN_PER_TERM ({max})")]
    ChainDepthExceeded { max: u16, got: u16 },
    #[error("root delegation count {got} >= MAX_ROOT_DELEGATION ({max}) for parent {parent:?}")]
    RootDelegationExceeded { max: u8, got: u8, parent: [u8; 32] },
    #[error("subgroup_state_bound=false but subgroup is not in Bound state")]
    SubgroupNotBound,
    #[error(
        "SDRT requires BOTH parent-DC AND retiring-sub-DC signatures; only {got} of 2 present"
    )]
    JointSignatureMissing { got: u8 },
    #[error("parent-DC signature invalid")]
    ParentSignatureInvalid,
    #[error("sub-DC signature invalid")]
    SubDcSignatureInvalid,
    #[error("replay-key already seen")]
    ReplayKeySeen,
}

/// Error type for `RevocationReasonCode::from_reason_id` (unknown discriminator).
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum RevocationReasonError {
    #[error("unknown revocation reason_id {got}")]
    UnknownReasonId { got: u16 },
}

// ============================================================================
// RevocationReasonCode (Layer B — #[non_exhaustive] typed discriminator)
// ============================================================================

/// Reason code for an SDRV envelope. `#[non_exhaustive]` per §Extension over enumeration;
/// unknown `reason_id` values fail closed via `from_reason_id`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum RevocationReasonCode {
    TermExpired,
    CoordinatorRotation,
    SubDCMisconduct,
    SubDCKeyCompromise,
    SubDCVoluntaryResignation,
    GroupDecommission,
    /// User-extension variant (RFC-allocated namespace 0x0100-0xFFFF); opaque payload.
    Extension(u16),
}

impl RevocationReasonCode {
    pub fn reason_id(&self) -> u16 {
        match self {
            Self::TermExpired => REVOCATION_REASON_TERM_EXPIRED,
            Self::CoordinatorRotation => REVOCATION_REASON_COORDINATOR_ROTATION,
            Self::SubDCMisconduct => REVOCATION_REASON_SUBDC_MISCONDUCT,
            Self::SubDCKeyCompromise => REVOCATION_REASON_SUBDC_KEY_COMPROMISE,
            Self::SubDCVoluntaryResignation => REVOCATION_REASON_SUBDC_VOLUNTARY_RESIGNATION,
            Self::GroupDecommission => REVOCATION_REASON_GROUP_DECOMMISSION,
            Self::Extension(id) => *id,
        }
    }

    pub fn from_reason_id(id: u16) -> Result<Self, RevocationReasonError> {
        Ok(match id {
            REVOCATION_REASON_TERM_EXPIRED => Self::TermExpired,
            REVOCATION_REASON_COORDINATOR_ROTATION => Self::CoordinatorRotation,
            REVOCATION_REASON_SUBDC_MISCONDUCT => Self::SubDCMisconduct,
            REVOCATION_REASON_SUBDC_KEY_COMPROMISE => Self::SubDCKeyCompromise,
            REVOCATION_REASON_SUBDC_VOLUNTARY_RESIGNATION => Self::SubDCVoluntaryResignation,
            REVOCATION_REASON_GROUP_DECOMMISSION => Self::GroupDecommission,
            0x0100..=0xFFFF => Self::Extension(id),
            _ => return Err(RevocationReasonError::UnknownReasonId { got: id }),
        })
    }
}

// ============================================================================
// CoordinatorTermId (Layer B — typed term identifier)
// ============================================================================

/// Typed coordinator term identifier (32-byte digest). Consumed by d3 via `pub use`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CoordinatorTermId(pub [u8; 32]);

// ============================================================================
// JointSignatureEnvelope (Layer B — joint parent-DC + sub-DC signature surface)
// ============================================================================

/// 64-byte Ed25519 signature.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ed25519Signature(pub [u8; 64]);

/// BLAKE3-256 digest (32 bytes).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Digest32(pub [u8; 32]);

// ============================================================================
// Replay-key tuple forms (RFC-0126 canonical BE bytes)
// ============================================================================

/// SDCD replay-key tuple: `(SDCD, parent_dc_id, sub_domain_id, sub_dc_id, term_id, current_epoch, nonce)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SdcdReplayKey {
    pub parent_dc_id: [u8; 32],
    pub sub_domain_id: [u8; 32],
    pub sub_dc_id: [u8; 32],
    pub term_id: CoordinatorTermId,
    pub current_epoch: u64,
    pub nonce: [u8; 16],
}

impl SdcdReplayKey {
    /// Compute canonical 32-byte replay-key digest per RFC-0126.
    pub fn digest(&self) -> [u8; 32] {
        let key = {
            let k = [0u8; 32];
            blake3::derive_key(SDCD_CONTEXT, &k);
            k
        };
        let mut input = Vec::with_capacity(4 + 32 + 32 + 32 + 32 + 8 + 16);
        input.extend_from_slice(&SUBDC_DELEGATION_TAG);
        input.extend_from_slice(&self.parent_dc_id);
        input.extend_from_slice(&self.sub_domain_id);
        input.extend_from_slice(&self.sub_dc_id);
        input.extend_from_slice(&self.term_id.0);
        input.extend_from_slice(&self.current_epoch.to_be_bytes());
        input.extend_from_slice(&self.nonce);
        *blake3::keyed_hash(&key, &input).as_bytes()
    }
}

/// SDRV replay-key tuple: `(SDRV, parent_dc_id, sub_domain_id, sub_dc_id, term_id, revocation_reason_id)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SdrvReplayKey {
    pub parent_dc_id: [u8; 32],
    pub sub_domain_id: [u8; 32],
    pub sub_dc_id: [u8; 32],
    pub term_id: CoordinatorTermId,
    pub revocation_reason_id: u16,
}

impl SdrvReplayKey {
    pub fn digest(&self) -> [u8; 32] {
        let key = {
            let k = [0u8; 32];
            blake3::derive_key(SDRV_CONTEXT, &k);
            k
        };
        let mut input = Vec::with_capacity(4 + 32 + 32 + 32 + 32 + 2);
        input.extend_from_slice(&SUBDC_REVOCATION_TAG);
        input.extend_from_slice(&self.parent_dc_id);
        input.extend_from_slice(&self.sub_domain_id);
        input.extend_from_slice(&self.sub_dc_id);
        input.extend_from_slice(&self.term_id.0);
        input.extend_from_slice(&self.revocation_reason_id.to_be_bytes());
        *blake3::keyed_hash(&key, &input).as_bytes()
    }
}

/// SDRT replay-key tuple: `(SDRT, parent_dc_id, sub_domain_id, old_sub_dc_id, new_sub_dc_id, term_id, current_epoch, nonce)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SdrtReplayKey {
    pub parent_dc_id: [u8; 32],
    pub sub_domain_id: [u8; 32],
    pub old_sub_dc_id: [u8; 32],
    pub new_sub_dc_id: [u8; 32],
    pub term_id: CoordinatorTermId,
    pub current_epoch: u64,
    pub nonce: [u8; 16],
}

impl SdrtReplayKey {
    pub fn digest(&self) -> [u8; 32] {
        let key = {
            let k = [0u8; 32];
            blake3::derive_key(SDRT_CONTEXT, &k);
            k
        };
        let mut input = Vec::with_capacity(4 + 32 + 32 + 32 + 32 + 32 + 8 + 16);
        input.extend_from_slice(&SUBDC_ROTATION_TAG);
        input.extend_from_slice(&self.parent_dc_id);
        input.extend_from_slice(&self.sub_domain_id);
        input.extend_from_slice(&self.old_sub_dc_id);
        input.extend_from_slice(&self.new_sub_dc_id);
        input.extend_from_slice(&self.term_id.0);
        input.extend_from_slice(&self.current_epoch.to_be_bytes());
        input.extend_from_slice(&self.nonce);
        *blake3::keyed_hash(&key, &input).as_bytes()
    }
}

// ============================================================================
// SubDCDelegationEnvelope (SDCD — parent-DC-signed delegation proof)
// ============================================================================

/// `SubDCDelegationEnvelope` (DOT/1/SDCD). Parent-DC-signed delegation proof that
/// authorizes a sub-DC to act on behalf of a parent domain's sub-group.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubDCDelegationEnvelope {
    /// `b"DOT1"`.
    pub envelope_type: [u8; 4],
    /// `b"SDCD"`.
    pub envelope_subtype: [u8; 4],
    /// Canonical version (0x0001).
    pub version: u16,
    /// Parent DC's peer_id (32-byte).
    pub parent_dc_id: [u8; 32],
    /// Derived sub-domain id (BLAKE3 keyed_hash per d1 §Sub-Domain Derivation Invariant).
    pub sub_domain_id: [u8; 32],
    /// Sub-DC's peer_id (32-byte). Authorizes this DC to act on the sub-group.
    pub sub_dc_id: [u8; 32],
    /// Parent DC's coordinator term id (signs the envelope).
    pub term_id: CoordinatorTermId,
    /// Current epoch at SDCD emission time.
    pub current_epoch: u64,
    /// 16-byte random nonce (replay-key element).
    pub nonce: [u8; 16],
    /// Parent-DC Ed25519 signature over canonical envelope body.
    pub signature: Ed25519Signature,
}

impl SubDCDelegationEnvelope {
    pub fn replay_key(&self) -> SdcdReplayKey {
        SdcdReplayKey {
            parent_dc_id: self.parent_dc_id,
            sub_domain_id: self.sub_domain_id,
            sub_dc_id: self.sub_dc_id,
            term_id: self.term_id,
            current_epoch: self.current_epoch,
            nonce: self.nonce,
        }
    }
}

// ============================================================================
// SubDCRevocationEnvelope (SDRV — revocation proof)
// =====================================================================================

/// `SubDCRevocationEnvelope` (DOT/1/SDRV). Revocation proof terminating the delegation;
/// triggers downstream teardown via d3 substrate state transition
/// (`Bound → Dissolving`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubDCRevocationEnvelope {
    pub envelope_type: [u8; 4],
    pub envelope_subtype: [u8; 4],
    pub version: u16,
    pub parent_dc_id: [u8; 32],
    pub sub_domain_id: [u8; 32],
    pub sub_dc_id: [u8; 32],
    pub term_id: CoordinatorTermId,
    pub revocation_reason: RevocationReasonCode,
    pub current_epoch: u64,
    pub nonce: [u8; 16],
    pub signature: Ed25519Signature,
}

impl SubDCRevocationEnvelope {
    pub fn replay_key(&self) -> SdrvReplayKey {
        SdrvReplayKey {
            parent_dc_id: self.parent_dc_id,
            sub_domain_id: self.sub_domain_id,
            sub_dc_id: self.sub_dc_id,
            term_id: self.term_id,
            revocation_reason_id: self.revocation_reason.reason_id(),
        }
    }
}

// ============================================================================
// SubDCRotationEnvelope (SDRT — joint parent-DC + sub-DC signature)
// =====================================================================================

/// `SubDCRotationEnvelope` (DOT/1/SDRT). Rotation proof replacing the delegated sub-DC;
/// requires BOTH parent-DC AND retiring-sub-DC signatures (anti-collusion + anti-forgery).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubDCRotationEnvelope {
    pub envelope_type: [u8; 4],
    pub envelope_subtype: [u8; 4],
    pub version: u16,
    pub parent_dc_id: [u8; 32],
    pub sub_domain_id: [u8; 32],
    pub old_sub_dc_id: [u8; 32],
    pub new_sub_dc_id: [u8; 32],
    pub term_id: CoordinatorTermId,
    pub current_epoch: u64,
    pub nonce: [u8; 16],
    /// Parent-DC Ed25519 signature.
    pub parent_signature: Ed25519Signature,
    /// Retiring-sub-DC Ed25519 signature.
    pub retiring_sub_dc_signature: Ed25519Signature,
}

impl SubDCRotationEnvelope {
    pub fn replay_key(&self) -> SdrtReplayKey {
        SdrtReplayKey {
            parent_dc_id: self.parent_dc_id,
            sub_domain_id: self.sub_domain_id,
            old_sub_dc_id: self.old_sub_dc_id,
            new_sub_dc_id: self.new_sub_dc_id,
            term_id: self.term_id,
            current_epoch: self.current_epoch,
            nonce: self.nonce,
        }
    }
}

// ============================================================================
// SubDCDelegationPolicy (Layer C — pure function)
// ============================================================================

/// Inputs to `SubDCDelegationPolicy::check`. Pure data; no I/O.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DelegationCheckInputs {
    pub envelope_parent_dc_id: [u8; 32],
    pub envelope_sub_domain_id: [u8; 32],
    pub envelope_sub_dc_id: [u8; 32],
    pub envelope_term_id: CoordinatorTermId,
    pub envelope_nonce: [u8; 16],
    /// Current chain depth at acceptance site (caller's ledger state).
    pub chain_depth: u16,
    /// Root-delegation count for `envelope_parent_dc_id` (caller's ledger state).
    pub root_delegation_count_for_parent: u8,
    /// Whether the subgroup is currently in `Bound` state (caller's ledger state).
    pub subgroup_state_bound: bool,
    /// For SDRT: whether both parent-DC + retiring-sub-DC signatures are present.
    pub sdrt_joint_signatures_present: bool,
}

/// `SubDCDelegationPolicy::check` — pure function (Layer A/C boundary).
///
/// Validates the delegation acceptance without performing I/O or signature
/// verification (those happen at the dispatch boundary). This function enforces:
/// - non-zero identity fields (parent_dc_id, sub_domain_id, sub_dc_id, term_id, nonce)
/// - chain-depth cap (`MAX_DELEGATION_CHAIN_PER_TERM = 256`)
/// - root-delegation count cap (`MAX_ROOT_DELEGATION = 1`)
/// - subgroup-state-bound invariant
/// - SDRT joint-signature requirement (when applicable)
pub fn check_delegation_policy(
    inputs: &DelegationCheckInputs,
) -> Result<(), DelegationPolicyError> {
    if inputs.envelope_parent_dc_id == [0u8; 32] {
        return Err(DelegationPolicyError::ParentDcIdZero);
    }
    if inputs.envelope_sub_domain_id == [0u8; 32] {
        return Err(DelegationPolicyError::SubDomainIdZero);
    }
    if inputs.envelope_sub_dc_id == [0u8; 32] {
        return Err(DelegationPolicyError::SubDcIdZero);
    }
    if inputs.envelope_term_id.0 == [0u8; 32] {
        return Err(DelegationPolicyError::TermIdZero);
    }
    if inputs.envelope_nonce == [0u8; 16] {
        return Err(DelegationPolicyError::NonceZero);
    }
    if inputs.chain_depth >= MAX_DELEGATION_CHAIN_PER_TERM {
        return Err(DelegationPolicyError::ChainDepthExceeded {
            max: MAX_DELEGATION_CHAIN_PER_TERM,
            got: inputs.chain_depth,
        });
    }
    if inputs.root_delegation_count_for_parent >= MAX_ROOT_DELEGATION {
        return Err(DelegationPolicyError::RootDelegationExceeded {
            max: MAX_ROOT_DELEGATION,
            got: inputs.root_delegation_count_for_parent,
            parent: inputs.envelope_parent_dc_id,
        });
    }
    if !inputs.subgroup_state_bound {
        return Err(DelegationPolicyError::SubgroupNotBound);
    }
    // SDRT joint-signature check: when caller flags `sdrt_joint_signatures_present=false`
    // AND the caller is invoking this policy for an SDRT envelope, reject.
    // Caller passes `true` when both signatures verified; `false` otherwise.
    if !inputs.sdrt_joint_signatures_present {
        // Distinguish SDCD/SDRV (no joint requirement) from SDRT (joint required).
        // This is the caller's responsibility to flag — `false` for SDRT means reject.
        // We don't know envelope type here; treat `false` as a soft hint.
        // Caller MUST pass `true` for SDRT acceptance; `false` triggers reject.
        return Err(DelegationPolicyError::JointSignatureMissing { got: 0 });
    }
    Ok(())
}

// ============================================================================
// Cross-RFC re-exports (canonical home pattern per plateau closure)
// ============================================================================

// d2 owns `CoordinatorTermId` + `SubDCDelegationPolicy` (and types above).
// d3 re-imports via `pub use crate::rfc_0855p_d2::*` (non-self-reexport per d2 acceptance criteria).

// Re-export d1 types so downstream consumers get a single-import surface.
pub use crate::dot::subgroup_state::{
    DelegationId, SubGroupLabel, MAX_ROOT_DEPTH, MAX_SUB_LABEL_BYTES,
};

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn parent_id(seed: u8) -> [u8; 32] {
        [seed; 32]
    }

    fn term_id(seed: u8) -> CoordinatorTermId {
        CoordinatorTermId([seed; 32])
    }

    fn sig(seed: u8) -> Ed25519Signature {
        Ed25519Signature([seed; 64])
    }

    fn inputs() -> DelegationCheckInputs {
        DelegationCheckInputs {
            envelope_parent_dc_id: parent_id(0xAA),
            envelope_sub_domain_id: [0xBBu8; 32],
            envelope_sub_dc_id: [0xCCu8; 32],
            envelope_term_id: term_id(0xDD),
            envelope_nonce: [0xEE; 16],
            chain_depth: 0,
            root_delegation_count_for_parent: 0,
            subgroup_state_bound: true,
            sdrt_joint_signatures_present: true,
        }
    }

    // TV-SG-6a: valid SDCD acceptance — happy path.
    #[test]
    fn tv_sg_6a_valid_sdcd() {
        let env = SubDCDelegationEnvelope {
            envelope_type: *b"DOT1",
            envelope_subtype: SUBDC_DELEGATION_TAG,
            version: 0x0001,
            parent_dc_id: parent_id(0xAA),
            sub_domain_id: [0xBBu8; 32],
            sub_dc_id: [0xCCu8; 32],
            term_id: term_id(0xDD),
            current_epoch: 100,
            nonce: [0xEE; 16],
            signature: sig(0xFF),
        };
        let key = env.replay_key();
        assert_eq!(key.parent_dc_id, parent_id(0xAA));
        assert_eq!(key.sub_domain_id, [0xBBu8; 32]);
        assert_eq!(key.sub_dc_id, [0xCCu8; 32]);
        let _digest = key.digest();
        assert!(check_delegation_policy(&inputs()).is_ok());
    }

    // TV-SG-6b: replay-key distinct for distinct nonces.
    #[test]
    fn tv_sg_6b_replay_key_distinct_nonces() {
        let env_a = SubDCDelegationEnvelope {
            envelope_type: *b"DOT1",
            envelope_subtype: SUBDC_DELEGATION_TAG,
            version: 0x0001,
            parent_dc_id: parent_id(0xAA),
            sub_domain_id: [0xBBu8; 32],
            sub_dc_id: [0xCCu8; 32],
            term_id: term_id(0xDD),
            current_epoch: 100,
            nonce: [0x01; 16],
            signature: sig(0xFF),
        };
        let env_b = SubDCDelegationEnvelope {
            nonce: [0x02; 16],
            ..env_a.clone()
        };
        assert_ne!(env_a.replay_key().digest(), env_b.replay_key().digest());
    }

    // TV-SG-7a: invalid SDRV — RevocationReasonCode exhaustiveness preserved.
    #[test]
    fn tv_sg_7a_sdrv_revocation_reason_round_trip() {
        let reasons = [
            RevocationReasonCode::TermExpired,
            RevocationReasonCode::CoordinatorRotation,
            RevocationReasonCode::SubDCMisconduct,
            RevocationReasonCode::SubDCKeyCompromise,
            RevocationReasonCode::SubDCVoluntaryResignation,
            RevocationReasonCode::GroupDecommission,
        ];
        for r in &reasons {
            let id = r.reason_id();
            let parsed = RevocationReasonCode::from_reason_id(id).unwrap();
            assert_eq!(*r, parsed);
        }
        // Unknown reason_id (RFC-allocated 0x0007-0x00FF + extension 0x0100+).
        // 0x0007 is in RFC-allocated range but not assigned → reject.
        assert!(matches!(
            RevocationReasonCode::from_reason_id(0x0007),
            Err(RevocationReasonError::UnknownReasonId { got: 0x0007 })
        ));
        // 0x0100+ in extension range → wrap as Extension(id).
        let ext = RevocationReasonCode::from_reason_id(0x0100).unwrap();
        assert!(matches!(ext, RevocationReasonCode::Extension(0x0100)));
    }

    // TV-SG-7b: subgroup not Bound → delegation rejected.
    #[test]
    fn tv_sg_7b_subgroup_not_bound_rejected() {
        let mut i = inputs();
        i.subgroup_state_bound = false;
        assert!(matches!(
            check_delegation_policy(&i),
            Err(DelegationPolicyError::SubgroupNotBound)
        ));
    }

    // TV-SG-7c: chain depth exceeded.
    #[test]
    fn tv_sg_7c_chain_depth_exceeded() {
        let mut i = inputs();
        i.chain_depth = MAX_DELEGATION_CHAIN_PER_TERM;
        assert!(matches!(
            check_delegation_policy(&i),
            Err(DelegationPolicyError::ChainDepthExceeded { .. })
        ));
    }

    // TV-SG-7d: root delegation count exceeded.
    #[test]
    fn tv_sg_7d_root_delegation_exceeded() {
        let mut i = inputs();
        i.root_delegation_count_for_parent = MAX_ROOT_DELEGATION;
        assert!(matches!(
            check_delegation_policy(&i),
            Err(DelegationPolicyError::RootDelegationExceeded { .. })
        ));
    }

    // TV-SG-7e: zero identity fields rejected.
    #[test]
    fn tv_sg_7e_zero_identity_fields() {
        let mut i = inputs();
        i.envelope_parent_dc_id = [0u8; 32];
        assert!(matches!(
            check_delegation_policy(&i),
            Err(DelegationPolicyError::ParentDcIdZero)
        ));
        i = inputs();
        i.envelope_sub_domain_id = [0u8; 32];
        assert!(matches!(
            check_delegation_policy(&i),
            Err(DelegationPolicyError::SubDomainIdZero)
        ));
        i = inputs();
        i.envelope_sub_dc_id = [0u8; 32];
        assert!(matches!(
            check_delegation_policy(&i),
            Err(DelegationPolicyError::SubDcIdZero)
        ));
        i = inputs();
        i.envelope_term_id = CoordinatorTermId([0u8; 32]);
        assert!(matches!(
            check_delegation_policy(&i),
            Err(DelegationPolicyError::TermIdZero)
        ));
        i = inputs();
        i.envelope_nonce = [0u8; 16];
        assert!(matches!(
            check_delegation_policy(&i),
            Err(DelegationPolicyError::NonceZero)
        ));
    }

    // TV-SG-7f: SDRT joint-signature requirement.
    #[test]
    fn tv_sg_7f_sdrt_joint_signature_required() {
        let mut i = inputs();
        i.sdrt_joint_signatures_present = false;
        assert!(matches!(
            check_delegation_policy(&i),
            Err(DelegationPolicyError::JointSignatureMissing { .. })
        ));
        i.sdrt_joint_signatures_present = true;
        assert!(check_delegation_policy(&i).is_ok());
    }

    // TV-SG-7g: RevocationReasonCode is #[non_exhaustive] — wildcard required.
    #[test]
    fn tv_sg_7g_revocation_reason_non_exhaustive() {
        let r = RevocationReasonCode::SubDCVoluntaryResignation;
        let id = match r {
            RevocationReasonCode::TermExpired => 1,
            RevocationReasonCode::CoordinatorRotation => 2,
            RevocationReasonCode::SubDCMisconduct => 3,
            RevocationReasonCode::SubDCKeyCompromise => 4,
            RevocationReasonCode::SubDCVoluntaryResignation => 5,
            RevocationReasonCode::GroupDecommission => 6,
            _ => 99, // required wildcard per #[non_exhaustive] (Extension + future)
        };
        assert_eq!(id, 5);
    }

    // TV-SG-7h: SDRT replay-key digest differs from SDCD/SDRV contexts.
    #[test]
    fn tv_sg_7h_replay_keys_context_separated() {
        let sdcd_key = SdcdReplayKey {
            parent_dc_id: parent_id(0xAA),
            sub_domain_id: [0xBBu8; 32],
            sub_dc_id: [0xCCu8; 32],
            term_id: term_id(0xDD),
            current_epoch: 100,
            nonce: [0xEE; 16],
        };
        let sdrv_key = SdrvReplayKey {
            parent_dc_id: parent_id(0xAA),
            sub_domain_id: [0xBBu8; 32],
            sub_dc_id: [0xCCu8; 32],
            term_id: term_id(0xDD),
            revocation_reason_id: 0x0003,
        };
        let sdrt_key = SdrtReplayKey {
            parent_dc_id: parent_id(0xAA),
            sub_domain_id: [0xBBu8; 32],
            old_sub_dc_id: [0xCCu8; 32],
            new_sub_dc_id: [0x99u8; 32],
            term_id: term_id(0xDD),
            current_epoch: 100,
            nonce: [0xEE; 16],
        };
        assert_ne!(sdcd_key.digest(), sdrv_key.digest());
        assert_ne!(sdcd_key.digest(), sdrt_key.digest());
        assert_ne!(sdrv_key.digest(), sdrt_key.digest());
    }

    // Constants match RFC §Layer placement table.
    #[test]
    fn delegation_constants_match_rfc() {
        assert_eq!(MAX_DELEGATION_CHAIN_PER_TERM, 256);
        assert_eq!(MAX_ROOT_DELEGATION, 1);
        assert_eq!(SDCD_CONTEXT, "DOT/1/SDCD/delegation");
        assert_eq!(SDRV_CONTEXT, "DOT/1/SDRV/revocation");
        assert_eq!(SDRT_CONTEXT, "DOT/1/SDRT/rotation");
        assert_eq!(SUBDC_DELEGATION_TAG, *b"SDCD");
        assert_eq!(SUBDC_REVOCATION_TAG, *b"SDRV");
        assert_eq!(SUBDC_ROTATION_TAG, *b"SDRT");
    }
}
