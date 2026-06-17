//! Governance rotation (mission 0855p-b-governance-rfc).
//!
//! Defines:
//! - `GOVERNANCE_ROTATION` envelope type for key rotation on
//!   governance key compromise.
//! - `0x000E` slash reason code for governance key compromise.
//! - `GOVERNANCE_MIGRATION_WINDOW = 100` epoch migration window.
//! - 5-of-7 recovery multi-sig (configurable threshold).
//!
//! ## Status
//!
//! The full RFC-0855p-d "Governance Lifecycle" document is a
//! follow-up; this module ships the types and validation logic
//! per the mission text. The DKG ceremony is documented in the
//! mission's operator guide and is not implemented in code
//! (DKG requires an interactive multi-party protocol).

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

/// Migration window in epochs (per mission spec). At 1-min
/// epochs, 100 minutes.
pub const GOVERNANCE_MIGRATION_WINDOW: u64 = 100;

/// Recovery multi-sig: 5-of-7 (per mission spec).
pub const RECOVERY_THRESHOLD: usize = 5;
pub const RECOVERY_TOTAL: usize = 7;

/// 0x000E slash reason code (mission 0855p-b-governance-rfc).
pub const SLASH_REASON_GOVERNANCE_KEY_COMPROMISE: u16 = 0x000E;

/// A `GOVERNANCE_ROTATION` envelope.
///
/// The 3-of-5 governance multi-sig signs this envelope to
/// announce a new `governance_id` (typically after a key
/// compromise). The old `governance_id` remains valid for
/// historical slashing (immutability) but is invalid for new
/// slash votes after `effective_epoch`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GovernanceRotation {
    /// The new governance_id (32-byte).
    pub new_governance_id: [u8; 32],
    /// The old governance_id (32-byte). Remains valid for
    /// historical slashing only.
    pub old_governance_id: [u8; 32],
    /// Evidence of the key compromise (typically a slash vote
    /// with reason `0x000E`).
    pub evidence: Vec<u8>,
    /// The epoch when the new governance_id takes effect.
    pub effective_epoch: u64,
    /// The signatures of the recovery multi-sig (5-of-7).
    pub signatures: Vec<Vec<u8>>,
    /// Unix epoch seconds when the rotation was signed.
    pub signed_at_epoch: u64,
}

impl GovernanceRotation {
    /// Returns true if the rotation has enough signatures
    /// (5-of-7 recovery multi-sig).
    pub fn has_quorum(&self) -> bool {
        self.signatures.len() >= RECOVERY_THRESHOLD
    }

    /// Returns the deadline epoch by which missions must
    /// migrate: `effective_epoch + GOVERNANCE_MIGRATION_WINDOW`.
    pub fn migration_deadline(&self) -> u64 {
        self.effective_epoch + GOVERNANCE_MIGRATION_WINDOW
    }

    /// Returns true if the rotation is still in the migration
    /// window (matters for slashing validations).
    pub fn in_migration_window(&self, current_epoch: u64) -> bool {
        current_epoch >= self.effective_epoch
            && current_epoch <= self.migration_deadline()
    }
}

/// A slash vote with a `governance_id`. Used to enforce that
/// votes with old `governance_id` are rejected after the
/// rotation's `effective_epoch`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GovernanceScopedVote {
    pub governance_id: [u8; 32],
    pub payload_hash: [u8; 32],
    pub vote_epoch: u64,
    pub signature: Vec<u8>,
}

/// Validate that a slash vote's `governance_id` is current.
///
/// Returns `Ok(())` if `vote.governance_id == active_governance_id`.
///
/// Returns `Err(OldGovernanceId)` if the vote uses an old
/// `governance_id` after the rotation's `effective_epoch`.
/// Returns `Err(MigrationExpired)` if the migration window has
/// elapsed and the old id is being used.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GovernanceValidationError {
    /// The vote's `governance_id` is the old (deprecated) one
    /// and we're past the effective epoch but within the
    /// migration window. Still valid for historical slashing
    /// only — but a NEW slash vote with the old id is
    /// invalid.
    OldGovernanceId,
    /// The migration window has elapsed and the old id is
    /// still being used. The mission is suspended.
    MigrationExpired,
    /// The signature is invalid.
    BadSignature,
}

pub fn validate_governance_id(
    vote: &GovernanceScopedVote,
    rotation: Option<&GovernanceRotation>,
    current_epoch: u64,
    active_governance_id: &[u8; 32],
) -> Result<(), GovernanceValidationError> {
    if &vote.governance_id == active_governance_id {
        return Ok(());
    }
    let rotation = match rotation {
        Some(r) if r.old_governance_id == vote.governance_id => r,
        _ => return Err(GovernanceValidationError::BadSignature),
    };
    if current_epoch < rotation.effective_epoch {
        // Old id is still valid before the rotation takes
        // effect.
        return Ok(());
    }
    if current_epoch <= rotation.migration_deadline() {
        // Within the migration window. The vote is rejected
        // for new slashing (the new id is required).
        return Err(GovernanceValidationError::OldGovernanceId);
    }
    Err(GovernanceValidationError::MigrationExpired)
}

/// Recovery multi-sig signer set (5-of-7).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RecoveryMultisig {
    pub signers: BTreeSet<String>,
}

impl RecoveryMultisig {
    /// Create a recovery multi-sig from a set of signers.
    /// Returns None if the set has < 5 signers.
    pub fn new(signers: BTreeSet<String>) -> Option<Self> {
        if signers.len() < RECOVERY_THRESHOLD {
            return None;
        }
        Some(Self { signers })
    }

    /// Returns the threshold (5).
    pub fn threshold(&self) -> usize {
        RECOVERY_THRESHOLD
    }

    /// Returns the total number of signers (typically 7).
    pub fn total(&self) -> usize {
        self.signers.len()
    }

