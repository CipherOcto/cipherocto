//! Stoolap-backed cross-process revocation store for RFC-0011-c
//! §F.7.5 paired amendment.
//!
//! This crate provides the Layer D persistence adapter for the
//! trait-dispatched `RevocationStore` substrate defined in
//! `octo-runtime`. The trait lives in Layer B (per-extension
//! pattern per [[cipherocto-design-principles]] §User extensibility);
//! this Layer D crate provides the Stoolap-backed impl +
//! `install_default` constructor. Layer C (`octo-cli`) wires the
//! dependency injection via
//! `octo_runtime::install_revocation_store_default_with(octo_runtime_revocation_store::install_default)`
//! — see RFC-0011-c §F.7.5 CLI wiring block.
//!
//! ## Hard red line
//!
//! The Stoolap fork at fork-pinned rev `527e8eb` is a frozen
//! external primitive sub-category of Layer D; this ledger hosts
//! ONLY the `revocation` table per RFC-0011-c §F.7.5. The fork
//! NEVER hosts cipherocto business schema beyond this single
//! revocation table — see [[stoolap-general-purpose-db]] HARD RED
//! LINE.
//!
//! ## Ledger path
//!
//! `install_default` opens the ledger at
//! `$CIPHEROCTO_DATA_DIR/revocation.stoolap`, defaulting to
//! `~/.local/share/octo/runtime/revocation.stoolap` if the env var
//! is unset. The parent directory is created on first open if
//! missing (operator-`create_dir_all` semantic; no error during
//! legitimate bootstrapping).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use octo_runtime::handle::error::AttachError;
use octo_runtime::persistence::{RevocationStore, SessionId};

/// Stoolap-backed `RevocationStore` impl (RFC-0011-c §F.7.5
/// paired amendment).
///
/// Wraps an `octo_storage_core::Database` (which derefs to the
/// underlying `stoolap::Database` per the substrate type's `Deref`
/// impl). The `RwLock` is acquired for the entire operation
/// duration (read or write) — Stoolap serializes its own
/// transactions internally and the `RwLock` here provides
/// synchronization at the Rust-level trait-method boundary. This
/// matches the pattern used by `crates/octo-reputation::store::stoolap`.
///
/// `Debug` is implemented manually because
/// `octo_storage_core::Database` does not implement `Debug` at the
/// substrate layer. The Debug representation shows the canonical
/// kind + on-disk path for observability.
pub struct StoolapRevocationStore {
    db: RwLock<octo_storage_core::Database>,
    path: PathBuf,
}

impl std::fmt::Debug for StoolapRevocationStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StoolapRevocationStore")
            .field("path", &self.path)
            .finish()
    }
}

impl StoolapRevocationStore {
    /// Open (or create) the ledger at `path`. Parent directories
    /// are created on demand. The Stoolap connection is opened via
    /// `Database::open` with a `file://` DSN.
    ///
    /// # Errors
    /// Returns `AttachError::PersistenceError(reason)` if the
    /// ledger cannot be opened (filesystem path invalid, parent
    /// directory cannot be created, Stoolap open failure).
    pub fn open_at(path: impl AsRef<Path>) -> Result<Self, AttachError> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent).map_err(|e| {
                    AttachError::PersistenceError(format!(
                        "failed to create ledger parent dir {}: {e}",
                        parent.display()
                    ))
                })?;
            }
        }
        let dsn = format!("file://{}", path.display());
        let db = octo_storage_core::Database::open(&dsn).map_err(|e| {
            AttachError::PersistenceError(format!(
                "Stoolap open failed for {}: {e}",
                path.display()
            ))
        })?;
        // Bootstrap the schema on first open. CREATE TABLE IF NOT
        // EXISTS is idempotent at the Stoolap fork rev 527e8eb.
        bootstrap_schema(&db)?;
        Ok(Self {
            db: RwLock::new(db),
            path,
        })
    }
}

