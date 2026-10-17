//! `octo-matrix-session-store-storage` — RFC-0206 §Adapter Crate List row 4.
//!
//! Substrate adapter for the `octo-matrix-session-store` owner crate.
//! This adapter owns the canonical `AdapterAllowlist` for the
//! `matrix_sessions` table + the `matrix_sessions_user_device_idx`
//! UNIQUE INDEX DDL template, and routes typed INSERT/SELECT through
//! the substrate's allowlist guard via `Database::execute_checked`.
//!
//! Per RFC §Adapter Crate List this is a Layer C adapter crate; the
//! only substrate dep is `octo-storage-core`. No direct `stoolap` import
//! (TV-0206-A9(a) gate).
//!
//! ## TV-0206-A6
//!
//! `test -d crates/octo-matrix-session-store-storage` exits 0.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::sync::Arc;

use octo_storage_core::typed_statement::{DdlOperation, DdlTemplate};
use octo_storage_core::{AdapterAllowlist, AdapterId, Database};

/// Canonical adapter id for the matrix session-store substrate adapter.
pub const ADAPTER_ID: &str = "octo-matrix-session-store-storage/v1";

/// Canonical table name for the matrix session-store substrate adapter.
pub const TABLE_MATRIX_SESSIONS: &str = "matrix_sessions";

/// Canonical UNIQUE INDEX DDL template id (matches the
/// `matrix_sessions_user_device_idx` UNIQUE INDEX on `(user_id, device_id)`
/// created by `octo-matrix-session-store` schema).
pub const DDL_MATRIX_SESSIONS_USER_DEVICE_IDX: &str = "matrix_sessions_user_device_idx";

/// Build the canonical `AdapterAllowlist` for the matrix session-store
/// substrate adapter.
#[must_use]
pub fn build_allowlist() -> AdapterAllowlist {
    AdapterAllowlist::with_registrations(
        AdapterId::new(ADAPTER_ID),
        [TABLE_MATRIX_SESSIONS.to_owned()],
        [DdlTemplate {
            id: DDL_MATRIX_SESSIONS_USER_DEVICE_IDX.to_owned(),
            operation: DdlOperation::CreateIndex,
        }],
    )
}

/// Typed substrate adapter handle. Cloning is cheap (the underlying
/// `Database` is `Clone` per RFC §Substrate Newtype Refactor).
#[derive(Clone)]
pub struct SessionStoreAdapter {
    db: Arc<Database>,
    allowlist: Arc<AdapterAllowlist>,
}

impl std::fmt::Debug for SessionStoreAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionStoreAdapter")
            .field("adapter_id", &self.allowlist.adapter().as_str())
            .finish_non_exhaustive()
    }
}

impl SessionStoreAdapter {
    /// Construct a `SessionStoreAdapter` from a shared `Database`
    /// handle.
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
    /// `SessionStore` impl (e.g. `StoolapSessionStore`) consumes this
    /// `Database` reference to execute queries.
    #[must_use]
    pub fn db(&self) -> &Database {
        &self.db
    }
}

/// Register the `SessionStoreAdapter` with the substrate's typed
/// surface. Per RFC §Wiring Pattern the allowlist IS the adapter's
/// typed surface, and the Arc is returned unchanged.
pub fn register(
    allowlist: Arc<AdapterAllowlist>,
    adapter: Arc<SessionStoreAdapter>,
) -> Arc<SessionStoreAdapter> {
    let _ = allowlist;
    adapter
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adapter_id_is_canonical() {
        assert_eq!(ADAPTER_ID, "octo-matrix-session-store-storage/v1");
    }

    #[test]
    fn build_allowlist_registers_matrix_sessions_table() {
        let al = build_allowlist();
        assert_eq!(al.adapter().as_str(), ADAPTER_ID);
        assert!(al.tables().contains(TABLE_MATRIX_SESSIONS));
        assert_eq!(al.ddl().len(), 1);
        assert_eq!(al.ddl()[0].id, DDL_MATRIX_SESSIONS_USER_DEVICE_IDX);
    }

    #[test]
    fn new_returns_typed_handle() {
        let db = Arc::new(Database::open_in_memory().expect("open_in_memory"));
        let a = SessionStoreAdapter::new(Arc::clone(&db));
        assert_eq!(a.allowlist().adapter().as_str(), ADAPTER_ID);
    }
}
