---
name: 0206-001-substrate-newtype
description: Open 2026-08-20 v3.0; RFC-0206 v2.1 §Substrate Newtype Refactor — create Database newtype + TypedStatement 6-variant enum + AdapterAllowlist + register helper + [features] section + facade migration. Closes TV-0206-A1..A5, A13, A14. Supersedes v2.1 (substrate-newtype body only; facade migration per R2 CRIT 3).
metadata:
  node_type: mission
  type: project
  originSessionId: 9a316ae1-cb15-46f4-801f-99c2545bccf7
  created: 2026-08-20T00:00:00.000Z
  v: "3.0"
  supersedes: v2.1
  depends_on:
    - 0206-011-rfc-0206-v21-amendment
    - RFC-0206 v2.1
---

# Mission `0206-001-substrate-newtype` v3.0 — OPEN 2026-08-20

## v3.0 Changes from v2.1

RFC-0206 v2.1 amendment (`0206-011`) re-baselines substrate spec:

- **8-pub-use cap** per RFC §Cargo.toml Templates Layer A v2.1: Database + TypedStatement + AdapterAllowlist + AdapterId + SubstrateError + Result + open/open_in_memory + DEFAULT_TRACKER_TABLE = 8 top-level `pub use` statements + `pub mod migrations` (3 nested pub-use for `ensure_tracker_table`, `current_version`, `applied_version`)
- **Facade migration** per R2 CRIT 3: `crates/octo-storage/src/lib.rs` becomes 4-item re-export (Database + TypedStatement + AdapterAllowlist + register) per RFC §Cargo.toml Templates Layer B v2.1; v2.1 mission handed facade ownership to this mission per R2 closure
- **§Migration Order coexistence**: per RFC v2.1 §Migration Order, substrate retains BOTH legacy (`_legacy_apply_pending`, `_legacy_StorageError`, etc.) + new API during ≥ 6-month transition window
- **§Escape Hatch Enumeration**: substrate internal API + 5 adapter crates use `From<Database> for stoolap::Database` legitimately; 11 consumer crates MUST NOT (renamed to `octo_storage_core::Database` in `0206-002 v3.0` + `0206-008`)

## v2.1 Changes from v2.0

R2 findings applied:

- 4 decomposed AC gates summing to 11 types: `Database` (newtype + Deref + one-way From) + `TypedStatement` (6-variant enum) + `SqlSelect`/`SqlInsert`/`SqlUpdate`/`SqlDelete` (table-typed query structs) + `DdlTemplate` + `DdlOperation` + `AdapterAllowlist` + `AdapterId` + `SubstrateError` + `Result<T>` + `register<V: VaultStore>`
- RFC-0206 v2.0 Accepted precondition: depends_on RFC-0206 (already satisfied per memory card `rfc-0205-0206-r9-v20-status.md`)
- `AdapterId` visibility added (NEW type from RFC §Substrate Newtype Refactor)

## v2.0 Changes from v1.0

R1 wholesale rewrite: 4 decomposed AC gates summing to 11 types per RFC §Substrate Newtype Refactor

## Scope

Create Layer A substrate body per RFC-0206 v2.1 §Substrate Newtype Refactor + facade migration per §Cargo.toml Templates Layer B. Substrate redesign = Layer A semver-major change.

### New files

- `crates/octo-storage-core/src/database.rs` — `Database(stoolap::Database)` newtype + `Deref<Target = stoolap::Database>` + one-way `From<Database> for stoolap::Database` (NOT reverse) + `open(path) -> Result<Self, SubstrateError>` + `open_in_memory() -> Result<Self, SubstrateError>` + `execute_checked(adapter_id: AdapterId, stmt: &TypedStatement) -> Result<(), SubstrateError>` + legacy `_legacy_open(path) -> Result<stoolap::Database, SubstrateError>` + `_legacy_open_in_memory() -> Result<stoolap::Database, SubstrateError>` per §Migration Order
- `crates/octo-storage-core/src/typed_statement.rs` — `TypedStatement` enum (6 variants: `Select(SqlSelect)`, `Insert(SqlInsert)`, `Update(SqlUpdate)`, `Delete(SqlDelete)`, `DdlNoOp`, `DdlRegistered(DdlTemplate)`) + `SqlSelect`/`SqlInsert`/`SqlUpdate`/`SqlDelete` structs (each with `tables() -> Vec<String>` method) + `DdlTemplate` struct + `DdlOperation` enum
- `crates/octo-storage-core/src/allowlist.rs` — `AdapterAllowlist` struct (`registered_tables: HashSet<String>`, `registered_ddl: Vec<DdlTemplate>`) + `AdapterId` newtype (wraps `String`) + `AdapterAllowlist::new(tables: HashSet<String>, ddl: Vec<DdlTemplate>) -> Self` + `check(&self, stmt: &TypedStatement) -> Result<(), SubstrateError>` (matches DDL + checks tables for typed queries) + `register_table(&mut self, table: String)` + `register_ddl(&mut self, template: DdlTemplate)`
- `crates/octo-storage-core/src/error.rs` — REPLACE existing `StorageError` with `SubstrateError` (new variants: `DdlNotInAllowlist { template: String }`, `TableNotInNamespace { table: String }`, `Storage { source: stoolap::Error }` etc.) + `StorageError` retained as deprecated type alias for backward-compat per §Migration Order + `Result<T>` type alias

