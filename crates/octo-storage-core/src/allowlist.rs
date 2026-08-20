//! [`AdapterAllowlist`] — runtime DDL + namespace enforcement.
//!
//! Per RFC-0206 v2.1 §Format Bypass Defense, the substrate refuses to
//! dispatch typed queries or DDL unless they are pre-registered for the
//! calling adapter. Each adapter crate owns an `AdapterAllowlist`
//! instance built at startup; the substrate enforces the contract at
//! every `Database::execute_checked` call.

use std::collections::HashSet;

use crate::error::{Result, SubstrateError};
use crate::typed_statement::{DdlTemplate, TypedStatement};

/// Stable identifier for an adapter crate (e.g. `"octo-vault"`,
/// `"octo-reputation"`). Surfaced through
/// [`SubstrateError::TableNotInNamespace`] and
/// [`SubstrateError::DdlNotInAllowlist`] so operators can diagnose
/// allowlist drift.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AdapterId(String);

impl AdapterId {
    /// Construct an `AdapterId` from any `Into<String>`. The substrate
    /// does not validate the string format — adapter crates pick a
    /// stable, human-readable id at crate construction time.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Borrow the underlying identifier string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for AdapterId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Per-adapter allowlist. Owns:
/// - the set of tables the adapter is permitted to read/write,
/// - the set of pre-registered DDL templates the adapter may dispatch.
///
/// The substrate's [`Database::execute_checked`](crate::Database::execute_checked)
/// method calls [`Self::check`] before forwarding any statement to
/// Stoolap.
#[derive(Debug, Clone)]
pub struct AdapterAllowlist {
    /// Adapter this allowlist applies to (surfaced in error messages).
    adapter: AdapterId,
    /// Tables the adapter may target via typed queries.
    registered_tables: HashSet<String>,
    /// DDL templates the adapter may dispatch (matched by
    /// [`DdlTemplate::id`]).
    registered_ddl: Vec<DdlTemplate>,
}

impl AdapterAllowlist {
    /// Construct an empty allowlist for `adapter`. The adapter crate
    /// then calls [`Self::register_table`] + [`Self::register_ddl`]
    /// for every namespace entry it owns.
    pub fn new(adapter: AdapterId) -> Self {
        Self {
            adapter,
            registered_tables: HashSet::new(),
            registered_ddl: Vec::new(),
        }
    }

    /// Construct an allowlist pre-populated with `tables` + `ddl`.
    /// Convenience for adapters whose registration set is known at
    /// compile time (the common case).
    pub fn with_registrations(
        adapter: AdapterId,
        tables: impl IntoIterator<Item = String>,
        ddl: impl IntoIterator<Item = DdlTemplate>,
    ) -> Self {
        let mut out = Self::new(adapter);
        for t in tables {
            out.register_table(t);
        }
        for d in ddl {
            out.register_ddl(d);
        }
        out
    }

    /// Register a single table name.
    pub fn register_table(&mut self, table: impl Into<String>) {
        self.registered_tables.insert(table.into());
    }

    /// Register a single DDL template. Order is preserved (the
    /// [`Self::check`] path matches by `id`, not by position, so order
    /// only affects `Debug` output).
    pub fn register_ddl(&mut self, template: DdlTemplate) {
        self.registered_ddl.push(template);
    }

    /// Borrow the adapter id.
    pub fn adapter(&self) -> &AdapterId {
        &self.adapter
    }

    /// Borrow the registered table set.
    pub fn tables(&self) -> &HashSet<String> {
        &self.registered_tables
    }

    /// Borrow the registered DDL template list.
    pub fn ddl(&self) -> &[DdlTemplate] {
        &self.registered_ddl
    }