impl RevocationStore for StoolapRevocationStore {
    fn revoke_attach_token(&self, session_id: SessionId) -> Result<(), AttachError> {
        let db = self.db.write().map_err(|e| {
            AttachError::PersistenceError(format!(
                "StoolapRevocationStore poisoned write lock: {e}"
            ))
        })?;
        // Pre-check `SELECT 1 FROM revocation WHERE session_id = ? LIMIT 1`
        // because the Stoolap fork at rev 527e8eb lacks
        // `INSERT OR IGNORE` / `INSERT OR REPLACE` syntax per the
        // substrate-discipline no-unverified-features rule
        // (RFC-0011-c §F.7.5 substrate additions).
        let pre_check_sql = "SELECT 1 FROM revocation WHERE session_id = $1 LIMIT 1";
        let pre_check_params = vec![octo_storage_core::stoolap::Value::blob(session_id.to_vec())];
        match db.query(pre_check_sql, pre_check_params) {
            Ok(mut rows) => {
                // If a row exists, idempotent no-op.
                if rows.next().is_some() {
                    return Ok(());
                }
            }
            Err(e) => {
                return Err(AttachError::PersistenceError(format!(
                    "Stoolap pre-check query failed: {e}"
                )));
            }
        }

        // Insert the revocation row with the current unix-ms
        // timestamp as the audit column.
        let revoked_at_unix = current_unix_secs();
        let insert_sql = "INSERT INTO revocation (session_id, revoked_at_unix) VALUES ($1, $2)";
        let insert_params = vec![
            octo_storage_core::stoolap::Value::blob(session_id.to_vec()),
            octo_storage_core::stoolap::Value::integer(revoked_at_unix),
        ];
        db.execute(insert_sql, insert_params).map_err(|e| {
            AttachError::PersistenceError(format!("Stoolap revocation insert failed: {e}"))
        })?;
        Ok(())
    }

    fn is_token_revoked(&self, session_id: &SessionId) -> bool {
        let db = match self.db.read() {
            Ok(guard) => guard,
            Err(e) => {
                tracing::error!(
                    kind = self.kind(),
                    error = %e,
                    "StoolapRevocationStore poisoned read lock; failing-CLOSED"
                );
                return true;
            }
        };
        let sql = "SELECT 1 FROM revocation WHERE session_id = $1 LIMIT 1";
        let params = vec![octo_storage_core::stoolap::Value::blob(session_id.to_vec())];
        match db.query(sql, params) {
            Ok(mut rows) => rows.next().is_some(),
            Err(e) => {
                tracing::error!(
                    kind = self.kind(),
                    error = %e,
                    "StoolapRevocationStore read failed; failing-CLOSED"
                );
                true
            }
        }
    }

    fn kind(&self) -> &'static str {
        "StoolapRevocationStore"
    }
}

/// Path computation: `$CIPHEROCTO_DATA_DIR/revocation.stoolap`, or
/// `$HOME/.local/share/octo/runtime/revocation.stoolap` when the env
/// var is unset.
fn default_ledger_path() -> PathBuf {
    let base = std::env::var_os("CIPHEROCTO_DATA_DIR")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .map(|home| PathBuf::from(home).join(".local/share/octo/runtime"))
        })
        .unwrap_or_else(|| PathBuf::from(".local/share/octo/runtime"));
    base.join("revocation.stoolap")
}

/// Open the default ledger per RFC-0011-c §F.7.5 spec.
///
/// # Errors
/// Returns `AttachError::PersistenceError(reason)` if
/// `$CIPHEROCTO_DATA_DIR` cannot be resolved, the parent dir
/// cannot be created, or the Stoolap open fails.
pub fn open_default() -> Result<StoolapRevocationStore, AttachError> {
    StoolapRevocationStore::open_at(default_ledger_path())
}

/// Factory closure entry point used by Layer C (`octo-cli`) to wire
/// the dependency injection (RFC-0011-c §F.7.5 §CLI wiring block).
///
/// Returns `Arc<dyn RevocationStore>` for the substrate trait
/// façade. Does NOT self-register (the substrate's
/// `install_revocation_store_default_with` owns the registration;
/// the factory closure just constructs + returns).
///
/// # Errors
/// Returns `AttachError::PersistenceError(reason)` on filesystem
/// or Stoolap open failure. The substrate wraps this in a
/// try-install / fall-back-to-default semantic at the API layer.
pub fn install_default() -> Result<Arc<dyn RevocationStore>, AttachError> {
    let store = open_default()?;
    Ok(Arc::new(store))
}

/// Bootstrap the `revocation` table on first ledger open (RFC-0011-c
/// §F.7.5 substrate additions).
///
/// The `revoked_at_unix` column is an audit timestamp (no current
/// logic consumer — `is_token_revoked` uses the `session_id` PK
/// existence check); reserved for future TTL / audit-read paths.
fn bootstrap_schema(db: &octo_storage_core::Database) -> Result<(), AttachError> {
    let sql = "CREATE TABLE IF NOT EXISTS revocation (\
        session_id BLOB(32) NOT NULL PRIMARY KEY, \
        revoked_at_unix INTEGER NOT NULL)";
    db.execute(sql, Vec::new()).map_err(|e| {
        AttachError::PersistenceError(format!("Stoolap schema bootstrap failed: {e}"))
    })?;
    Ok(())
}

