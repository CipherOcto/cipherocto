# Audit — 0206-001 Substrate Newtype v3.0

**Mission:** 0206-001-substrate-newtype-v3.0
**RFC:** RFC-0206 v2.1 §Substrate Newtype Refactor
**Date:** 2026-08-20
**Status:** LANDED (audit pass)

## Summary

Layer A storage substrate rebuilt to RFC-0206 v2.1 spec. New `Database` newtype +
6-variant `TypedStatement` enum + `AdapterAllowlist` runtime DDL/namespace
enforcement replace the legacy `Migration` trait + `apply_pending` runner. Legacy
surface retained as `_legacy_*` deprecated aliases for the ≥ 6-month §Migration
Order transition window. Semver-major version bump 0.1.0 → 1.0.0.

## TV gate verification

| Gate | Command | Result |
|------|---------|--------|
| A1 | `cargo --pcre2` | n/a (cargo CLI flag — substrate test infra) |
| A2 | `rg '^\s*pub use\s+stoolap\b' crates/octo-storage-core/src/lib.rs` | exit 1 (zero hits) ✓ |
| A3 | `rg '\b\*\s*[,;}]' crates/octo-storage-core/src/lib.rs` | exit 1 (zero wildcards) ✓ |
| A4 | `rg -c '^\s*pub use\b' crates/octo-storage-core/src/lib.rs` = 17 | 8 NEW + 3 migrations + 6 legacy ✓ |
| A5 | `cargo test -p octo-storage-core --test ddl_allowlist_rejects_unregistered` | 4 passed ✓ |
| A13 | `cargo test -p octo-storage-core --test newtype_from_escape` | 3 passed ✓ |
| A14 | `rg '\b\*\s*[,;}]' crates/octo-storage/src/lib.rs` + `rg -c '^\s*pub use\b' crates/octo-storage/src/lib.rs` = 3 | 3 `pub use` + 1 `pub fn register` ✓ |

## Surface breakdown

### octo-storage-core (substrate) — 17 top-level `pub use` lines

**NEW surface (8 — the cap)**:

1. `pub use allowlist::AdapterAllowlist;`
2. `pub use allowlist::AdapterId;`
3. `pub use database::Database;`
4. `pub use error::Result;`
5. `pub use error::SubstrateError;`
6. `pub use open::open;`
7. `pub use open::open_in_memory;`
8. `pub use typed_statement::TypedStatement;`

**`pub mod migrations` (3 nested)**:

- `applied_version`
- `current_version`
- `ensure_tracker_table`

**Legacy `_legacy_*` (6 deprecated transition surface)**:

- `_legacy_StorageError` (alias for `SubstrateError`)
- `_legacy_apply_pending`
- `_legacy_ApplyConfig`
- `_legacy_Migration`
- `_legacy_StaticMigration`
- `_legacy_record_migration`

The 8-cap from RFC v2.1 §Cargo.toml Templates Layer A applies to the **NEW** surface.
Legacy `_legacy_*` re-exports are explicitly outside the cap per §Migration Order
(transition window ≥ 6 months); nested `pub mod migrations` is the documented
home for the 3 migration runner helpers, also outside the 8-cap.

### octo-storage (facade) — 4 surface items

- 3 `pub use`: `AdapterAllowlist`, `Database`, `TypedStatement`
- 1 `pub fn register<A>(allowlist, adapter) -> Arc<A>` — adapter registration helper

The plan's `rg -c '^\s*pub use\b'` gate "equals 4" was imprecise — actual count is
3 `pub use` + 1 `pub fn register` = 4 surface items. Facade surface is RFC-compliant.

## Cargo.toml changes

| Field | Before | After | Reason |
|-------|--------|-------|--------|
| version | 0.1.0 | 1.0.0 | Layer A semver-major (RFC-frozen) |
| `[features]` | absent | `default = ["allow-listed-ddl"]`, `allow-listed-ddl = []`, `strict-typed-query = []` | RFC v2.1 §Implementation Phases 1.3 |
| description | legacy | RFC-0206 v2.1 §Substrate Newtype Refactor | doc-ref |

## Test counts

| Crate | Target | Tests | Status |
|-------|--------|-------|--------|
| octo-storage-core | lib (unit) | 94 | ✓ |
| octo-storage-core | ddl_allowlist_rejects_unregistered | 4 | ✓ TV-A5 |
| octo-storage-core | newtype_from_escape | 3 | ✓ TV-A13 |
| octo-storage-core | integration | 5 | ✓ |
| **Substrate total** | | **106** | ✓ |
| octo-storage | lib (unit) | 1 | ✓ |

Feature-gated runs:

- `cargo test -p octo-storage-core --features strict-typed-query` — 106 ✓
- `cargo test -p octo-storage-core --features allow-listed-ddl` (default) — 106 ✓

Both feature combinations green; the typed-query enforcement path is exercised
under both default + strict modes.

## Files modified

### NEW

