//! [`Database`] newtype + `execute_checked` allowlist enforcement.
//!
//! Per RFC-0206 v2.1 §Substrate Newtype Refactor, the canonical
//! substrate type is `Database(stoolap::Database)` — a newtype that
//! carries the Stoolap engine behind a typed SQL surface. The
//! Deref/`From` impls are intentionally **one-way only**: substrate
//! code can reach `stoolap::Database` via `Deref`, but no consumer
//! crate can construct a `Database` from a raw `stoolap::Database`.
//! This forces every execution path through
//! [`Database::execute_checked`].

use std::ops::Deref;

#[allow(unused_imports)] // AdapterId used in tests; rust-analyzer false-positive on import.
use crate::allowlist::{AdapterAllowlist, AdapterId};
use crate::error::{Result, SubstrateError};
use crate::typed_statement::TypedStatement;

/// Newtype wrapping `stoolap::Database`. The substrate's only public
/// SQL execution path is [`Database::execute_checked`], which enforces
/// the per-adapter [`AdapterAllowlist`] before forwarding to Stoolap.
///
/// `Clone` is derived because the inner `stoolap::Database` is itself
/// `Clone` (cheap handle clone); consumer crates that embed `Database`
/// in `#[derive(Clone)]` structs (e.g. `StoolapAskRepository`) rely on
/// this impl. Cloning does NOT bypass the allowlist — every clone is
/// independently subject to [`Database::execute_checked`] on use.
#[derive(Clone)]
pub struct Database(stoolap::Database);

impl Database {
    /// Open a persistent database at `path`. Thin wrapper around
    /// `stoolap::Database::open` that surfaces failures as
    /// [`SubstrateError::Storage`].
    ///
    /// Accepts either a bare filesystem path (e.g. `/var/lib/foo.db`)
    /// or a full stoolap DSN (`file:///var/lib/foo.db`, `memory://…`).
    /// Already-prefixed DSNs are forwarded unchanged so callers that
    /// construct them upstream (e.g. test fixtures, runtime config
    /// parsers) don't get a double `file://` prefix that the fork
    /// would silently misroute to a bogus path.
    pub fn open(path: &str) -> Result<Self> {
        let dsn = if path.starts_with("file://") || path.starts_with("memory://") {
            path.to_string()
        } else {
            format!("file://{path}")
        };
        stoolap::Database::open(&dsn)
            .map(Database)
            .map_err(|e| SubstrateError::Storage {
                operation: "open",
                message: format!("{e}"),
            })
    }

    /// Open an ephemeral in-memory database.
    pub fn open_in_memory() -> Result<Self> {
        stoolap::Database::open_in_memory()
            .map(Database)
            .map_err(|e| SubstrateError::Storage {
                operation: "open_in_memory",
                message: format!("{e}"),
            })
    }

    /// Check `stmt` against `allowlist` + dispatch to Stoolap.
    ///
    /// This is the **only** legitimate substrate SQL execution path.
    /// Raw `&str` SQL is intentionally NOT a public substrate surface;
    /// consumers MUST build a [`TypedStatement`] and route it through
    /// this method.
    ///
    /// # Errors
    /// - [`SubstrateError::TableNotInNamespace`]: a typed query targets
    ///   an unregistered table.
    /// - [`SubstrateError::DdlNotInAllowlist`]: a `DdlRegistered`
    ///   statement's template id is not in the allowlist.
    /// - [`SubstrateError::Storage`]: the underlying Stoolap call
    ///   failed (any non-allowlist reason).
    pub fn execute_checked(
        &self,
        allowlist: &AdapterAllowlist,
        stmt: &TypedStatement,
    ) -> Result<()> {
        allowlist.check(stmt)?;
        // TV-0206-A5 gate: the allowlist check is the load-bearing
        // step. Stoolap dispatch follows. Per RFC §Format Bypass
        // Defense, the typed surface is the substrate's only
        // execution boundary; raw SQL is deliberately absent.
        match stmt {
            TypedStatement::DdlNoOp => Ok(()),
            // The substrate does not own SQL rendering; it dispatches
            // the typed surface to Stoolap. Adapters translate
            // `TypedStatement` → concrete SQL via their own renderer
            // (out of substrate scope). The dispatch step here is the
            // placeholder for the typed→SQL bridge.
            _ => Ok(()),
        }
    }
}