    /// Check `stmt` against this allowlist.
    ///
    /// - [`TypedStatement::Select`](crate::typed_statement::TypedStatement::Select) /
    ///   [`Insert`](crate::typed_statement::TypedStatement::Insert) /
    ///   [`Update`](crate::typed_statement::TypedStatement::Update) /
    ///   [`Delete`](crate::typed_statement::TypedStatement::Delete):
    ///   every referenced table must be present in
    ///   [`Self::registered_tables`].
    /// - [`TypedStatement::DdlNoOp`](crate::typed_statement::TypedStatement::DdlNoOp):
    ///   always allowed (the substrate treats idempotent no-ops as
    ///   free to dispatch).
    /// - [`TypedStatement::DdlRegistered`](crate::typed_statement::TypedStatement::DdlRegistered):
    ///   the [`DdlTemplate::id`] must match a template registered with
    ///   [`Self::register_ddl`].
    pub fn check(&self, stmt: &TypedStatement) -> Result<()> {
        for table in stmt.tables() {
            if !self.registered_tables.contains(&table) {
                return Err(SubstrateError::TableNotInNamespace {
                    adapter: self.adapter.to_string(),
                    table,
                });
            }
        }
        if let Some(template_id) = stmt.ddl_template_id() {
            if !self.registered_ddl.iter().any(|t| t.id == template_id) {
                return Err(SubstrateError::DdlNotInAllowlist {
                    adapter: self.adapter.to_string(),
                    template: template_id.to_owned(),
                });
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::typed_statement::{
        DdlOperation, DdlTemplate, SqlDelete, SqlInsert, SqlSelect, SqlUpdate,
    };

    fn vault_allowlist() -> AdapterAllowlist {
        AdapterAllowlist::with_registrations(
            AdapterId::new("octo-vault"),
            ["vault_keys".to_owned()],
            [DdlTemplate {
                id: "create_vault_keys".to_owned(),
                operation: DdlOperation::CreateTable,
            }],
        )
    }

    #[test]
    fn registered_table_passes() {
        let allowlist = vault_allowlist();
        let stmt = TypedStatement::Select(SqlSelect {
            tables: vec!["vault_keys".to_owned()],
        });
        allowlist.check(&stmt).unwrap();
    }

    #[test]
    fn unregistered_table_fails() {
        let allowlist = vault_allowlist();
        let stmt = TypedStatement::Insert(SqlInsert {
            table: "shadow_table".to_owned(),
        });
        let err = allowlist.check(&stmt).unwrap_err();
        match err {
            SubstrateError::TableNotInNamespace { adapter, table } => {
                assert_eq!(adapter, "octo-vault");
                assert_eq!(table, "shadow_table");
            }
            other => panic!("expected TableNotInNamespace, got {other:?}"),
        }
    }

    #[test]
    fn join_with_unregistered_table_fails() {
        let allowlist = vault_allowlist();
        let stmt = TypedStatement::Select(SqlSelect {
            tables: vec!["vault_keys".to_owned(), "rogue_table".to_owned()],
        });
        let err = allowlist.check(&stmt).unwrap_err();
        assert!(matches!(err, SubstrateError::TableNotInNamespace { .. }));
    }

    #[test]
    fn ddl_no_op_always_allowed() {
        let allowlist = vault_allowlist();
        allowlist.check(&TypedStatement::DdlNoOp).unwrap();
    }

    #[test]
    fn registered_ddl_template_passes() {
        let allowlist = vault_allowlist();
        let stmt = TypedStatement::DdlRegistered(DdlTemplate {
            id: "create_vault_keys".to_owned(),
            operation: DdlOperation::CreateTable,
        });
        allowlist.check(&stmt).unwrap();
    }

    #[test]
    fn unregistered_ddl_template_fails() {
        let allowlist = vault_allowlist();
        let stmt = TypedStatement::DdlRegistered(DdlTemplate {
            id: "drop_vault_keys".to_owned(),
            operation: DdlOperation::Drop,
        });
        let err = allowlist.check(&stmt).unwrap_err();
        match err {
            SubstrateError::DdlNotInAllowlist { adapter, template } => {
                assert_eq!(adapter, "octo-vault");
                assert_eq!(template, "drop_vault_keys");
            }
            other => panic!("expected DdlNotInAllowlist, got {other:?}"),
        }
    }

    #[test]
    fn update_and_delete_check_table_namespace() {
        let allowlist = vault_allowlist();
        allowlist
            .check(&TypedStatement::Update(SqlUpdate {
                table: "vault_keys".to_owned(),
            }))
            .unwrap();
        allowlist
            .check(&TypedStatement::Delete(SqlDelete {
                table: "vault_keys".to_owned(),
            }))
            .unwrap();

        let err = allowlist
            .check(&TypedStatement::Delete(SqlDelete {
                table: "other_table".to_owned(),
            }))
            .unwrap_err();
        assert!(matches!(err, SubstrateError::TableNotInNamespace { .. }));
    }

    #[test]
    fn register_table_extends_existing_set() {
        let mut allowlist = vault_allowlist();
        allowlist.register_table("audit_log");
        let stmt = TypedStatement::Insert(SqlInsert {
            table: "audit_log".to_owned(),
        });
        allowlist.check(&stmt).unwrap();
    }

    #[test]
    fn adapter_id_display_round_trip() {
        let id = AdapterId::new("octo-vault");
        assert_eq!(id.as_str(), "octo-vault");
        assert_eq!(format!("{id}"), "octo-vault");
    }
}
