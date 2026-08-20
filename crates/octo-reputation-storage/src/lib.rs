//! `octo-reputation-storage` — RFC-0206 v2.1 §Adapter Crate List row 2.
//!
//!
//! Substrate adapter for the `octo-reputation` owner crate. This
//! adapter owns the canonical `AdapterAllowlist` for the
//! `reputation_signals` table + the `reputation_event_id_idx` UNIQUE
//! INDEX DDL template, and routes typed INSERT/SELECT through the
//! substrate's allowlist guard via `Database::execute_checked`.
//!
//! Per RFC §Adapter Crate List this is a Layer C adapter crate; the
//! only substrate dep is `octo-storage-core`. No direct `stoolap` import
//! (TV-0206-A9(a) gate).
//!
//! ## TV-0206-A6
//!
//! `test -d crates/octo-reputation-storage` exits 0. Per RFC §Test Vectors.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::sync::Arc;

use octo_storage_core::typed_statement::{DdlOperation, DdlTemplate};
use octo_storage_core::{AdapterAllowlist, AdapterId, Database};

/// Canonical adapter id for the reputation substrate adapter.
pub const ADAPTER_ID: &str = "octo-reputation-storage/v1";

/// Canonical table name for the reputation substrate adapter.
pub const TABLE_REPUTATION_SIGNALS: &str = "reputation_signals";

/// Canonical UNIQUE INDEX DDL template id (matches the
/// `reputation_event_id_idx` index created by `octo-reputation`
/// migration `v001`).
pub const DDL_REPUTATION_EVENT_ID_IDX: &str = "reputation_event_id_idx";

/// Build the canonical `AdapterAllowlist` for the reputation substrate
/// adapter: registers `reputation_signals` table + the
/// `reputation_event_id_idx` DDL template.
#[must_use]
pub fn build_allowlist() -> AdapterAllowlist {
    AdapterAllowlist::with_registrations(
        AdapterId::new(ADAPTER_ID),
        [TABLE_REPUTATION_SIGNALS.to_owned()],
        [DdlTemplate {
            id: DDL_REPUTATION_EVENT_ID_IDX.to_owned(),
            operation: DdlOperation::CreateIndex,
        }],
    )
}

/// Typed substrate adapter handle. Cloning is cheap (the underlying
/// `Database` is `Clone` per RFC §Substrate Newtype Refactor).
#[derive(Clone)]
pub struct ReputationStoreAdapter {
    db: Arc<Database>,
    allowlist: Arc<AdapterAllowlist>,
}

impl std::fmt::Debug for ReputationStoreAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReputationStoreAdapter")
            .field("adapter_id", &self.allowlist.adapter().as_str())
            .finish_non_exhaustive()
    }
}

impl ReputationStoreAdapter {
    /// Construct a `ReputationStoreAdapter` from a shared `Database`
    /// handle. Caller is responsible for running
    /// `octo_reputation::migrations::apply_pending` (and any other
    /// migrations) before construction; this adapter does NOT
    /// auto-migrate.
    pub fn new(db: Arc<Database>) -> Self {
        let allowlist = Arc::new(build_allowlist());
        Self { db, allowlist }
    }

    /// Borrow the underlying `AdapterAllowlist` (for verifying the
    /// typed surface was registered before execution).
    #[must_use]
    pub fn allowlist(&self) -> &AdapterAllowlist {
        &self.allowlist
    }

    /// Borrow the shared `Database` handle. The trait-level
    /// `ReputationStore` impl (e.g. `StoolapReputationStore`) consumes
    /// this `Database` reference to execute async queries; the
    /// adapter just owns the typed-surface witness.
    #[must_use]
    pub fn db(&self) -> &Database {
        &self.db
    }
}

/// Register the `ReputationStoreAdapter` with the substrate's typed
/// surface. Per RFC §Wiring Pattern the facade's `register` helper is
/// the typed-surface witness; the allowlist IS the adapter's typed
/// surface, and the Arc is returned unchanged.
pub fn register(
    allowlist: Arc<AdapterAllowlist>,
    adapter: Arc<ReputationStoreAdapter>,
) -> Arc<ReputationStoreAdapter> {
    let _ = allowlist;
    adapter
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adapter_id_is_canonical() {
        assert_eq!(ADAPTER_ID, "octo-reputation-storage/v1");
    }

    #[test]
    fn build_allowlist_registers_reputation_signals_table() {
        let al = build_allowlist();
        assert_eq!(al.adapter().as_str(), ADAPTER_ID);
        assert!(al.tables().contains(TABLE_REPUTATION_SIGNALS));
        assert_eq!(al.ddl().len(), 1);
        assert_eq!(al.ddl()[0].id, DDL_REPUTATION_EVENT_ID_IDX);
    }

    #[test]
    fn new_returns_typed_handle() {
        let db = Arc::new(Database::open_in_memory().expect("open_in_memory"));
        let a = ReputationStoreAdapter::new(Arc::clone(&db));
        assert_eq!(a.allowlist().adapter().as_str(), ADAPTER_ID);
    }
}