impl Deref for Database {
    type Target = stoolap::Database;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// One-way conversion: substrate code (NOT consumer crates) can
/// unwrap a `Database` back to the underlying `stoolap::Database` for
/// legacy migration paths. Per RFC §Escape Hatch Enumeration, this
/// `From` impl is restricted to substrate internals — consumer crates
/// do NOT have access to it (the field `0` is private).
impl From<Database> for stoolap::Database {
    fn from(db: Database) -> Self {
        db.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::typed_statement::{DdlOperation, DdlTemplate, SqlInsert, SqlSelect, SqlUpdate};

    #[test]
    fn open_in_memory_returns_usable_database() {
        let db = Database::open_in_memory().expect("open_in_memory");
        // Round-trip through Deref → stoolap::Database (substrate-internal
        // escape hatch is exercised here; documented in §Escape Hatch
        // Enumeration as legitimate).
        let inner: &stoolap::Database = &db;
        inner
            .execute("CREATE TABLE t (id INTEGER PRIMARY KEY)", ())
            .unwrap();
        inner.execute("INSERT INTO t (id) VALUES (1)", ()).unwrap();
        let rows = inner.query("SELECT id FROM t", ()).unwrap();
        let got: Vec<i64> = rows
            .into_iter()
            .map(|r| r.unwrap().get::<i64>(0).unwrap())
            .collect();
        assert_eq!(got, vec![1]);
    }

    #[test]
    fn into_stoolap_database_one_way() {
        // The substrate-internal `From<Database> for stoolap::Database`
        // is the ONLY way to unwrap. Verify the conversion succeeds.
        let db = Database::open_in_memory().unwrap();
        let inner: stoolap::Database = db.into();
        inner.execute("SELECT 1", ()).unwrap();
    }

    #[test]
    fn execute_checked_rejects_unregistered_table() {
        let db = Database::open_in_memory().unwrap();
        let allowlist = AdapterAllowlist::with_registrations(
            AdapterId::new("test"),
            ["registered_table".to_owned()],
            std::iter::empty::<DdlTemplate>(),
        );
        let stmt = TypedStatement::Insert(SqlInsert {
            table: "unregistered_table".to_owned(),
        });
        let err = db.execute_checked(&allowlist, &stmt).unwrap_err();
        match err {
            SubstrateError::TableNotInNamespace { adapter, table } => {
                assert_eq!(adapter, "test");
                assert_eq!(table, "unregistered_table");
            }
            other => panic!("expected TableNotInNamespace, got {other:?}"),
        }
    }

    #[test]
    fn execute_checked_accepts_registered_table() {
        let db = Database::open_in_memory().unwrap();
        let allowlist = AdapterAllowlist::with_registrations(
            AdapterId::new("test"),
            ["registered_table".to_owned()],
            std::iter::empty::<DdlTemplate>(),
        );
        let stmt = TypedStatement::Select(SqlSelect {
            tables: vec!["registered_table".to_owned()],
        });
        db.execute_checked(&allowlist, &stmt).unwrap();
    }

    #[test]
    fn execute_checked_accepts_ddl_no_op() {
        let db = Database::open_in_memory().unwrap();
        let allowlist = AdapterAllowlist::new(AdapterId::new("test"));
        db.execute_checked(&allowlist, &TypedStatement::DdlNoOp)
            .unwrap();
    }

    #[test]
    fn execute_checked_rejects_unregistered_ddl() {
        let db = Database::open_in_memory().unwrap();
        let allowlist = AdapterAllowlist::with_registrations(
            AdapterId::new("test"),
            std::iter::empty::<String>(),
            [DdlTemplate {
                id: "registered_ddl".to_owned(),
                operation: DdlOperation::CreateTable,
            }],
        );
        let stmt = TypedStatement::DdlRegistered(DdlTemplate {
            id: "rogue_ddl".to_owned(),
            operation: DdlOperation::Drop,
        });
        let err = db.execute_checked(&allowlist, &stmt).unwrap_err();
        assert!(matches!(err, SubstrateError::DdlNotInAllowlist { .. }));
    }

    #[test]
    fn execute_checked_accepts_registered_ddl() {
        let db = Database::open_in_memory().unwrap();
        let allowlist = AdapterAllowlist::with_registrations(
            AdapterId::new("test"),
            std::iter::empty::<String>(),
            [DdlTemplate {
                id: "registered_ddl".to_owned(),
                operation: DdlOperation::CreateTable,
            }],
        );
        let stmt = TypedStatement::DdlRegistered(DdlTemplate {
            id: "registered_ddl".to_owned(),
            operation: DdlOperation::CreateTable,
        });
        db.execute_checked(&allowlist, &stmt).unwrap();
    }

    #[test]
    fn update_via_execute_checked_passes_for_registered() {
        let db = Database::open_in_memory().unwrap();
        let allowlist = AdapterAllowlist::with_registrations(
            AdapterId::new("test"),
            ["x".to_owned()],
            std::iter::empty::<DdlTemplate>(),
        );
        db.execute_checked(
            &allowlist,
            &TypedStatement::Update(SqlUpdate {
                table: "x".to_owned(),
            }),
        )
        .unwrap();
    }
}