### Modify files

- `crates/octo-storage-core/src/lib.rs` — restructure re-export surface:
  - **8 top-level `pub use`** per RFC v2.1 §Cargo.toml Templates Layer A: `Database`, `TypedStatement`, `AdapterAllowlist`, `AdapterId`, `SubstrateError`, `Result`, `open`/`open_in_memory` (2 items from `crate::open::*`), `DEFAULT_TRACKER_TABLE` (counts as 1 const item, not pub-use statement)
  - **`pub mod migrations`** with 3 nested pub-use: `ensure_tracker_table`, `current_version`, `applied_version` (moved from top-level tracker re-exports)
  - **Legacy `_legacy_*` re-exports** per §Migration Order: `_legacy_apply_pending`, `_legacy_StorageError`, `_legacy_Migration`, `_legacy_StaticMigration`, `_legacy_ApplyConfig`, `_legacy_record_migration` (deprecated aliases for ≥ 6-month transition)
- `crates/octo-storage-core/Cargo.toml` — ADD `[features]` section: `default = ["allow-listed-ddl"]`, `allow-listed-ddl = []`, `strict-typed-query = []`; semver-major version bump `0.1.0` → `1.0.0` (Layer A RFC-frozen, semver-major only per CLAUDE.md)
- `crates/octo-storage/src/lib.rs` — facade migration per RFC v2.1 §Cargo.toml Templates Layer B: 4-item re-export (Database, TypedStatement, AdapterAllowlist, register) + drop legacy re-exports (apply_pending, open, open_in_memory, ApplyConfig, Migration, StaticMigration, StorageError, DEFAULT_TRACKER_TABLE); keep `#[cfg(test)] mod tests { facade_round_trips_substrate_surface }` test updated to reference new symbols

### Tests

- `crates/octo-storage-core/tests/newtype_from_escape.rs` — TV-0206-A13: construct `Database`, call `.into()`, get `stoolap::Database`, run `execute()`
- `crates/octo-storage-core/tests/ddl_allowlist_rejects_unregistered.rs` — TV-0206-A5: typed query against unregistered table → `SubstrateError::DdlNotInAllowlist`

## Acceptance Criterion

- `crates/octo-storage-core/src/{database,typed_statement,allowlist}.rs` exist with non-empty body
- `crates/octo-storage-core/src/error.rs` defines `SubstrateError` enum + `Result<T>` type alias + `StorageError` deprecated alias
- `crates/octo-storage-core/src/lib.rs` has exactly **8 top-level `pub use` statements** + `pub mod migrations` (verified via `rg -c '^\s*pub use\b' crates/octo-storage-core/src/lib.rs` ≤ 8)
- `crates/octo-storage-core/src/lib.rs` has NO `pub use stoolap::Database` (verified via `rg '^\s*pub use\s+stoolap\b' crates/octo-storage-core/src/lib.rs` exits 1) — TV-A2
- `crates/octo-storage-core/src/lib.rs` has NO `pub use foo::*;` wildcards (verified via `rg '\b\*\s*[,;}]' crates/octo-storage-core/src/lib.rs` exits 1) — TV-A3
- `crates/octo-storage-core/src/lib.rs` has legacy `_legacy_*` re-exports for backward-compat per §Migration Order
- `crates/octo-storage-core/Cargo.toml` has `[features] default = ["allow-listed-ddl"]`, `allow-listed-ddl = []`, `strict-typed-query = []` (verified via `rg '^\[features\]' crates/octo-storage-core/Cargo.toml` exits 0)
- `crates/octo-storage-core/Cargo.toml` `version = "1.0.0"` (semver-major bump)
- `crates/octo-storage/src/lib.rs` has exactly **4-item re-export**: Database, TypedStatement, AdapterAllowlist, register (verified via `rg -c '^\s*pub use\b' crates/octo-storage/src/lib.rs` equals 4) — TV-A14
- `crates/octo-storage/src/lib.rs` has NO `pub use foo::*;` wildcards — TV-A14
- `crates/octo-storage-core/tests/newtype_from_escape.rs` passes — TV-A13
- `crates/octo-storage-core/tests/ddl_allowlist_rejects_unregistered.rs` passes — TV-A5
- `cargo build -p octo-storage-core -p octo-storage` green
- `cargo test -p octo-storage-core --tests --lib` green (existing tests + 2 new)
- `cargo test -p octo-storage --lib` green (facade round-trip test updated)
- `cargo clippy -p octo-storage-core -p octo-storage --all-targets --features full -- -D warnings` green
- `cargo fmt --all -- --check` green

