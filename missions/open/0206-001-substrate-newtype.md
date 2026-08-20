---
name: 0206-001-substrate-newtype
v: "2.0"
supersedes: v1.0
description: Open 2026-08-20; RFC-0206 v2.0 §Substrate Newtype Refactor + §Cargo.toml Templates Layer A. crates/octo-storage-core/ sole fork consumer + Database newtype + TypedStatement enum + DDL allowlist + 11-item re-export set.
metadata:
  node_type: mission
  type: project
  originSessionId: 9a316ae1-cb15-46f4-801f-834acacd23ae
  created: 2026-08-20T00:00:00.000Z
  updated: 2026-08-20T00:00:00.000Z
depends_on:
  - 0205-002-phase1-deliverables
  - RFC-0205
  - RFC-0206
---

# Mission `0206-001-substrate-newtype` — OPEN 2026-08-20 (v2.0)

## Scope

Implement RFC-0206 v2.0 §Substrate Newtype Refactor + §Cargo.toml Templates Layer A. Create the substrate `crates/octo-storage-core/` as the SOLE direct fork consumer in the workspace.

Covers:

- **Substrate skeleton** at `crates/octo-storage-core/src/{database,typed_statement,allowlist,error,migrations}.rs`
- **`pub struct Database(stoolap::Database)`** newtype in `database.rs` with `Deref<Target = stoolap::Database>` + `From<Database> for stoolap::Database` (one-way escape for typed-query allowlist sites); NO reverse `From<stoolap::Database>` to prevent Layer B reverse-engineering. The `stoolap::Database` field is NOT `pub` (TupleStruct field remains private).
- **`TypedStatement` enum** in `typed_statement.rs` (RFC-0206 v2.0 §Definitions ordering):
  - `Select(SqlSelect)` — read-only query typed statement
  - `DdlNoOp` — DDL statement substrate passes through without allowlist check (PRAGMA, ANALYZE); counted as no-op for adapter quota
  - `DdlRegistered(DdlTemplate)` — DDL matched against allowlist, `DdlTemplate` carries `DdlOperation` semantics
  - `Insert(SqlInsert)` — typed insert statement
  - `Update(SqlUpdate)` — typed update statement
  - `Delete(SqlDelete)` — typed delete statement
- **`SqlSelect` / `SqlInsert` / `SqlUpdate` / `SqlDelete`** typed query structs (one-line signature per type, owned by `typed_statement.rs`)
- **`DdlTemplate`** in `migrations.rs` wrapping `DdlOperation` (CreateTable/AlterTable/DropTable enum) + carrying the SQL string
- **`DdlOperation`** in `migrations.rs`: `CreateTable`, `AlterTable`, `DropTable` enum (3 variants)
- **`Result<T>`** in `migrations.rs`: type alias `pub type Result<T> = std::result::Result<T, SubstrateError>` (NOT `crate::Result` — explicit alias required to avoid downstream shadowing)
- **`AdapterAllowlist`** in `allowlist.rs` with `check(&TypedStatement) -> Result<(), SubstrateError>` runtime enforcement; rejects DDL outside allowlist
- **`AdapterId`** type lives in `crate::allowlist` (callers use `octo_storage_core::allowlist::AdapterId`) — NOT re-exported in `lib.rs`
- **`SubstrateError` enum** in `error.rs`: `DdlNotInAllowlist { template }`, `TableNotInNamespace { table }`, `Stderr(stoolap::Error)`. Note: `AdapterIdNotRegistered` variant is NOT in RFC-0206 v2.0 §Substrate Newtype Refactor and is therefore OUT of scope.
- **`Cargo.toml` Layer A** template per RFC-0206 v2.0 §Cargo.toml Templates Layer A: `stoolap = { git = "https://github.com/CipherOcto/stoolap", rev = "<sha-0>" }`; `[features] default = ["allow-listed-ddl"]` AND `allow-listed-ddl = []` AND `strict-typed-query = []` declared as mutually empty features
- **11-item re-export set** at `crates/octo-storage-core/src/lib.rs` per RFC-0206 v2.0 §Cargo.toml Templates Layer A: exactly 8 `pub use` statements (Database, TypedStatement, SqlSelect, SqlInsert, SqlUpdate, SqlDelete, AdapterAllowlist, SubstrateError) + 1 `pub mod migrations` (which exposes DdlTemplate, DdlOperation, Result) = 11 items total. `stoolap::Database` NOT re-exported.
- **`execute_checked` API**: `Database::execute_checked(&self, adapter_id: AdapterId, stmt: TypedStatement) -> Result<(), SubstrateError>`
- **`open()` / `open_in_memory()` constructors** returning `Result<Database, SubstrateError>`
- **`pub-use-statement cap enforcement**`: `rg -c '^\s*pub use\b' crates/octo-storage-core/src/lib.rs` MUST equal 8 (NOT ≤ 8)
- **Wildcard detector**: `rg '\b\*\s*[,;}]' crates/octo-storage-core/src/` MUST equal 0 (rg-checked at AC time, across ALL substrate files); `rg 'pub use\b.*\*' crates/octo-storage-core/src/migrations.rs` MUST exit 1 (no wildcard re-exports in migrations either)

