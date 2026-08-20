---
name: 0206-001-substrate-newtype
description: Open 2026-08-20; RFC-0206 v2.0 §Substrate Newtype Refactor + §Cargo.toml Templates Layer A. crates/octo-storage-core/ sole fork consumer + Database newtype + TypedStatement enum + DDL allowlist + 11-item re-export set.
metadata:
  node_type: mission
  type: project
  originSessionId: 9a316ae1-cb15-46f4-801f-834acacd23ae
  created: 2026-08-20T00:00:00.000Z
---

# Mission `0206-001-substrate-newtype` — OPEN 2026-08-20

## Scope

Implement RFC-0206 v2.0 §Substrate Newtype Refactor + §Cargo.toml Templates Layer A. Create the substrate `crates/octo-storage-core/` as the SOLE direct fork consumer in the workspace.

Covers:

- **Substrate skeleton** at `crates/octo-storage-core/src/{database,typed_statement,allowlist,error,migrations}.rs`
- **`pub struct Database(stoolap::Database)`** newtype in `database.rs` with `Deref<Target = stoolap::Database>` + `From<Database> for stoolap::Database` (one-way escape for typed-query allowlist sites); NO reverse `From<stoolap::Database>` to prevent Layer B reverse-engineering
- **`TypedStatement` enum** in `typed_statement.rs`: `Select(SqlSelect)`, `Insert(SqlInsert)`, `Update(SqlUpdate)`, `Delete(SqlDelete)`, `DdlNoOp`, `DdlRegistered(DdlTemplate)`
- **`AdapterAllowlist`** in `allowlist.rs` with `check(&TypedStatement) -> Result<(), SubstrateError>` runtime enforcement; rejects DDL outside allowlist
- **`SubstrateError` enum** in `error.rs`: `DdlNotInAllowlist { template }`, `TableNotInNamespace { table }`, `AdapterIdNotRegistered { id }`, `Stderr(stoolap::Error)`
- **`Cargo.toml` Layer A** template per RFC-0206 v2.0 §Cargo.toml Templates Layer A: `stoolap = { git = "https://github.com/CipherOcto/stoolap", rev = "<sha-0>" }`; `[features] default = ["allow-listed-ddl"]`
- **11-item re-export set** at `crates/octo-storage-core/src/lib.rs` per RFC-0206 v2.0 §Cargo.toml Templates Layer A (11 `pub use` statements + 1 `pub mod migrations`); `stoolap::Database` NOT re-exported
- **`execute_checked` API**: `Database::execute_checked(&self, adapter_id: AdapterId, stmt: TypedStatement) -> Result<(), SubstrateError>`
- **`open()` / `open_in_memory()` constructors** returning `Result<Database, SubstrateError>`
- **8-pub-use cap enforcement**: substrate `pub use` count ≤ 8 (`rg -c '^\s*pub use\b' crates/octo-storage-core/src/lib.rs` ≤ 8)
- **Wildcard detector**: `rg '\b\*\s*[,;}]' crates/octo-storage-core/src/lib.rs` MUST equal 0 (lint-enforced)

## Acceptance Criterion

- `crates/octo-storage-core/Cargo.toml` declares ONLY `stoolap` (no other workspace deps); `[features] default = ["allow-listed-ddl"]`
- `crates/octo-storage-core/src/lib.rs` re-exports exactly 11 items per RFC-0206 v2.0 spec (verified by `rg -c '^\s*pub use\b'`)
- `Database::open()` returns `Result<Database, _>` (NOT `Result<stoolap::Database, _>` — Layer A handle leak closed)
- `From<Database> for stoolap::Database` impl exists; reverse `From<stoolap::Database> for Database` does NOT exist
- `TypedStatement` enum has 6 variants per RFC-0206 v2.0 spec
- `AdapterAllowlist::check()` runtime test (`crates/octo-storage-core/tests/ddl_allowlist_rejects_unregistered.rs`) returns `SubstrateError::DdlNotInAllowlist` on unregistered DDL
- TV-0206-A2, A3, A4, A13 gate commands green
- `cargo build -p octo-storage-core` green; `cargo clippy -p octo-storage-core --all-targets --all-features -- -D warnings` green; `cargo fmt --all -- --check` green

## Files / Artifacts

- New: `crates/octo-storage-core/Cargo.toml` + `src/lib.rs` + `src/database.rs` + `src/typed_statement.rs` + `src/allowlist.rs` + `src/error.rs` + `src/migrations.rs`
- New: `crates/octo-storage-core/tests/ddl_allowlist_rejects_unregistered.rs` + `tests/newtype_from_escape.rs`

## Cross-references

- RFC-0206 v2.0 §Substrate Newtype Refactor
- RFC-0206 v2.0 §Cargo.toml Templates Layer A
- RFC-0206 v2.0 §Cargo.toml Cross-Cuts
- RFC-0206 v2.0 TV-0206-A2, A3, A4, A13

## Out of scope

- 29 Layer B TYPE renames (owned by `0206-002-layer-b-type-renames` — depends on this mission)
- 5 adapter crates (owned by `0206-004-adapter-crates` — depends on this mission)
- `crates/octo-storage/` facade (separate substrate gate; lands as part of `0206-004-adapter-crates`)
- HolderRegistry trait move (owned by `0206-003-trait-moves`)
- StoolapDidRegistry impl move (owned by `0206-003-trait-moves`)

## Dependencies

- Stoolap fork at freeze tag `octo-stoolap-frozen-v0` (per RFC-0205 v2.0 §Release-Tag Pin Policy; pending Phase 1.3 of `0205-002-phase1-deliverables`)