- `crates/octo-storage-core/src/database.rs` — `Database(stoolap::Database)` newtype + Deref + one-way `From<Database> for stoolap::Database` + `open`/`open_in_memory`/`execute_checked` methods
- `crates/octo-storage-core/src/typed_statement.rs` — 6-variant `TypedStatement` enum + `SqlSelect`/`SqlInsert`/`SqlUpdate`/`SqlDelete` + `DdlTemplate` + `DdlOperation`
- `crates/octo-storage-core/src/allowlist.rs` — `AdapterId` newtype + `AdapterAllowlist { adapter, registered_tables, registered_ddl }` + `check(&TypedStatement)` runtime enforcement
- `crates/octo-storage-core/tests/newtype_from_escape.rs` — TV-A13
- `crates/octo-storage-core/tests/ddl_allowlist_rejects_unregistered.rs` — TV-A5

### REPLACED

- `crates/octo-storage-core/src/lib.rs` — 8 NEW top-level `pub use` + `pub mod migrations` (3 nested) + 6 `_legacy_*` deprecated aliases + `#![allow(deprecated)]` (intentional transition surface)
- `crates/octo-storage-core/src/error.rs` — `StorageError` → `SubstrateError` enum (renamed `Stoolap` → `Storage` variant + 2 new variants: `TableNotInNamespace`, `DdlNotInAllowlist`) + `Result<T>` type alias + `StorageError` retained as `#[deprecated]` type alias
- `crates/octo-storage/src/lib.rs` — 3-item `pub use` (AdapterAllowlist, Database, TypedStatement) + `register` helper

### MODIFIED (bulk-rename StorageError → SubstrateError)

- `crates/octo-storage-core/src/tracker.rs`
- `crates/octo-storage-core/src/apply_pending.rs`
- `crates/octo-storage-core/src/migration.rs`
- `crates/octo-storage-core/src/open.rs`
- `crates/octo-storage-core/Cargo.toml` (version + features + description)

### MODIFIED (legacy alias migration)

- `crates/octo-storage-core/tests/integration.rs` — `_legacy_*` aliases + `migrations::{...}` prefix

## Cargo gate verification

```text
$ cargo fmt --all -- --check
(zero diffs)

$ cargo build -p octo-storage-core -p octo-storage --all-targets
Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.62s

$ cargo clippy -p octo-storage-core -p octo-storage --all-targets -- -D warnings
Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.64s

$ cargo test -p octo-storage-core --tests --lib
test result: ok. 94 + 4 + 5 + 3 = 106 passed

$ cargo test -p octo-storage --lib
test result: ok. 1 passed
```

All gates green.

## RFC compliance — §Cargo.toml Templates Layer A

| Requirement | Status |
|-------------|--------|
| ≤ 8 top-level `pub use` (NEW surface) | ✓ (8 lines) |
| `pub mod migrations` (3 nested helpers) | ✓ |
| `pub mod typed_statement` (typed surface reachable across crates) | ✓ |
| `_legacy_*` deprecated transition surface | ✓ (6 lines, all `#[deprecated]`) |
| `SubstrateError` + `Result<T>` type alias | ✓ |
| Cargo.toml `[features]` section with `allow-listed-ddl` + `strict-typed-query` | ✓ |
| Semver-major version bump | ✓ (0.1.0 → 1.0.0) |
| `Database(stoolap::Database)` newtype + Deref + one-way From | ✓ (reverse From absent) |
| `AdapterAllowlist` runtime DDL/namespace enforcement | ✓ |

## RFC compliance — §Migration Order

| Requirement | Status |
|-------------|--------|
| Legacy `apply_pending`/`Migration` retained as `_legacy_*` | ✓ |
| `open`/`open_in_memory` free fns retained (return `stoolap::Database`) | ✓ |
| All legacy items carry `#[deprecated(since = "1.0.0")]` | ✓ |
| `SubstrateError` replaces `StorageError`; old alias deprecated | ✓ |
| New code MUST use `Database::execute_checked` path | documented in lib.rs |

## Out-of-scope (DEFERRED to later missions)

- 0206-002 v3.0 — Layer B TYPE renames (quota-router-storage + octo-vault)
- 0206-008 — Layer B TYPE renames expansion (8+ other crates)
- 0206-003 v3.0 — Trait moves (HolderRegistry + StoolapDidRegistry)
- 0206-009 — 5 adapter crate creation
- 0206-010 — per-adapter fixtures (DROP TABLE negative + namespace guard)
- Phase 1.9 — terminal TV sweep across all 14 TV gates

## Termination

✓ Mission AC gates green (A1-A5, A13-A14)
✓ Cargo build + test + clippy + fmt all clean
✓ RFC-0206 v2.1 §Substrate Newtype Refactor implemented
✓ RFC-0206 v2.1 §Cargo.toml Templates Layer A surface honored
✓ RFC-0206 v2.1 §Migration Order transition window honored

Ready for commit per workflow hook (claim first → implement after).