## Acceptance Criterion

- `crates/octo-storage-core/Cargo.toml` declares ONLY `stoolap` (no other workspace deps); `[features] default = ["allow-listed-ddl"]` AND `allow-listed-ddl = []` AND `strict-typed-query = []` declared as mutually empty features
- **pub-use-statement cap**: `rg -c '^\s*pub use\b' crates/octo-storage-core/src/lib.rs` MUST equal 8 (TV-0206-A4 statement cap)
- **11-item re-export set**: `rg -c '^\s*pub (use|mod|const|fn|struct|enum|type)\b' crates/octo-storage-core/src/{lib,database,typed_statement,allowlist,error,migrations}.rs` MUST equal 11 (item count; TV-0206-A4 item gate)
- **Wildcard detector**: `rg '\b\*\s*[,;}]' crates/octo-storage-core/src/` MUST equal 0; `rg 'pub use\b.*\*' crates/octo-storage-core/src/migrations.rs` MUST exit 1
- **Field privacy**: `rg 'pub stoolap::Database' crates/octo-storage-core/src/database.rs` exits 1 (field is NOT pub)
- **From impl direction gates**: `rg 'impl From<Database> for stoolap::Database' crates/octo-storage-core/src/database.rs` exits 0; `rg 'impl From<stoolap::Database> for Database' crates/octo-storage-core/` exits 1
- `Database::open()` returns `Result<Database, _>` (NOT `Result<stoolap::Database, _>` — Layer A handle leak closed)
- `TypedStatement` enum has 6 variants per RFC-0206 v2.0 §Definitions ordering (Select, DdlNoOp, DdlRegistered, Insert, Update, Delete)
- `AdapterAllowlist::check()` runtime test (`crates/octo-storage-core/tests/ddl_allowlist_rejects_unregistered.rs`) returns `SubstrateError::DdlNotInAllowlist` on unregistered DDL
- **`migrations.rs` AC**: file exists, declares `pub fn current_version() -> u32 { 0 }` placeholder, NO wildcard `pub use` re-exports
- **`migrations.rs` reachability**: `cargo doc -p octo-storage-core --no-deps` generates doc page for `crate::migrations`
- **RFC-0205 v2.0 Accepted precondition**: `rg -m1 '^Accepted' rfcs/accepted/storage/0205-stoolap-fork-stability.md` exits 0
- **Commit hash pin**: `git rev-parse HEAD:rfcs/accepted/storage/0206-octo-storage-split.md` matches RFC-0206 v2.0 commit hash
- TV-0206-A1, A2, A3, A4, A5, A13 gate commands green (TV-0206-A1 = cargo builtin parser, no --pcre2; TV-0206-A5 = DDL allowlist runtime enforcement)
- `cargo build -p octo-storage-core` green; `cargo clippy -p octo-storage-core --all-targets --all-features -- -D warnings` green; `cargo fmt --all -- --check` green

## Files / Artifacts

- New: `crates/octo-storage-core/Cargo.toml` + `src/lib.rs` + `src/database.rs` + `src/typed_statement.rs` + `src/allowlist.rs` + `src/error.rs` + `src/migrations.rs`
- New: `crates/octo-storage-core/tests/ddl_allowlist_rejects_unregistered.rs` + `tests/newtype_from_escape.rs`

## Cross-references

- RFC-0206 v2.0 §Substrate Newtype Refactor
- RFC-0206 v2.0 §Cargo.toml Templates Layer A
- RFC-0206 v2.0 §Cargo.toml Cross-Cuts
- RFC-0206 v2.0 §Definitions (TypedStatement variant ordering)
- RFC-0206 v2.0 TV-0206-A1, A2, A3, A4, A5, A13
- RFC-0205 v2.0 §Release-Tag Pin Policy

## Out of scope

- 29 Layer B TYPE renames (owned by `0206-002-layer-b-type-renames` — depends on this mission)
- 5 adapter crates (owned by `0206-004-adapter-crates` — depends on this mission)
- `crates/octo-storage/` facade (separate substrate gate; lands as part of `0206-004-adapter-crates`)
- HolderRegistry trait move (owned by `0206-003-trait-moves`)
- StoolapDidRegistry impl move (owned by `0206-003-trait-moves`)
- `SubstrateError::AdapterIdNotRegistered` variant (not in RFC-0206 v2.0 §Substrate Newtype Refactor)

