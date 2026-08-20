//! `pub mod stoolap` re-export block per RFC-0206 v2.2 §Substrate Re-export Block.
//!
//! 1:1 aliases for `stoolap` types consumers need to type row-decoding
//! code returned by `Database::execute_checked`. Consumers use
//! `use octo_storage_core::stoolap::{ResultRow, ApiTransaction, Rows, Error, Value}`
//! instead of taking a direct `stoolap` Cargo.toml dep, preserving
//! the substrate as the abstraction layer per CLAUDE.md §Core Engineering
//! Principles ("no parallel abstractions").
//!
//! **Deliberately excludes `stoolap::Database`** — the inner type behind
//! the `Database` newtype. Reaching for the raw type via the re-export
//! block would defeat the newtype abstraction; consumers must use
//! `octo_storage_core::Database` + the `From<Database>` escape hatch
//! (substrate-internal usage) per §Substrate Newtype Refactor.

// `pub use` re-exports — clippy unused_imports false-positive: these
// are intentional public re-exports without internal consumers; they
// MUST stay reachable for downstream crates to `use
// octo_storage_core::stoolap::{...}`.
#![allow(unused_imports)]

pub use stoolap::core::Error;
// NB: stoolap's top-level `Transaction` re-export (lib.rs line 144,
// `pub use storage::{..., Transaction, ...}`) binds to the
// `storage::Transaction` *trait* (storage/traits/transaction.rs:48),
// not the `api::Transaction` *struct* (api/transaction.rs:56) that
// consumers actually want. The struct is exposed at the top level as
// `stoolap::ApiTransaction` (lib.rs line 192,
// `pub use api::{..., Transaction as ApiTransaction, ...}`). Re-export
// the latter so `octo_storage_core::stoolap::ApiTransaction` is usable
// as a type in consumer function signatures.
pub use stoolap::ApiTransaction;
pub use stoolap::ResultRow;
pub use stoolap::Rows;
pub use stoolap::Value;