## Files / Artifacts

- NEW: `crates/octo-storage-core/src/database.rs`
- NEW: `crates/octo-storage-core/src/typed_statement.rs`
- NEW: `crates/octo-storage-core/src/allowlist.rs`
- MODIFY: `crates/octo-storage-core/src/error.rs` (REPLACE StorageError with SubstrateError + deprecated alias)
- MODIFY: `crates/octo-storage-core/src/lib.rs` (8-pub-use + pub mod migrations + _legacy_* re-exports)
- MODIFY: `crates/octo-storage-core/Cargo.toml` ([features] section + semver-major version bump)
- MODIFY: `crates/octo-storage/src/lib.rs` (4-item re-export + register + update round-trip test)
- NEW: `crates/octo-storage-core/tests/newtype_from_escape.rs` (TV-A13)
- NEW: `crates/octo-storage-core/tests/ddl_allowlist_rejects_unregistered.rs` (TV-A5)
- NEW: `docs/audits/0206-001-substrate-newtype-audit.md` (substrate surface audit documenting legacy migration paths)

## Cross-references

- RFC-0206 v2.1 §Substrate Newtype Refactor (lines 218-247)
- RFC-0206 v2.1 §Cargo.toml Templates Layer A (lines 96-128 v2.1)
- RFC-0206 v2.1 §Cargo.toml Templates Layer B (lines 130-136 v2.1)
- RFC-0206 v2.1 §Wiring Pattern (lines 150-185 v2.1)
- RFC-0206 v2.1 §Format Bypass Defense (lines 248-280)
- RFC-0206 v2.1 §Migration Order (lines added v2.1)
- RFC-0206 v2.1 §Escape Hatch Enumeration (added v2.1)
- TV-0206-A1..A5, A13, A14
- Mission `0206-011-rfc-0206-v21-amendment` (RFC-only amendment unblocks this mission)

## Out of scope

- TYPE renames in consumer crates (owned by `0206-002 v3.0` + `0206-008`)
- Trait moves (owned by `0206-003 v3.0`)
- Adapter crate creation (owned by `0206-009`)
- Per-adapter fixtures (owned by `0206-010`)
- VaultStore/ReputationStore/SessionStore/PolicyStore trait declarations (owned by `0206-009` per §Wiring Pattern v2.1)
- Phase 2 typed-query expansion (RFC v3.0 deferred per §Implementation Phases 2.1)
- Phase 3 legacy removal (RFC v3.0 ≥ 2027-02-20)

## Dependencies

- `0206-011-rfc-0206-v21-amendment` (RFC spec re-baseline; LANDED 2026-08-20)
- RFC-0206 v2.1 (Accepted per amendment)

## Version History

| Version | Date       | Change                                                                                                                                                                                                                       |
| ------- | ---------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| v1.0    | 2026-08-20 | Initial filing (4 decomposed AC gates summing to 11 types)                                                                                                                                                                  |
| v2.0    | 2026-08-20 | R1 wholesale rewrite (11-type surface spec)                                                                                                                                                                                  |
| v2.1    | 2026-08-20 | R2 closure (AdapterId visibility + facade ownership transferred from 0206-004 per R2 CRIT 3)                                                                                                                                |
| v3.0    | 2026-08-20 | RFC-0206 v2.1 amendment alignment: 8-pub-use cap + facade migration scope (per RFC §Cargo.toml Templates Layer B v2.1) + §Migration Order legacy coexistence + §Escape Hatch Enumeration (7 legitimate sites documented) |