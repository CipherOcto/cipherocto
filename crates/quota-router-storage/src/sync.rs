//! Cipherocto-side sync subscription config (Phase C integration).
//!
//! Per [[stoolap-general-purpose-db]] + master plan §5 Storage Architecture note:
//! - The **fork** is a general-purpose DB and stays table-agnostic.
//! - The **cipherocto sync engine** (in `octo-sync` workspace, git dep) declares
//!   which tables to replicate via `DatabaseSyncAdapter` calls.
//!
//! This module is the cipherocto-side enum + default list that the sync engine
//! reads at startup to know "the cipherocto consumer schema is: Asks (today);
//! future tables added as we ship them." Adding a new cipherocto table means:
//! 1. New `cipherocto_schema_version` migration (cipherocto-side SQL).
//! 2. New entry in this enum + default list.
//! 3. New DAO methods in octo-core (or sibling crate).
//!
//! Fork integration: pass `ReplicatedTables::default().names()` to your
//! `StoolapAdapter` constructor when configuring the sync engine. The fork
//! reads cipherocto's tables out of the WAL automatically; this list is only
//! used for subscription / event filtering.

use serde::{Deserialize, Serialize};

/// Cipherocto-side tables that participate in sync (Phase C: Asks only).
///
/// Per [[stoolap-general-purpose-db]]: this enum lists cipherocto consumer
/// tables. It is NOT a list of engine/system tables (`_sys_*`) — those are
/// owned by the fork and never crossed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CipheroctoTable {
    /// `asks` table — per-node Ask pricing (RFC-0959 v1.0).
    Asks,
}

impl CipheroctoTable {
    /// On-disk table name (the SQL identifier).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Asks => "asks",
        }
    }
}

impl std::fmt::Display for CipheroctoTable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for CipheroctoTable {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "asks" => Ok(Self::Asks),
            other => Err(format!("unknown cipherocto table: {other}")),
        }
    }
}

/// Default replicated-tables subscription (Phase C).
///
/// Add new tables here when their migration lands + their DAO exists.
#[derive(Debug, Clone, Default)]
pub struct ReplicatedTables {
    tables: Vec<CipheroctoTable>,
}

impl ReplicatedTables {
    /// Default subscription: `asks` only (Phase C MVP).
    /// Add new tables as they ship.
    #[must_use]
    pub fn default_phase_c() -> Self {
        Self {
            tables: vec![CipheroctoTable::Asks],
        }
    }

    /// Custom subscription (e.g., for testing or feature-flagged rollouts).
    #[must_use]
    pub fn new(tables: Vec<CipheroctoTable>) -> Self {
        Self { tables }
    }

    /// List of cipherocto table names (passed to fork's `StoolapAdapter`
    /// when configuring the sync engine subscription).
    #[must_use]
    pub fn names(&self) -> Vec<&'static str> {
        self.tables.iter().map(|t| t.as_str()).collect()
    }

    /// Iterate cipherocto tables.
    pub fn iter(&self) -> impl Iterator<Item = CipheroctoTable> + '_ {
        self.tables.iter().copied()
    }

    /// Add a table to the subscription.
    pub fn add(&mut self, table: CipheroctoTable) {
        if !self.tables.contains(&table) {
            self.tables.push(table);
        }
    }

    /// Whether `table` is in this subscription.
    #[must_use]
    pub fn contains(&self, table: CipheroctoTable) -> bool {
        self.tables.contains(&table)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_phase_c_includes_asks() {
        let sub = ReplicatedTables::default_phase_c();
        assert!(sub.contains(CipheroctoTable::Asks));
        assert_eq!(sub.names(), vec!["asks"]);
    }

    #[test]
    fn table_as_str_roundtrip() {
        let t = CipheroctoTable::Asks;
        let s = t.as_str();
        let back: CipheroctoTable = s.parse().unwrap();
        assert_eq!(t, back);
    }

    #[test]
    fn unknown_table_rejected() {
        assert!("garbage".parse::<CipheroctoTable>().is_err());
    }

    #[test]
    fn add_idempotent() {
        let mut sub = ReplicatedTables::default_phase_c();
        sub.add(CipheroctoTable::Asks);
        sub.add(CipheroctoTable::Asks);
        assert_eq!(sub.tables.len(), 1);
    }

    #[test]
    fn display_format() {
        assert_eq!(CipheroctoTable::Asks.to_string(), "asks");
    }
}
