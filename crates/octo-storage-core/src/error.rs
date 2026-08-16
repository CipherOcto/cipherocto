//! [`StorageError`] — the single error type surfaced from this crate.
//!
//! Per `cipherocto-design-principles` §Layer A row, this enum is
//! RFC-frozen. Variants may be added in semver-minor; existing variants
//! never rename without a semver-major bump.

use thiserror::Error;

/// Errors from migration + database-construction operations in this crate.
#[derive(Debug, Error)]
pub enum StorageError {
    /// Wraps a `stoolap::Database` operation failure (execute / query / open).
    #[error("stoolap error during {operation}: {message}")]
    Stoolap {
        /// Which operation produced the failure (`"execute"`, `"open_in_memory"`, etc.).
        operation: &'static str,
        /// Stoolap's error message verbatim.
        message: String,
    },

    /// A registered migration's SQL failed to apply.
    #[error("migration version {version} ({name}) failed: {message}")]
    MigrationFailed {
        /// Migration version that failed.
        version: u32,
        /// Migration name (`Migration::name()`).
        name: &'static str,
        /// SQL statement + stoolap error.
        message: String,
    },

    /// Catalog has migrations only up to `catalog_max`, but the DB records a higher
    /// applied version (downgrade scenario or unknown migration recorded by older code).
    #[error(
        "database is at migration version {version}, but catalog max is {catalog_max} \
             (likely a downgrade — check code version or roll forward)"
    )]
    UnknownMigration {
        /// Version recorded in DB.
        version: u32,
        /// Highest version this code's catalog exposes.
        catalog_max: u32,
    },

    /// Wall-clock failure (system time before UNIX_EPOCH).
    #[error("system time error: {0}")]
    SystemTime(String),

    /// Operation requested by an owner crate is not supported by this substrate.
    #[error("unsupported: {0}")]
    Unsupported(String),
}

impl StorageError {
    /// Wrap a stoolap error with an operation tag.
    pub fn stoolap(operation: &'static str, e: impl std::fmt::Display) -> Self {
        Self::Stoolap {
            operation,
            message: e.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_surfaces_all_variants() {
        let e1 = StorageError::stoolap("execute", "duplicate column foo");
        assert!(e1.to_string().contains("execute"));
        assert!(e1.to_string().contains("duplicate column foo"));

        let e2 = StorageError::MigrationFailed {
            version: 7,
            name: "create_x",
            message: "syntax error".to_owned(),
        };
        let s = e2.to_string();
        assert!(s.contains("version 7"));
        assert!(s.contains("create_x"));
        assert!(s.contains("syntax error"));

        let e3 = StorageError::UnknownMigration {
            version: 12,
            catalog_max: 8,
        };
        let s = e3.to_string();
        assert!(s.contains("12"));
        assert!(s.contains("8"));

        let e4 = StorageError::SystemTime("epoch".to_owned());
        assert!(e4.to_string().contains("epoch"));

        let e5 = StorageError::Unsupported("nope".to_owned());
        assert!(e5.to_string().contains("nope"));
    }

    #[test]
    fn debug_is_identifiable() {
        let e = StorageError::stoolap("open", "no such file");
        let dbg = format!("{e:?}");
        assert!(dbg.contains("Stoolap"));
        assert!(dbg.contains("open"));
    }
}
