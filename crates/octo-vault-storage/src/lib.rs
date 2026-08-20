//! `octo-vault-storage` — RFC-0206 v2.1 §Adapter Crate List row 1.
//!
//! Substrate adapter for the `octo-vault` owner crate. This crate:
//!
//! - declares the canonical `AdapterId` + `AdapterAllowlist` for the
//!   `vaults` table + the `vaults_vault_id_idx` UNIQUE INDEX DDL
//!   template,
//! - wraps `octo_vault::VaultSubstrate` in a typed adapter surface that
//!   routes every DDL through `Database::execute_checked` so the
//!   substrate's `AdapterAllowlist` is the load-bearing guard (per
//!   RFC §Format Bypass Defense),
//! - exposes a thin `VaultStore` struct that downstream crates
//!   (`octo-cap-macaroon-vault`) can `Arc::clone` and inject into the
//!   `VaultLookup` verify path.
//!
//! Per RFC §Adapter Crate List this is a Layer C adapter crate; the
//! only substrate dep is `octo-storage-core`. No direct `stoolap` import
//! (TV-0206-A9(a) gate).
//!
//! ## TV-0206-A6
//!
//! `test -d crates/octo-vault-storage` exits 0. Per RFC §Test Vectors.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::sync::Arc;

use octo_storage_core::typed_statement::{DdlOperation, DdlTemplate, SqlInsert, SqlSelect};
use octo_storage_core::{AdapterAllowlist, AdapterId, Database, TypedStatement};
use octo_vault::{ChainId, VaultId, VaultState, VaultSubstrate};

/// Canonical adapter id for the vault substrate adapter.
pub const ADAPTER_ID: &str = "octo-vault-storage/v1";

/// Canonical table name for the vault substrate adapter.
pub const TABLE_VAULTS: &str = "vaults";

/// Canonical UNIQUE INDEX DDL template id (matches the
/// `vaults_vault_id_idx` index created by `octo-vault` migration `v001`).
pub const DDL_VAULTS_VAULT_ID_IDX: &str = "vaults_vault_id_idx";

/// Build the canonical `AdapterAllowlist` for the vault substrate
/// adapter: registers `vaults` table + the `vaults_vault_id_idx` DDL
/// template. Returned by [`register`].
#[must_use]
pub fn build_allowlist() -> AdapterAllowlist {
    AdapterAllowlist::with_registrations(
        AdapterId::new(ADAPTER_ID),
        [TABLE_VAULTS.to_owned()],
        [DdlTemplate {
            id: DDL_VAULTS_VAULT_ID_IDX.to_owned(),
            operation: DdlOperation::CreateIndex,
        }],
    )
}

/// Typed substrate adapter handle. Cloning is cheap (`Arc` inside the
/// underlying `VaultSubstrate`); the substrate `Database` handle itself
/// is `Clone` per RFC §Substrate Newtype Refactor.
#[derive(Clone)]
pub struct VaultStore {
    db: Arc<Database>,
    allowlist: Arc<AdapterAllowlist>,
    substrate: VaultSubstrate,
}

impl std::fmt::Debug for VaultStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VaultStore")
            .field("adapter_id", &self.allowlist.adapter().as_str())
            .finish_non_exhaustive()
    }
}

impl VaultStore {
    /// Construct a `VaultStore` from a shared `Database` handle.
    /// Caller is responsible for running `octo_vault::apply(&db)` (and
    /// any other migrations) before construction; this adapter does
    /// NOT auto-migrate.
    pub fn new(db: Arc<Database>) -> Self {
        let allowlist = Arc::new(build_allowlist());
        let substrate = VaultSubstrate::new(Arc::clone(&db));
        Self {
            db,
            allowlist,
            substrate,
        }
    }

    /// Borrow the underlying `AdapterAllowlist` (for verifying the
    /// typed surface was registered before execution).
    #[must_use]
    pub fn allowlist(&self) -> &AdapterAllowlist {
        &self.allowlist
    }

    /// Borrow the wrapped `VaultSubstrate`. Consumers needing the
    /// `lookup_by_vault_id` primitive (e.g. `octo-cap-macaroon-vault`
    /// when wiring `VaultLookup`) call this.
    #[must_use]
    pub fn substrate(&self) -> &VaultSubstrate {
        &self.substrate
    }

