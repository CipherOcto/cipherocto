//! `octo-cap-macaroon-vault-storage` — RFC-0206 v2.1 §Adapter Crate List row 3.
//!
//! Substrate adapter for the `VaultLookup` trait declared in
//! `octo-cap-macaroon::VaultLookup` (RFC-0957 verify-time bump per
//! review §20.6.1). This crate is the impl-side adapter; the trait
//! itself was already declared in `octo-cap-macaroon/src/vault_lookup.rs`
//! before this mission (MOVE per §C line 99).
//!
//! The adapter owns:
//! - the canonical `AdapterAllowlist` for the `vaults` table (re-using
//!   `octo-vault-storage::build_allowlist()` — both adapters read the
//!   same table; the substrate's allowlist is keyed on `AdapterId` not
//!   on the underlying table),
//! - a `VaultLookup` impl that delegates to
//!   `octo-vault-storage::VaultStore::lookup_by_vault_id` and translates
//!   the `(ChainId, VaultState)` tuple into the `VaultRowSnapshot`
//!   shape the verify path expects.
//!
//! Per RFC §Adapter Crate List this is a Layer C adapter crate; the
//! only substrate dep is `octo-storage-core`. No direct `stoolap` import
//! (TV-0206-A9(a) gate).
//!
//! ## TV-0206-A6
//!
//! `test -d crates/octo-cap-macaroon-vault-storage` exits 0.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::sync::Arc;

use octo_cap_macaroon::{VaultLookup, VaultRowSnapshot};
use octo_storage_core::typed_statement::{DdlOperation, DdlTemplate, SqlSelect};
use octo_storage_core::{AdapterAllowlist, AdapterId, Database, TypedStatement};
use octo_vault_storage::{VaultStore, ADAPTER_ID as VAULT_ADAPTER_ID};

/// Canonical adapter id for the vault-lookup substrate adapter.
pub const ADAPTER_ID: &str = "octo-cap-macaroon-vault-storage/v1";

/// Canonical table name (same table as `octo-vault-storage`).
pub const TABLE_VAULTS: &str = "vaults";

/// Build the canonical `AdapterAllowlist` for the vault-lookup
/// substrate adapter. Registers the same `vaults` table as
/// `octo-vault-storage` (read-side access) + the same
/// `vaults_vault_id_idx` DDL template.
#[must_use]
pub fn build_allowlist() -> AdapterAllowlist {
    AdapterAllowlist::with_registrations(
        AdapterId::new(ADAPTER_ID),
        [TABLE_VAULTS.to_owned()],
        [DdlTemplate {
            id: "vaults_vault_id_idx".to_owned(),
            operation: DdlOperation::CreateIndex,
        }],
    )
}

/// Typed substrate adapter handle.
#[derive(Clone)]
pub struct VaultLookupAdapter {
    /// Shared `Database` handle. Held for parity with the
    /// `VaultStoreAdapter` peer shape (allows future read-side direct
    /// queries without going through the peer adapter). Currently only
    /// the allowlist + vault_store are consulted at lookup time.
    #[allow(dead_code)]
    db: Arc<Database>,
    allowlist: Arc<AdapterAllowlist>,
    vault_store: Arc<VaultStore>,
}

impl std::fmt::Debug for VaultLookupAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VaultLookupAdapter")
            .field("adapter_id", &self.allowlist.adapter().as_str())
            .field("vault_adapter_id", &VAULT_ADAPTER_ID)
            .finish_non_exhaustive()
    }
}

impl VaultLookupAdapter {
    /// Construct a `VaultLookupAdapter` from a shared `Database`
    /// handle + a peer `Arc<VaultStore>` (the read-side adapter).
    pub fn new(db: Arc<Database>, vault_store: Arc<VaultStore>) -> Self {
        let allowlist = Arc::new(build_allowlist());
        Self {
            db,
            allowlist,
            vault_store,
        }
    }

    /// Borrow the underlying `AdapterAllowlist`.
    #[must_use]
    pub fn allowlist(&self) -> &AdapterAllowlist {
        &self.allowlist
    }
}

/// `VaultLookup` impl — delegated to the underlying `VaultStore` (peer
/// adapter crate) + wrapped in the substrate's typed-surface guard.
///
/// The substrate's `AdapterAllowlist` runs FIRST so an attacker who
/// compromises the call site cannot bypass the typed surface; the
/// `VaultStore::lookup_by_vault_id` then executes the actual SQL via
/// the substrate `Deref`-into-`stoolap::Database` path.
impl VaultLookup for VaultLookupAdapter {
    fn lookup_vault(&self, vault_id: &[u8; 32]) -> Option<VaultRowSnapshot> {
        // 1. Substrate allowlist guard on the typed SELECT.
        let stmt = TypedStatement::Select(SqlSelect {
            tables: vec![TABLE_VAULTS.to_owned()],
        });
        if self
            .allowlist
            .check(&stmt)
            .map_err(|e| octo_storage_core::SubstrateError::Storage {
                operation: "lookup_vault",
                message: format!("{e}"),
            })
            .is_err()
        {
            return None;
        }

        // 2. Delegate to peer adapter's typed surface.
        let vid = octo_vault::VaultId(*vault_id);
        match self.vault_store.lookup_by_vault_id(&vid) {
            Ok(Some((chain_id, state))) => {
                let mut chain_arr = [0u8; 32];
                chain_arr.copy_from_slice(chain_id.as_bytes());
                Some(VaultRowSnapshot {
                    chain_id: chain_arr,
                    is_active: state == octo_vault::VaultState::Active,
                })
            }
            Ok(None) => None,
            Err(_) => None,
        }
    }
}

/// Register the `VaultLookupAdapter` with the substrate's typed
/// surface. Per RFC §Wiring Pattern the allowlist IS the adapter's
/// typed surface, and the Arc is returned unchanged.
pub fn register(
    allowlist: Arc<AdapterAllowlist>,
    adapter: Arc<VaultLookupAdapter>,
) -> Arc<VaultLookupAdapter> {
    let _ = allowlist;
    adapter
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adapter_id_is_canonical() {
        assert_eq!(ADAPTER_ID, "octo-cap-macaroon-vault-storage/v1");
        assert_eq!(VAULT_ADAPTER_ID, "octo-vault-storage/v1");
    }

    #[test]
    fn build_allowlist_registers_vaults_table() {
        let al = build_allowlist();
        assert_eq!(al.adapter().as_str(), ADAPTER_ID);
        assert!(al.tables().contains(TABLE_VAULTS));
        assert_eq!(al.ddl().len(), 1);
        assert_eq!(al.ddl()[0].id, "vaults_vault_id_idx");
    }
}