## Dependencies

- Stoolap fork at freeze tag `octo-stoolap-frozen-v0` (per RFC-0205 v2.0 §Release-Tag Pin Policy; pending Phase 1.3 of `0205-002-phase1-deliverables`)
- `0205-002-phase1-deliverables` (Phase 1.3 fork freeze)
- RFC-0205 v2.0 Accepted status
- RFC-0206 v2.0 Accepted status

## v2.0 Changes from v1.0

Applied 19 R1 findings (2 CRIT + 4 HIGH + 6 MED + 7 LOW):

### CRIT (2/2 applied)

- **CRIT 1** (Scope line 28 + AC line 39): resolved numerical contradiction between "11-item re-export set" and "pub-use-statement cap ≤ 8". Scope now specifies exactly 8 `pub use` statements (Database, TypedStatement, SqlSelect, SqlInsert, SqlUpdate, SqlDelete, AdapterAllowlist, SubstrateError) + 1 `pub mod migrations` (DdlTemplate, DdlOperation, Result) = 11 items total.
- **CRIT 2** (AC `rg -c '^\s*pub use\b'`): split into TWO verifiable gates: (a) pub-use-statement cap = 8 (TV-0206-A4 statement cap), (b) item count = 11 across `{lib,database,typed_statement,allowlist,error,migrations}.rs` (TV-0206-A4 item gate).

### HIGH (4/4 applied)

- **HIGH 1**: removed `SubstrateError::AdapterIdNotRegistered { id }` variant from scope (not in RFC-0206 v2.0 §Substrate Newtype Refactor). Error enum now: `DdlNotInAllowlist`, `TableNotInNamespace`, `Stderr`. Listed as OOS.
- **HIGH 2**: added explicit AC for `migrations.rs` (exists, declares `pub fn current_version() -> u32 { 0 }` placeholder, no wildcard `pub use` re-exports).
- **HIGH 3**: `AdapterId` type lives in `crate::allowlist`, NOT re-exported in `lib.rs`; callers use `octo_storage_core::allowlist::AdapterId`.
- **HIGH 4**: wildcard detector extended scope to ALL substrate files (`rg '\b\*\s*[,;}]' crates/octo-storage-core/src/` = 0); added `rg 'pub use\b.*\*' crates/octo-storage-core/src/migrations.rs` exits 1.

### MED (6/6 applied)

- **MED 1**: added `strict-typed-query = []` feature alongside `allow-listed-ddl = []`.
- **MED 2**: documented one-line signature per type for `TypedStatement` variants.
- **MED 3**: documented `DdlOperation` (CreateTable/AlterTable/DropTable enum) and `Result<T>` (alias over SubstrateError) types in scope.
- **MED 4**: added From impl direction gates (`From<Database> for stoolap::Database` present, reverse absent).
- **MED 5**: added RFC-0205 v2.0 Accepted precondition gate (`rg -m1 '^Accepted' rfcs/accepted/storage/0205-stoolap-fork-stability.md` exits 0).
- **MED 6**: renamed "lint-enforced" to "rg-checked at AC time" (actual mechanism is one-shot rg).

### LOW (7/7 applied)

- **LOW 1**: added `rg 'pub stoolap::Database' crates/octo-storage-core/src/database.rs` exits 1 (field is NOT pub).
- **LOW 2**: added TV-0206-A1 (cargo builtin parser, no --pcre2) + TV-0206-A5 (DDL allowlist runtime enforcement) to ACs.
- **LOW 3**: documented `DdlNoOp` semantics (PRAGMA/ANALYZE pass-through, no allowlist check, counted as no-op for adapter quota).
- **LOW 4**: renamed "8-pub-use cap" to "pub-use-statement cap" for clarity.
- **LOW 5**: added commit hash pin (`git rev-parse HEAD:rfcs/accepted/storage/0206-octo-storage-split.md` matches v2.0).
- **LOW 6**: added `migrations.rs` reachability AC (`cargo doc -p octo-storage-core --no-deps` generates doc page).
- **LOW 7**: aligned `TypedStatement` variant ordering to RFC §Definitions (Select, DdlNoOp, DdlRegistered, Insert, Update, Delete).

### YAML frontmatter

- Added `v: "2.0"` + `supersedes: v1.0` + `updated: 2026-08-20T00:00:00.000Z`.
- Added `depends_on:` block listing `0205-002-phase1-deliverables`, `RFC-0205`, `RFC-0206`.

### Substrate skeleton

- Confirmed `migrations.rs` is in scope (not in RFC §Phase 1.2 list but required by `pub mod migrations` re-export count).