    /// Typed `INSERT INTO vaults` guard. Validates the table is in the
    /// adapter's allowlist, then runs the SQL via the substrate
    /// `Deref`-into-`stoolap::Database` path (per RFC §Substrate Newtype
    /// Refactor: the typed guard is the load-bearing step; SQL rendering
    /// + dispatch lives in the adapter crate).
    ///
    /// # Errors
    /// - [`octo_storage_core::SubstrateError::TableNotInNamespace`]
    ///   if the typed insert targets a table outside this adapter's
    ///   namespace.
    /// - [`octo_storage_core::SubstrateError::Storage`] if the
    ///   underlying Stoolap execution fails.
    pub fn insert_vault(
        &self,
        vault_id: &VaultId,
        chain_id: &ChainId,
        state: VaultState,
    ) -> Result<(), octo_storage_core::SubstrateError> {
        let stmt = TypedStatement::Insert(SqlInsert {
            table: TABLE_VAULTS.to_owned(),
        });
        self.db.execute_checked(&self.allowlist, &stmt)?;
        // Substrate guard passed — dispatch via Deref to `stoolap::Database`.
        let state_str = state.as_str();
        self.db
            .execute(
                "INSERT INTO vaults (vault_id, chain_id, state) VALUES (?, ?, ?)",
                (
                    vault_id.as_bytes().as_slice(),
                    chain_id.as_bytes().as_slice(),
                    state_str,
                ),
            )
            .map_err(|e| octo_storage_core::SubstrateError::Storage {
                operation: "insert_vault",
                message: format!("{e}"),
            })?;
        Ok(())
    }

    /// Typed `SELECT FROM vaults` guard. Validates the table is in the
    /// adapter's allowlist, then returns the same `(ChainId, VaultState)`
    /// tuple shape that `VaultSubstrate::lookup_by_vault_id` returns.
    pub fn lookup_by_vault_id(
        &self,
        vault_id: &VaultId,
    ) -> Result<Option<(ChainId, VaultState)>, octo_storage_core::SubstrateError> {
        let stmt = TypedStatement::Select(SqlSelect {
            tables: vec![TABLE_VAULTS.to_owned()],
        });
        self.db.execute_checked(&self.allowlist, &stmt)?;
        self.substrate.lookup_by_vault_id(vault_id).map_err(|_e| {
            octo_storage_core::SubstrateError::Storage {
                operation: "lookup_by_vault_id",
                message: "substrate lookup failed".into(),
            }
        })
    }
}

/// Register the `VaultStore` with the substrate's typed surface.
///
/// Per RFC §Wiring Pattern the facade's `register` helper is the
/// typed-surface witness: callers MUST build an `AdapterAllowlist` +
/// typed adapter before calling `register`, so the substrate's
/// load-bearing type system invariants are enforced. The body
/// consumes the `Arc<AdapterAllowlist>` (already built by
/// [`build_allowlist`]) + the `Arc<VaultStore>` and returns the
/// `Arc<VaultStore>` to the caller.
pub fn register(allowlist: Arc<AdapterAllowlist>, store: Arc<VaultStore>) -> Arc<VaultStore> {
    // Phase 1.9 hook: full registration body (writes `allowlist` to a
    // process-global registry keyed on `allowlist.adapter()`) lands
    // here. For now, this is the typed-surface witness — the
    // allowlist IS the adapter's typed surface, and the Arc is
    // returned unchanged.
    let _ = allowlist;
    store
}

/// Re-export of the owner crate's domain types for downstream
/// convenience (`octo-cap-macaroon-vault` does not need a direct
/// `octo-vault` dep just to plumb the substrate handle).
pub use octo_vault::{VaultError, VaultId as OwnerVaultId};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adapter_id_is_canonical() {
        assert_eq!(ADAPTER_ID, "octo-vault-storage/v1");
    }

    #[test]
    fn build_allowlist_registers_vaults_table() {
        let al = build_allowlist();
        assert_eq!(al.adapter().as_str(), ADAPTER_ID);
        assert!(al.tables().contains(TABLE_VAULTS));
        assert_eq!(al.ddl().len(), 1);
        assert_eq!(al.ddl()[0].id, DDL_VAULTS_VAULT_ID_IDX);
    }

    #[test]
    fn vault_store_new_returns_typed_handle() {
        let db = Arc::new(Database::open_in_memory().expect("open_in_memory"));
        let store = VaultStore::new(Arc::clone(&db));
        assert_eq!(store.allowlist().adapter().as_str(), ADAPTER_ID,);
    }
}
