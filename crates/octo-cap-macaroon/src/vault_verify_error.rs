//! [`VaultVerifyError`] — verify-time errors specific to the
//! `Caveat::Vault(vault_id)` binding path.
//!
//! Distinct from `MacaroonError` so the verify path can return
//! vault-specific errors with caller-controlled granularity (a caller
//! verifying a `VaultOperation` matches on the `VaultVerifyError`
//! variants directly without traversing `MacaroonError` noise).
//!
//! Per review §20.6.1 algorithm steps 2-4:
//!
//! ```text
//! 2. vault_row = vaults.vaults_vault_id_idx.get(vault_id)?  // <- VaultRowMissing
//! 3. Assert: vault_row.chain_id == op.target_chain_id          // <- ChainMismatch
//! 4. Assert: vault_row.state == Active                         // <- VaultNotActive
//! ```
//!
//! Plus §20.6.1 line 1328 (`WrappedOnly` chainless-parent rule) which
//! surfaces as [`VaultVerifyError::WrappedChainHasNoVault`] — NOTE:
//! its primary surface is `MacaroonError::WrappedChainHasNoVault`
//! (added in `macaroon.rs`); this enum carries it for the
//! `verify_for_vault_op` path's standalone returns.

#![allow(missing_docs)]

use thiserror::Error;

use crate::macaroon::MacaroonError;

/// Errors raised by [`crate::macaroon::Macaroon::verify_for_vault_op`]
/// and the [`crate::vault_lookup::VaultLookup`] adapter path.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum VaultVerifyError {
    /// Review §20.6.1 step 2 — vault row not found (look up returned `None`).
    #[error("vault row missing for vault_id {vault_id:?}")]
    VaultRowMissing {
        /// The opaque 32-byte vault identifier from `Caveat::Vault(vault_id)`.
        vault_id: [u8; 32],
    },

    /// Review §20.6.1 step 3 — vault row's `chain_id` does not match the
    /// operation's target chain.
    #[error("vault chain {vault_chain:?} does not match op chain {op_chain:?}")]
    ChainMismatch {
        /// Chain id from the looked-up vault row.
        vault_chain: [u8; 32],
        /// Chain id from the verification operation context.
        op_chain: [u8; 32],
    },

    /// Review §20.6.1 step 4 — vault row's state is not `Active`.
    /// `Frozen` and any future non-Active state map here.
    #[error("vault {vault_id:?} state is not Active (verify-time invariant rejects)")]
    VaultNotActive {
        /// The opaque 32-byte vault identifier.
        vault_id: [u8; 32],
    },

    /// Review §20.6.1 line 1328 — `WrappedOnly` chain has no
    /// `Caveat::Vault` in any ancestor. Per §20.6.1: "If the parent has
    /// NO `Caveat::Vault` ... the wrapped capability is treated as
    /// chainless — verifier rejects any `WrappedOnly` descendant that
    /// targets a chain (no parent chain to inherit). This is the safe
    /// default; chains must be explicit."
    #[error("WrappedOnly chain has no Caveat::Vault ancestor (chainless — safe default rejects)")]
    WrappedChainHasNoVault,

    /// Wraps any non-vault-specific `MacaroonError` raised during the
    /// `verify_for_vault_op` pipeline (chain mismatch, root-secret
    /// mismatch, wrapped-cycle, wrapped-depth-exceeded,
    /// wrapped-parent-not-found, attenuation-violation, capability-id
    /// mismatch). Distinct variant so callers can pattern-match
    /// vault-specific errors without traversing macaroon-crate noise.
    #[error("macaroon error during vault-op verification: {0}")]
    Macaroon(MacaroonError),
}

impl From<MacaroonError> for VaultVerifyError {
    fn from(e: MacaroonError) -> Self {
        Self::Macaroon(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vault_row_missing_carries_vault_id() {
        let e = VaultVerifyError::VaultRowMissing {
            vault_id: [0xAB; 32],
        };
        assert_eq!(
            e,
            VaultVerifyError::VaultRowMissing {
                vault_id: [0xAB; 32]
            }
        );
        let formatted = format!("{e}");
        assert!(formatted.contains("vault_id"));
    }

    #[test]
    fn chain_mismatch_carries_both_chains() {
        let e = VaultVerifyError::ChainMismatch {
            vault_chain: [0x01; 32],
            op_chain: [0x02; 32],
        };
        let formatted = format!("{e}");
        assert!(formatted.contains("chain"));
    }

    #[test]
    fn vault_not_active_carries_vault_id() {
        let e = VaultVerifyError::VaultNotActive {
            vault_id: [0xFF; 32],
        };
        assert_eq!(
            e,
            VaultVerifyError::VaultNotActive {
                vault_id: [0xFF; 32]
            }
        );
    }

    #[test]
    fn wrapped_chain_has_no_vault_carries_no_payload() {
        let e = VaultVerifyError::WrappedChainHasNoVault;
        assert_eq!(e, VaultVerifyError::WrappedChainHasNoVault);
        let formatted = format!("{e}");
        assert!(formatted.contains("WrappedOnly"));
    }
}
