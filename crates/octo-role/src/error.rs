//! Substrate error type for role provisioning per RFC-0011-d §Error Handling.
//!
//! Layer B typed substrate error. CLI translates these to `OctoCliError`
//! variants (M7) — unidirectional mapping per
//! [[cipherocto-design-principles]] no-premature-coupling.
//!
//! Per RFC-0011-d §Security 2: there is **NO** `RoleBindingConflict`
//! variant. Re-selecting with the same `(operator_did, role_id, chain_id)`
//! triggers last-writer-wins atomic UPDATE; the substrate never surfaces
//! a conflict error.

use thiserror::Error;

/// Substrate error for `octo_role::*` entrypoints.
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum RoleError {
    /// `octo_role::show` / `select` with missing `role_id`.
    #[error("role not found: {role_id}")]
    RoleNotFound {
        /// The role slug that was not found in the registry.
        role_id: String,
    },
    /// `select` with stake < `requires_octo_min`.
    #[error("stake insufficient: required {required}, available {available}")]
    StakeInsufficient {
        /// Minimum OCTO stake the role requires (micro-OCTO).
        required: u64,
        /// Currently staked amount (micro-OCTO).
        available: u64,
    },
    /// `select` rejected — auditor mode, Phase 2 prereq guard, ambiguous
    /// sub-group, etc.
    #[error("role not selectable: {role_id} ({reason})")]
    RoleNotSelectable {
        /// The role slug the operator attempted to select.
        role_id: String,
        /// Why the role cannot be selected at this time.
        reason: String,
    },
    /// `select` with `signer.did()` ≠ `operator_did`.
    ///
    /// Per RFC-0011-d F-16 reserved slot — exits 35 in the CLI mapping.
    #[error("signer mismatch: signer_did {signer_did} != operator_did {operator_did}")]
    SignerMismatch {
        /// DID reported by the capability signer.
        signer_did: String,
        /// Slash-ledger PK DID expected by the binding.
        operator_did: String,
    },
}

impl RoleError {
    /// Returns `true` if this error is recoverable by re-selecting the
    /// same role (last-writer-wins overwrites the binding).
    ///
    /// Per RFC-0011-d §Security 2 the substrate never returns a "conflict"
    /// variant — it either commits an atomic UPDATE or rolls back the
    /// transaction.
    pub fn is_recoverable(&self) -> bool {
        match self {
            Self::RoleNotFound { .. } => false,
            Self::StakeInsufficient { .. } => true,
            Self::RoleNotSelectable { .. } => true,
            Self::SignerMismatch { .. } => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_error_4_variants_only() {
        // Per RFC-0011-d §Mission Decomposition M7 row: 4 CLI variants.
        // Substrate mirrors this exactly; no RoleBindingConflict.
        let all: Vec<RoleError> = vec![
            RoleError::RoleNotFound {
                role_id: "x".into(),
            },
            RoleError::StakeInsufficient {
                required: 100,
                available: 50,
            },
            RoleError::RoleNotSelectable {
                role_id: "x".into(),
                reason: "auditor mode".into(),
            },
            RoleError::SignerMismatch {
                signer_did: "did:octo:a".into(),
                operator_did: "did:octo:b".into(),
            },
        ];
        assert_eq!(all.len(), 4);
    }

    #[test]
    fn recoverable_flags_correct() {
        let stake = RoleError::StakeInsufficient {
            required: 100,
            available: 50,
        };
        assert!(stake.is_recoverable());

        let signer = RoleError::SignerMismatch {
            signer_did: "did:octo:a".into(),
            operator_did: "did:octo:b".into(),
        };
        assert!(!signer.is_recoverable());
    }
}