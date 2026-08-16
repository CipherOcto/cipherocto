//! Layer A storage substrate for the cipherocto workspace.
//!
//! Owns two responsibilities:
//!
//! 1. **Migration runner.** The [`apply_pending`] function unifies three
//!    historically divergent owner-crate APIs (`quota-router-storage`,
//!    `octo-reputation`, `quota-router-sm-engine`) onto a single trait +
//!    version-tracking table. Owner crates depend on this crate instead
//!    of writing bespoke runners.
//!
//! 2. **Database constructors.** [`open`] and [`open_in_memory`] wrap the
//!    fork's [`stoolap::Database`] constructors and surface errors via
//!    this crate's [`StorageError`], so owner crates have one error type
//!    for "storage failed" without importing stoolap types directly.
//!
//! ## Layer model
//!
//! Per `cipherocto-design-principles` Layer A, this crate is
//! **RFC-frozen, semver-major only**. Every public type carries a
//! `#[doc = "..."]` linking the governing RFC.
//!
//! ## Stability contract
//!
//! - `Migration` trait shape: years-stable. New fields require a new
//!   version of the trait OR a separate `MigrationV2`.
//! - Tracker-table DDL: years-stable. Bumping the schema is a migration
//!   applied to every owner DB via `apply_pending` itself.
//! - Error variants may be **added** in semver-minor (no breaking);
//!   existing variants **never** rename without a semver-major.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod apply_pending;
mod error;
mod migration;
mod open;
mod sql_split;
mod tracker;

/// Tracker-table default. Picked to match the majority pre-substrate convention
/// (`octo-reputation` + `quota-router-sm-engine`); `quota-router-storage` will
/// migrate to this name as part of the S2 owner-crate migration.
///
/// See mission `octo-storage-split` §DP-1.
pub const DEFAULT_TRACKER_TABLE: &str = "schema_migrations";

pub use apply_pending::{apply_pending, ApplyConfig};
pub use error::StorageError;
pub use migration::{Migration, StaticMigration};
pub use open::{open, open_in_memory};
pub use tracker::{applied_version, current_version, ensure_tracker_table, record_migration};
