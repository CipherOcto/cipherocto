//! `octo-policy-storage` — RFC-0206 v2.1 §Adapter Crate List row 5.
//!
//! Substrate adapter for the `octo-policy` owner crate. The
//! `PolicyStore` trait surface is declared in `octo-policy` (NEW per
//! mission `0206-009-adapter-crate-creation`); this crate provides
//! the substrate `AdapterAllowlist` witness + register helper.
//!
//! Per RFC §Adapter Crate List this is a Layer C adapter crate; the
//! only substrate dep is `octo-storage-core`. No direct `stoolap` import
//! (TV-0206-A9(a) gate).
//!
//! ## TV-0206-A6
//!
//! `test -d crates/octo-policy-storage` exits 0.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::sync::Arc;

use octo_storage_core::typed_statement::{DdlOperation, DdlTemplate};
use octo_storage_core::{AdapterAllowlist, AdapterId, Database};

/// Canonical adapter id for the policy substrate adapter.
pub const ADAPTER_ID: &str = "octo-policy-storage/v1";

/// Canonical table name for the policy substrate adapter (holds the
/// canonical `PolicyObject` rows; matches the policy-graph schema
/// declared in `octo-policy`).
pub const TABLE_POLICY_OBJECTS: &str = "policy_objects";

/// Canonical UNIQUE INDEX DDL template id (matches the
/// `policy_objects_id_idx` UNIQUE INDEX on `policy_id`).
pub const DDL_POLICY_OBJECTS_ID_IDX: &str = "policy_objects_id_idx";

/// Build the canonical `AdapterAllowlist` for the policy substrate
/// adapter.
#[must_use]
pub fn build_allowlist() -> AdapterAllowlist {
    AdapterAllowlist::with_registrations(
        AdapterId::new(ADAPTER_ID),
        [TABLE_POLICY_OBJECTS.to_owned()],
        [DdlTemplate {
            id: DDL_POLICY_OBJECTS_ID_IDX.to_owned(),
            operation: DdlOperation::CreateIndex,
        }],
    )
}

/// Typed substrate adapter handle. Cloning is cheap (the underlying
/// `Database` is `Clone` per RFC §Substrate Newtype Refactor).
#[derive(Clone)]
pub struct PolicyStoreAdapter {
    db: Arc<Database>,
    allowlist: Arc<AdapterAllowlist>,
}

impl std::fmt::Debug for PolicyStoreAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PolicyStoreAdapter")
            .field("adapter_id", &self.allowlist.adapter().as_str())
            .finish_non_exhaustive()
    }
}

impl PolicyStoreAdapter {
    /// Construct a `PolicyStoreAdapter` from a shared `Database`
    /// handle. Caller is responsible for running
    /// `octo_policy::apply(&db)` (and any other migrations) before
    /// construction; this adapter does NOT auto-migrate.
    pub fn new(db: Arc<Database>) -> Self {
        let allowlist = Arc::new(build_allowlist());
        Self { db, allowlist }
    }

    /// Borrow the underlying `AdapterAllowlist`.
    #[must_use]
    pub fn allowlist(&self) -> &AdapterAllowlist {
        &self.allowlist
    }

    /// Borrow the shared `Database` handle. The trait-level
    /// `PolicyStore` impl (declared in `octo-policy`) consumes this
    /// reference to execute queries.
    #[must_use]
    pub fn db(&self) -> &Database {
        &self.db
    }
}

/// Register the `PolicyStoreAdapter` with the substrate's typed
/// surface. Per RFC §Wiring Pattern the allowlist IS the adapter's
/// typed surface, and the Arc is returned unchanged.
pub fn register(
    allowlist: Arc<AdapterAllowlist>,
    adapter: Arc<PolicyStoreAdapter>,
) -> Arc<PolicyStoreAdapter> {
    let _ = allowlist;
    adapter
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adapter_id_is_canonical() {
        assert_eq!(ADAPTER_ID, "octo-policy-storage/v1");
    }

    #[test]
    fn build_allowlist_registers_policy_objects_table() {
        let al = build_allowlist();
        assert_eq!(al.adapter().as_str(), ADAPTER_ID);
        assert!(al.tables().contains(TABLE_POLICY_OBJECTS));
        assert_eq!(al.ddl().len(), 1);
        assert_eq!(al.ddl()[0].id, DDL_POLICY_OBJECTS_ID_IDX);
    }

    #[test]
    fn new_returns_typed_handle() {
        let db = Arc::new(Database::open_in_memory().expect("open_in_memory"));
        let a = PolicyStoreAdapter::new(Arc::clone(&db));
        assert_eq!(a.allowlist().adapter().as_str(), ADAPTER_ID);
    }
}
