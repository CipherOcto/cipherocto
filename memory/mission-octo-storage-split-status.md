---
name: mission-octo-storage-split-status
description: S2 (Storage Layer Restructuring Plan) Phase 1 LANDED 2026-08-16. Layer A substrate crates/octo-storage-core/ (1173 LoC, 30 tests passing, clippy + fmt clean). Tracker-table name configurable via ApplyConfig (default 'schema_migrations'). Phase 2 (Layer B facade + 3 owner migrations) pending.
metadata:
  type: project
  originSessionId: 2026-08-16-s2
  modified: 2026-08-16T...
---

# Mission: octo-storage split — Status (S2 Phase 1 LANDED)

**Status:** Phase 1 LANDED (2026-08-16, claimant @mmacedoeu).
Phase 2 (Layer B facade + 3 owner migrations) pending.
**Plan ref:** `docs/plans/2026-08-16-storage-layer-restructuring-execution-plan.md`
§3 S2.

## Phase 1 deliverable (LANDED)

`crates/octo-storage-core/` (NEW, Layer A):

- `Migration` trait (`version` + `name` + `sql`, `Send + Sync`)
- `StaticMigration` zero-erased newtype (matches the 3 historical
  pre-substrate shapes; const-constructible for `const MIGRATIONS`
  slices)
- `ApplyConfig` (default `schema_migrations` tracker; overridable)
- `apply_pending(db, &[&'static dyn Migration], ApplyConfig)` —
  unified runner with idempotent ADD COLUMN swallow rule (cross-port
  from `quota-router-storage/migrations.rs` `is_idempotent_already_applied`)
- `ensure_tracker_table` + `current_version` + `applied_version` +
  `record_migration` — tracker-table primitives
- `open(path: &str)` + `open_in_memory()` — thin `stoolap::Database`
  wrappers (adds `file://` DSN prefix internally)
- `StorageError` enum (thiserror; `Stoolap` + `MigrationFailed` +
  `UnknownMigration` + `SystemTime` + `Unsupported` variants)
- `split_sql_statements` SQL splitter (lifted from
  `quota-router-storage/migrations.rs`, identical semantics)
- 30 tests pass: 25 lib + 5 integration TV per plan §24
- 0 clippy warnings (per `feedback_clippy_zero_warnings`)
- 0 fmt diffs (per `cargo-fmt-workflow`)

## Key API decisions

- **Tracker-table name parameter**: pre-existing convention diverges
  across the 3 owner crates (`octo-reputation` +
  `quota-router-sm-engine` use `schema_migrations`;
  `quota-router-storage` uses `cipherocto_schema_version`). The
  substrate exposes `ApplyConfig::with_tracker_table(name)` so each
  owner migrates WITHOUT a data-shape change in Phase 2.
- **DSN wrapping**: `octo_storage_core::open(path: &str)` internally
  prefixes `file://` — owner crates pass a plain filesystem path.
  Matches `stoolap::Database::open(dsn: &str)` signature in fork
  (`MEMORY_SCHEME = "memory"`, `FILE_SCHEME = "file"`).
- **Layer direction**: Layer A depends only on `stoolap` (Layer A
  substrate) + `thiserror`. No cipherocto-crate deps. Per
  `cipherocto-design-principles` §Layer A row, this crate is
  RFC-frozen.

## Cross-references

- [[stoolap-general-purpose-db]] — fork convention
- [[cipherocto-design-principles]] — Layer A/B/C/D/E stability model
- [[stoolap-fork-stability-audit-status]] — S1 LANDED; fork pin
  CURRENT (`a5c19d1c01015c5f50266884c522bb12b84aaa16`)
- [[no-phantom-mission-pointers]] — no slugs cited
- [[feedback_clippy_zero_warnings]] — clippy invariant
- [[cargo-fmt-workflow]] — fmt invariant
- [[feedback_initiation_user_only]] — local commits free; push +
  remote writes await user instruction

## Phase 2 (pending)

- `crates/octo-storage/` (NEW, Layer B facade — pure re-exports)
- Migrate `quota-router-storage/migrations.rs` → `octo_storage_core::apply_pending`
  (use `ApplyConfig::with_tracker_table("cipherocto_schema_version")`)
- Migrate `octo-reputation` (feature-gated `stoolap_runner` module)
- Migrate `quota-router-sm-engine/src/schema.rs` (per-version const + `MigratableDatabase` trait → unified `apply_pending`)
- 5 TV fixtures per plan §24 — already landed as integration tests; remaining work is the RFC body in S7
- Mission AC close-out + memory card refresh
