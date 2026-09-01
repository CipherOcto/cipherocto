//! `VaultOwnerIndex` port trait + `list_owned` substrate entry point
//! (RFC-0011-e §Substrate Additions).
//!
//! Split from `vault_operations.rs` per Wave 1.5 hygiene callout.
//!
//! Per [[cipherocto-design-principles]] "Storage is not a protocol",
//! the substrate here only declares the port trait. Production impl
//! lives at `octo-vault-stoolap` (Layer D transport adapter).

use thiserror::Error;

use crate::vault_summary::VaultSummary;

/// Vault operations errors (RFC-0011-e §Substrate Additions).
///
/// Substrate-internal error type for the `vault {list,balance,transfer}`
/// substrate calls. The CLI surfaces these as `OctoCliError::Substrate`
/// (RFC-0011 §Error Handling); operator-facing messages MUST go through
/// the CLI's redactor.
#[derive(Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum VaultError {
    /// Substrate-level failure (Stoolap / migration runner / port
    /// wiring).
    #[error(
        "vault substrate error during vault operations; \
         substrate-internal trace preserved in substrate logs only"
    )]
    Substrate,
    /// Owner DID not registered in the substrate owner index port.
    #[error("owner did not found in vault owner index")]
    OwnerNotFound,
    /// Underlying port returned an error.
    #[error("vault owner index port error: {0}")]
    Port(String),
}

/// `VaultOwnerIndex` port trait (RFC-0011-e §Substrate Additions — the
/// substrate port backing `list_owned`).
///
/// Production impl lives at `octo-vault-stoolap` (Layer D transport
/// adapter) and reads `vaults` by `(chain_id, owner_did)` filter per
/// §20.3 schema sketch. The substrate here only declares the port
/// shape; the read path lands in the Layer D adapter.
pub trait VaultOwnerIndex: Send + Sync {
    /// Return every `VaultSummary` row owned by `owner_did`. The
    /// substrate port is owner-scoped (cross-chain rows for the same
    /// owner surface in the same result vector); the CLI applies
    /// `--chain-id` filtering at the envelope layer.
    fn vaults_for_owner(&self, owner_did: &str) -> Result<Vec<VaultSummary>, VaultError>;
}

/// List every vault owned by `owner_did` (RFC-0011-e §Substrate
/// Additions).
///
/// Substrate port binding: reads via the supplied `index` (Layer D
/// adapter impl). The CLI wraps this call into the
/// `VaultListOutput` envelope (parent `schema_version = 3`).
///
/// # Layer direction
///
/// `list_owned` is a Layer B substrate entry point. It depends on:
///
/// - [`VaultOwnerIndex`] (Layer B port trait — this module).
/// - [`OwnerDid`] (substrate `String` alias; see module-level
///   "Substrate-truth deviation note" in `vault_operations.rs`).
///
/// It does NOT touch the storage backend directly; the Layer D adapter
/// (e.g. `octo-vault-stoolap`) implements `VaultOwnerIndex` against the
/// Stoolap-fork `vaults` table.
pub fn list_owned(
    index: &dyn VaultOwnerIndex,
    owner_did: &str,
) -> Result<Vec<VaultSummary>, VaultError> {
    index.vaults_for_owner(owner_did)
}
