//! Layer A storage substrate — RFC-0206 v2.2 §Substrate Newtype Refactor + §Substrate Re-export Block.
//!
//! Per the Layer A stability principle (CLAUDE.md), this crate is
//! RFC-frozen: any change to the public surface requires a semver-major
//! version bump + an RFC amendment.
//!
//! ## Surface (8 top-level pub-use + pub mod migrations)
//!
//! Per RFC-0206 v2.1 §Cargo.toml Templates Layer A, the substrate
//! exposes exactly **8 top-level `pub use` statements** plus a
//! `pub mod migrations` module (3 nested pub-use for the migration
//! runner helpers). The legacy `Migration` trait + `apply_pending`
//! runner are retained as `_legacy_*` re-exports per §Migration Order
//! for the ≥ 6-month transition window.

#![forbid(unsafe_code)]
#![warn(missing_docs)]
// All `#[deprecated]` warnings in this file are intentional: the
// `_legacy_*` re-exports below advertise the legacy surface for the
// ≥ 6-month §Migration Order transition window.
#![allow(deprecated)]

// Internal modules. `pub(crate)` modules are accessible to `_legacy_*`
// re-exports below; `pub` modules expose the typed query structs to
// downstream crates (adapters construct `TypedStatement::Select(SqlSelect
// { ... })` etc., so the underlying struct types must be reachable
// across crate boundaries). The public surface is the 8 top-level
// `pub use` statements + `pub mod migrations` + `pub mod typed_statement`
// + the `_legacy_*` deprecated aliases.
pub(crate) mod allowlist;
pub(crate) mod apply_pending;
pub(crate) mod database;
pub(crate) mod error;
pub(crate) mod migration;
pub(crate) mod open;
pub(crate) mod sql_split;
pub(crate) mod tracker;
pub mod typed_statement;

/// Default tracker table name (`schema_migrations`).
pub const DEFAULT_TRACKER_TABLE: &str = "schema_migrations";

// === 8 top-level `pub use` statements per RFC v2.1 §Cargo.toml Templates Layer A ===
// Count: Database, TypedStatement, AdapterAllowlist, AdapterId, SubstrateError,
// Result, open, open_in_memory = 8 statements.
//
// (The 9th `pub use crate::error::StorageError;` is intentionally absent —
// `StorageError` is a deprecated type alias in `error.rs`, NOT a top-level
// re-export. Consumers using the legacy alias must `use octo_storage_core::error::StorageError`
// explicitly; this surface deliberately does not promote the alias to top level
// to prevent silent adoption of the deprecated form.)

/// Per-adapter namespace + DDL allowlist.
pub use allowlist::AdapterAllowlist;
/// Stable adapter identifier (e.g. `"octo-vault"`).
pub use allowlist::AdapterId;
/// Newtype wrapping `stoolap::Database` + the typed
/// `Database::execute_checked` execution path.
pub use database::Database;
/// Canonical substrate `Result` alias.
pub use error::Result;
/// Canonical substrate error type.
pub use error::SubstrateError;
/// Open a persistent `Database` at `path`.
pub use open::open;
/// Open an ephemeral in-memory `Database`.
pub use open::open_in_memory;
/// Typed SQL surface (6-variant enum + SqlSelect/Insert/Update/Delete +
/// DdlTemplate + DdlOperation).
pub use typed_statement::TypedStatement;

// === `pub mod migrations` (3 nested pub-use per RFC v2.1) ===

/// Migration runner helpers — `ensure_tracker_table`, `current_version`,
/// `applied_version`. Substrate-private modules expose these; this
/// `pub mod` surface collects the 3 helpers as nested pub-use per
/// §Cargo.toml Templates Layer A.
pub mod migrations {
    pub use crate::tracker::applied_version;
    pub use crate::tracker::current_version;
    pub use crate::tracker::ensure_tracker_table;
}

// === `pub mod stoolap` (5 nested pub-use per RFC v2.2 §Substrate Re-export Block) ===
//
// v2.3 grows 5 → 6 (`DataType`); v2.4 adds `pub mod pubsub` (7 nested
// re-exports + nested `pub mod wal_pubsub`). 8 top-level `pub use` cap
// UNCHANGED across all amendments (re-export block is `pub mod`, not
// top-level `pub use`).
//
// 5 nested `pub use stoolap::*` re-exports so consumer crates can
// `use octo_storage_core::stoolap::{ResultRow, ApiTransaction, Rows,
// Error, Value}` instead of taking a direct `stoolap` Cargo.toml dep.
// Deliberately excludes `stoolap::Database` (the inner type behind
// the `Database` newtype — reverse escape hatch substrate prevents).
// 8 top-level `pub use` cap UNCHANGED (re-export block is `pub mod`,
// not 5 top-level `pub use`).
pub mod stoolap;

// === Legacy `_legacy_*` re-exports per §Migration Order ===
//
// Per RFC-0206 v2.1 §Migration Order, the pre-substrate surface
// (free-function `open`/`open_in_memory` returning `stoolap::Database`
// directly + the `Migration` trait + `apply_pending` runner) is
// retained for ≥ 6 months under `_legacy_*` aliases. **New code MUST
// use the `Database` newtype + `Database::execute_checked` path.**

/// Legacy error type alias (deprecated; use `SubstrateError` instead).
#[allow(deprecated)]
#[deprecated(
    since = "2.0.0",
    note = "use `SubstrateError` from the top-level surface; this legacy alias will be removed in v3.0"
)]
pub use error::StorageError as _legacy_StorageError;

/// Legacy apply-pending free function. Prefer
/// `migrations::ensure_tracker_table` + `Database::execute_checked`.
#[deprecated(
    since = "2.0.0",
    note = "use the typed `Database::execute_checked` path; this legacy runner will be removed in v3.0"
)]
pub use apply_pending::apply_pending as _legacy_apply_pending;

/// Legacy migration config struct.
#[deprecated(
    since = "2.0.0",
    note = "use the typed `Database::execute_checked` path; this legacy config will be removed in v3.0"
)]
pub use apply_pending::ApplyConfig as _legacy_ApplyConfig;

/// Legacy `Migration` trait (substrate-side; replaced by `TypedStatement`).
#[deprecated(
    since = "2.0.0",
    note = "use `TypedStatement`; this legacy trait will be removed in v3.0"
)]
pub use migration::Migration as _legacy_Migration;

/// Legacy `StaticMigration` zero-erased newtype.
#[deprecated(
    since = "2.0.0",
    note = "use `TypedStatement`; this legacy newtype will be removed in v3.0"
)]
pub use migration::StaticMigration as _legacy_StaticMigration;

/// Legacy `record_migration` helper.
#[deprecated(
    since = "2.0.0",
    note = "use the typed `Database::execute_checked` path; this legacy helper will be removed in v3.0"
)]
pub use tracker::record_migration as _legacy_record_migration;

#[cfg(test)]
mod tests {
    #[test]
    fn substrate_surface_compiles() {
        // Smoke test: every top-level type is reachable.
        let _db: crate::Result<crate::Database> = crate::Database::open_in_memory();
    }
}