    /// Returns true if the provided set of signers has at least
    /// the threshold.
    pub fn meets_threshold(&self, signed_by: &BTreeSet<String>) -> bool {
        // Count the intersection.
        signed_by.intersection(&self.signers).count() >= RECOVERY_THRESHOLD
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotation_migration_deadline() {
        let r = GovernanceRotation {
            new_governance_id: [1; 32],
            old_governance_id: [0; 32],
            evidence: vec![],
            effective_epoch: 1000,
            signatures: vec![],
            signed_at_epoch: 999,
        };
        assert_eq!(r.migration_deadline(), 1100);
    }

    #[test]
    fn in_migration_window() {
        let r = GovernanceRotation {
            new_governance_id: [1; 32],
            old_governance_id: [0; 32],
            evidence: vec![],
            effective_epoch: 1000,
            signatures: vec![],
            signed_at_epoch: 999,
        };
        assert!(!r.in_migration_window(999)); // before effective
        assert!(r.in_migration_window(1000)); // at effective
        assert!(r.in_migration_window(1100)); // at deadline
        assert!(!r.in_migration_window(1101)); // past deadline
    }

    #[test]
    fn quorum_with_5_signatures() {
        let r = GovernanceRotation {
            new_governance_id: [1; 32],
            old_governance_id: [0; 32],
            evidence: vec![],
            effective_epoch: 1000,
            signatures: vec![vec![0]; 5],
            signed_at_epoch: 999,
        };
        assert!(r.has_quorum());
    }

    #[test]
    fn no_quorum_with_4_signatures() {
        let r = GovernanceRotation {
            new_governance_id: [1; 32],
            old_governance_id: [0; 32],
            evidence: vec![],
            effective_epoch: 1000,
            signatures: vec![vec![0]; 4],
            signed_at_epoch: 999,
        };
        assert!(!r.has_quorum());
    }

    #[test]
    fn validate_active_id_always_ok() {
        let vote = GovernanceScopedVote {
            governance_id: [1; 32],
            payload_hash: [0; 32],
            vote_epoch: 2000,
            signature: vec![],
        };
        let result = validate_governance_id(&vote, None, 2000, &[1; 32]);
        assert!(result.is_ok());
    }

    #[test]
    fn validate_old_id_before_effective_ok() {
        let vote = GovernanceScopedVote {
            governance_id: [0; 32],
            payload_hash: [0; 32],
            vote_epoch: 999,
            signature: vec![],
        };
        let r = GovernanceRotation {
            new_governance_id: [1; 32],
            old_governance_id: [0; 32],
            evidence: vec![],
            effective_epoch: 1000,
            signatures: vec![],
            signed_at_epoch: 999,
        };
        let result = validate_governance_id(&vote, Some(&r), 999, &[1; 32]);
        assert!(result.is_ok());
    }

    #[test]
    fn validate_old_id_during_migration_rejected() {
        let vote = GovernanceScopedVote {
            governance_id: [0; 32],
            payload_hash: [0; 32],
            vote_epoch: 1050,
            signature: vec![],
        };
        let r = GovernanceRotation {
            new_governance_id: [1; 32],
            old_governance_id: [0; 32],
            evidence: vec![],
            effective_epoch: 1000,
            signatures: vec![],
            signed_at_epoch: 999,
        };
        let result = validate_governance_id(&vote, Some(&r), 1050, &[1; 32]);
        assert_eq!(result, Err(GovernanceValidationError::OldGovernanceId));
    }

    #[test]
    fn validate_old_id_after_migration_rejected() {
        let vote = GovernanceScopedVote {
            governance_id: [0; 32],
            payload_hash: [0; 32],
            vote_epoch: 1200,
            signature: vec![],
        };
        let r = GovernanceRotation {
            new_governance_id: [1; 32],
            old_governance_id: [0; 32],
            evidence: vec![],
            effective_epoch: 1000,
            signatures: vec![],
            signed_at_epoch: 999,
        };
        let result = validate_governance_id(&vote, Some(&r), 1200, &[1; 32]);
        assert_eq!(result, Err(GovernanceValidationError::MigrationExpired));
    }

    #[test]
    fn recovery_multisig_threshold() {
        let signers: BTreeSet<String> = (0..7).map(|i| format!("s{i}")).collect();
        let m = RecoveryMultisig::new(signers.clone()).unwrap();
        assert_eq!(m.threshold(), 5);
        assert_eq!(m.total(), 7);
        let signed: BTreeSet<String> = (0..5).map(|i| format!("s{i}")).collect();
        assert!(m.meets_threshold(&signed));
    }

    #[test]
    fn recovery_multisig_rejects_below_threshold() {
        let signers: BTreeSet<String> = (0..7).map(|i| format!("s{i}")).collect();
        let m = RecoveryMultisig::new(signers).unwrap();
        let signed: BTreeSet<String> = (0..4).map(|i| format!("s{i}")).collect();
        assert!(!m.meets_threshold(&signed));
    }

    #[test]
    fn recovery_multisig_rejects_too_few_signers() {
        let signers: BTreeSet<String> = (0..4).map(|i| format!("s{i}")).collect();
        assert!(RecoveryMultisig::new(signers).is_none());
    }

    #[test]
    fn governance_rotation_serde_roundtrip() {
        let r = GovernanceRotation {
            new_governance_id: [1; 32],
            old_governance_id: [0; 32],
            evidence: vec![1, 2, 3],
            effective_epoch: 1000,
            signatures: vec![vec![0xAA]],
            signed_at_epoch: 999,
        };
        let json = serde_json::to_string(&r).unwrap();
        let back: GovernanceRotation = serde_json::from_str(&json).unwrap();
        assert_eq!(back, r);
    }
}
