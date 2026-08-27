//! [`TypedStatement`] — typed SQL surface for `Database::execute_checked`.
//!
//! Per RFC-0206 §Substrate Newtype Refactor, the substrate exposes
//! a 6-variant `TypedStatement` enum. The 5 typed-query variants
//! (`Select`/`Insert`/`Update`/`Delete`) carry a table-typed payload so
//! the [`AdapterAllowlist`](crate::AdapterAllowlist) can enforce the
//! adapter-namespace contract; the 2 DDL variants (`DdlNoOp` /
//! `DdlRegistered`) split the substrate's DDL surface into "no
//! permission needed" and "must be pre-registered" paths.

/// Typed SQL statement. The `Database::execute_checked` method accepts
/// this enum; raw `&str` SQL is NOT a public substrate surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypedStatement {
    /// Typed SELECT against one or more registered tables.
    Select(SqlSelect),
    /// Typed INSERT into a single registered table.
    Insert(SqlInsert),
    /// Typed UPDATE against a single registered table.
    Update(SqlUpdate),
    /// Typed DELETE from a single registered table.
    Delete(SqlDelete),
    /// A DDL no-op (e.g. an idempotent `CREATE TABLE IF NOT EXISTS`
    /// against an already-existing table, or an empty statement). The
    /// allowlist treats `DdlNoOp` as safe to dispatch without further
    /// registration.
    DdlNoOp,
    /// A pre-registered DDL template that has been declared in the
    /// adapter's [`AdapterAllowlist`](crate::AdapterAllowlist).
    DdlRegistered(DdlTemplate),
}

impl TypedStatement {
    /// Tables referenced by this statement. Used by
    /// [`AdapterAllowlist::check`](crate::AdapterAllowlist::check) to
    /// enforce the per-adapter namespace contract.
    pub fn tables(&self) -> Vec<String> {
        match self {
            Self::Select(s) => s.tables.clone(),
            Self::Insert(i) => vec![i.table.clone()],
            Self::Update(u) => vec![u.table.clone()],
            Self::Delete(d) => vec![d.table.clone()],
            Self::DdlNoOp | Self::DdlRegistered(_) => Vec::new(),
        }
    }

    /// Canonical identifier of the DDL template (only meaningful for
    /// [`Self::DdlRegistered`]). Returns `None` for all other variants.
    pub fn ddl_template_id(&self) -> Option<&str> {
        match self {
            Self::DdlRegistered(t) => Some(&t.id),
            _ => None,
        }
    }
}

/// Typed SELECT query. `tables` lists every table the statement
/// references (JOIN targets included); the allowlist verifies that
/// every entry is registered for the calling adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SqlSelect {
    /// Tables referenced by the SELECT (FROM + JOIN targets).
    pub tables: Vec<String>,
}

/// Typed INSERT statement. `table` is the single target table;
/// the allowlist verifies the entry is registered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SqlInsert {
    /// Single target table for the INSERT.
    pub table: String,
}

/// Typed UPDATE statement. `table` is the single target table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SqlUpdate {
    /// Single target table for the UPDATE.
    pub table: String,
}

/// Typed DELETE statement. `table` is the single target table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SqlDelete {
    /// Single target table for the DELETE.
    pub table: String,
}

/// A pre-registered DDL template. Adapters register a `DdlTemplate` at
/// startup; the substrate matches incoming `DdlRegistered` statements
/// against the registered set before dispatching.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DdlTemplate {
    /// Canonical template identifier (e.g. `"create_did_registry"`).
    /// Must match the `id` registered with
    /// [`AdapterAllowlist::register_ddl`](crate::AdapterAllowlist::register_ddl).
    pub id: String,
    /// The DDL operation kind (CREATE TABLE / CREATE INDEX / ALTER
    /// TABLE ADD COLUMN / etc.). The substrate does NOT execute the
    /// SQL; the adapter owns that. The operation tag is purely for
    /// audit + allowlist dispatch.
    pub operation: DdlOperation,
}

/// DDL operation kind for [`DdlTemplate`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DdlOperation {
    /// `CREATE TABLE` (with or without `IF NOT EXISTS`).
    CreateTable,
    /// `CREATE INDEX` (with or without `IF NOT EXISTS`).
    CreateIndex,
    /// `ALTER TABLE` (ADD COLUMN / DROP COLUMN / RENAME).
    AlterTable,
    /// `DROP TABLE` / `DROP INDEX` (idempotent variants only — raw
    /// `DROP TABLE x` requires operator confirmation at the adapter
    /// boundary, not the substrate).
    Drop,
    /// Other DDL not covered by the enum variants. Adapters that need
    /// a new DDL kind must extend this enum and add a matching
    /// allowlist dispatch arm.
    Other,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_tables_returns_all_referenced_tables() {
        let stmt = TypedStatement::Select(SqlSelect {
            tables: vec!["asks".to_owned(), "bids".to_owned()],
        });
        assert_eq!(stmt.tables(), vec!["asks".to_owned(), "bids".to_owned()]);
    }

    #[test]
    fn insert_update_delete_table_is_single() {
        let i = TypedStatement::Insert(SqlInsert {
            table: "asks".to_owned(),
        });
        assert_eq!(i.tables(), vec!["asks".to_owned()]);
        let u = TypedStatement::Update(SqlUpdate {
            table: "asks".to_owned(),
        });
        assert_eq!(u.tables(), vec!["asks".to_owned()]);
        let d = TypedStatement::Delete(SqlDelete {
            table: "asks".to_owned(),
        });
        assert_eq!(d.tables(), vec!["asks".to_owned()]);
    }

    #[test]
    fn ddl_no_op_has_no_tables() {
        assert!(TypedStatement::DdlNoOp.tables().is_empty());
    }

    #[test]
    fn ddl_registered_has_no_tables_but_carries_template_id() {
        let stmt = TypedStatement::DdlRegistered(DdlTemplate {
            id: "create_did_registry".to_owned(),
            operation: DdlOperation::CreateTable,
        });
        assert!(stmt.tables().is_empty());
        assert_eq!(stmt.ddl_template_id(), Some("create_did_registry"));
    }

    #[test]
    fn non_ddl_variants_have_no_template_id() {
        assert!(TypedStatement::DdlNoOp.ddl_template_id().is_none());
        assert!(TypedStatement::Delete(SqlDelete {
            table: "x".to_owned()
        })
        .ddl_template_id()
        .is_none());
    }
}