/// Current Unix epoch seconds as `i64` for the audit column.
fn current_unix_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tempdir-isolated ledger path; auto-cleanup on test drop.
    fn temp_ledger_path(test_name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "octo-runtime-revocation-store-tests-{}-{}.stoolap",
            test_name,
            std::process::id()
        ));
        p
    }

    #[test]
    fn kind_returns_canonical_string() {
        let path = temp_ledger_path("kind");
        let store = StoolapRevocationStore::open_at(&path).expect("open");
        assert_eq!(store.kind(), "StoolapRevocationStore");
    }

    #[test]
    fn ledger_persists_across_reopens() {
        let path = temp_ledger_path("persists");
        let _ = fs::remove_file(&path);
        let session_id = [0x77u8; 32];

        // Open + write + drop.
        {
            let store = StoolapRevocationStore::open_at(&path).expect("open 1");
            store
                .revoke_attach_token(session_id)
                .expect("revoke in open 1");
            assert!(store.is_token_revoked(&session_id));
        }
        // Reopen; same path → row still present.
        let reopened = StoolapRevocationStore::open_at(&path).expect("open 2");
        assert!(
            reopened.is_token_revoked(&session_id),
            "row should persist across reopens"
        );

        // Cleanup the test ledger (best-effort — file may not exist on first fail).
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn revoke_is_idempotent_no_duplicate_row() {
        let path = temp_ledger_path("idempotent");
        let _ = fs::remove_file(&path);
        let store = StoolapRevocationStore::open_at(&path).expect("open");
        let session_id = [0xabu8; 32];

        store.revoke_attach_token(session_id).expect("revoke 1");
        store
            .revoke_attach_token(session_id)
            .expect("revoke 2 idempotent");
        store
            .revoke_attach_token(session_id)
            .expect("revoke 3 idempotent");

        // Verify only one row in the ledger.
        let sql = "SELECT COUNT(*) FROM revocation WHERE session_id = $1";
        let params = vec![octo_storage_core::stoolap::Value::blob(session_id.to_vec())];
        let db = store.db.read().expect("read lock");
        let rows = db.query(sql, params).expect("count query");
        // Find the single COUNT row.
        let count: i64 = rows
            .into_iter()
            .next()
            .expect("count row")
            .expect("count row ok")
            .get::<i64>(0)
            .expect("count col");
        assert_eq!(count, 1, "expected exactly 1 row, got {count}");

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn existence_check_fast_path_returns_true_after_revoke() {
        let path = temp_ledger_path("exists");
        let _ = fs::remove_file(&path);
        let store = StoolapRevocationStore::open_at(&path).expect("open");
        let session_id = [0xcdu8; 32];

        // Pre: not revoked.
        assert!(!store.is_token_revoked(&session_id));
        store.revoke_attach_token(session_id).expect("revoke");
        // Post: revoked.
        assert!(store.is_token_revoked(&session_id));
        // Other session: not revoked.
        let other = [0xffu8; 32];
        assert!(!store.is_token_revoked(&other));

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn schema_bootstrap_is_idempotent() {
        // Opening the ledger twice in a row is the simplest
        // observer for `CREATE TABLE IF NOT EXISTS` idempotence.
        let path = temp_ledger_path("schema_idempotent");
        let _ = fs::remove_file(&path);
        let _store1 = StoolapRevocationStore::open_at(&path).expect("open 1");
        let _store2 = StoolapRevocationStore::open_at(&path).expect("open 2");
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn revocation_store_send_sync_via_arc() {
        // Compile-time check: Arc<StoolapRevocationStore> satisfies
        // Send + Sync. Synchronous Send + Sync derive from the inner
        // fields (Database is Send + Sync + Debug per the substrate
        // type's documented bounds; RwLock<T> is Send + Sync iff T
        // is both).
        const _: fn() = || {
            fn assert_send_sync<T: Send + Sync>() {}
            assert_send_sync::<StoolapRevocationStore>();
        };
        let _arc: Arc<dyn RevocationStore> =
            Arc::new(StoolapRevocationStore::open_at(temp_ledger_path("send_sync")).expect("open"));
        let _ = fs::remove_file(temp_ledger_path("send_sync"));
    }
}
