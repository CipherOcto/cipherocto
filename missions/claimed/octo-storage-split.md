# Mission: octo-storage split (Layer A→B→C crate family)

## Status

**Claimed (2026-08-16, claimant @mmacedoeu).** S2 deliverable per
`docs/plans/2026-08-16-storage-layer-restructuring-execution-plan.md`
§3 row 2 (Stream B.2). Work in progress.

## RFC

- Parent: NEW RFC `octo-storage-split` (Layer A→B→C packaging). Draft →
  Accepted per `docs/BLUEPRINT.md` §RFC Process; RFC body filed in S7 per
  plan §2 A.2.
- Related: RFC-0914-a `0914-a-stoolap-persistence` (persistence convention).
- Related: [[stoolap-general-purpose-db]] (CipherOcto fork convention).
- Source review: `docs/reviews/2026-08-15-storage-layer-restructuring-analysis.md`
  §4.6 / §4.6.1.

## Summary

Extract cipherocto's storage-substrate primitives (migration runner +
Database construction) from per-owner crates into a unified
`crates/octo-storage-core/` (Layer A) + `crates/octo-storage/` (Layer B
facade) crate family. Per-owner adapter crates (octo-reputation,
quota-router-sm-engine, quota-router-storage) depend on
`octo-storage-core` instead of the `stoolap` crate directly for
migration + DB-construction use.

### R1-F5 placeholder correction (preflight 2026-08-16)

The plan §2 B.2 text assumed `crates/octo-storage/` already existed as a
Layer B facade. **It does not.** Preflight verification:

- `crates/octo-storage/` — does NOT exist
- `crates/octo-storage-core/` — does NOT exist
- Closest existing crate: `crates/quota-router-storage/` (domain-specific
  storage for quota-router; not a substrate)

**Revised scope:** S2 deliverable is **NEW crate family** (create both
crates + migrate owner crates), not a refactor of an existing crate.

## Preflight inventory (2026-08-16)

### Owner crates with migrations (3)

| Crate                    | Migration files             | API style                                                                                   |
| ------------------------ | --------------------------- | ------------------------------------------------------------------------------------------- |
| `quota-router-storage`   | 12 (v001-v012)              | `Migration { version, name, sql }` struct + `apply_pending(db)`                             |
| `octo-reputation`        | 8 (v001-v005 + v010-v012)   | `(&'static str, &'static str)` tuple + `apply(db)` (feature-gated `stoolap_runner` module)  |
| `quota-router-sm-engine` | 7 (000_bootstrap + 001-006) | Per-version `const MIGRATION_NNN_SQL` + `MigratableDatabase` trait + `apply_migrations(db)` |

**3 distinct APIs** across owner crates. Unified trait required.

### Crates with direct stoolap dep but no migrations (7)

| Crate                           | Use                                |
| ------------------------------- | ---------------------------------- |
| `octo-adapter-telegram-mtproto` | Persistence (no schema migrations) |
| `octo-adapter-whatsapp`         | SQL storage                        |
| `octo-core`                     | Phase C migration runner + DAO     |
| `octo-matrix-session-store`     | Session store                      |
| `octo-whatsapp`                 | Embedded SQL DB (optional feature) |
| `quota-router-cli`              | CLI tool wrapper                   |
| `quota-router-core`             | Quota router SQL substrate         |

**Out of scope for S2** — these use `stoolap::Database` directly for
non-DB-construction access. They MAY benefit from a future octo-storage
facade for type aliases, but doing it now would inflate scope past MED.

## Scope

1. **`crates/octo-storage-core/`** (NEW, Layer A). Owns:
   - `pub trait Migration { fn version(&self) -> u32; fn name(&self) -> &'static str; fn sql(&self) -> &'static str; }`
   - `pub struct MigrationsHandle { ... }` — wraps a Database + ordered list of registered migrations
   - `pub fn apply_pending(db: &stoolap::Database, migrations: &[&'static dyn Migration]) -> Result<(), MigrationError>` — unified runner with version-tracking table
   - `pub fn open_in_memory() -> Result<stoolap::Database, StorageError>`
   - `pub fn open<P: AsRef<Path>>(path: P) -> Result<stoolap::Database, StorageError>`
   - `pub fn list_migrations(migrations: &[&'static dyn Migration]) -> Vec<u32>`
   - `pub enum StorageError { Stoolap(String), Migration(String), NotFound(String) }` (thiserror)
   - Layer A: depends only on `stoolap` + `thiserror` + `std`
   - 100% `cargo test -p octo-storage-core --lib` green

2. **`crates/octo-storage/`** (NEW, Layer B facade). Owns:
   - `pub use octo_storage_core::*;` (re-export all substrate types)
   - Optional convenience: `pub mod prelude { pub use octo_storage_core::*; }`
   - Layer B: depends only on `octo-storage-core`
   - 100% `cargo test -p octo-storage --lib` green (no test code; smoke-test re-exports)

3. **`quota-router-storage` migration**:
   - Replace `src/migrations.rs` private module with `use octo_storage_core::*`
   - Keep local `BUILTIN_MIGRATIONS: &[&'static dyn Migration]` (re-typed)
   - Replace direct `stoolap::Database::open_in_memory()` + `stoolap::Database::open(path)` with `octo_storage_core::open_in_memory()` / `octo_storage_core::open(path)`
   - Keep all existing tests green
   - Verify `cargo test -p quota-router-storage --lib` + `cargo test -p quota-router-storage --tests` green

4. **`octo-reputation` migration**:
   - Replace `src/migrations.rs` `stoolap_runner` module with `octo_storage_core::apply_pending` call
   - Keep `BUILTIN_MIGRATIONS` as a typed slice of `Migration` trait objects
   - Remove direct `stoolap::Database` dep for migration; route via octo-storage-core

5. **`quota-router-sm-engine` migration**:
   - Replace per-version `const MIGRATION_NNN_SQL` + `MigratableDatabase` trait + `apply_migrations(db)` with `octo_storage_core::apply_pending`
   - Keep `BOOTSTRAP_SQL` for the initial schema_migrations table OR absorb into octo-storage-core as the tracker-table DDL (decision documented in §Decision Points below)

6. **Adapter trait TV fixtures (5)** per plan §3 (S2 row) + §4 (S2 verification gates):
   - `octo-storage-core::Migration` trait — 2 TV (idempotency + ordering invariants)
   - `octo-storage-core::apply_pending` — 2 TV (fresh DB + partially-applied DB)
   - `octo-storage-core::open` / `open_in_memory` — 1 TV (round-trip with `execute` + `query`)

## Decision Points (resolve during implementation)

### DP-1: Where does the `cipherocto_schema_version` (or equivalent) tracker table DDL live?

Options:

- (A) Each owner crate keeps its own tracker table DDL (current state; per-crate)
- (B) Unified tracker table DDL in `octo-storage-core`; owner crates reuse
- (C) Hybrid: octo-storage-core provides a `default_tracker_sql()` helper; owners override

**Recommendation:** (B). One tracker table name (`cipherocto_schema_version`),
one DDL string in octo-storage-core. Owner crates use the helper.

### DP-2: `quota-router-sm-engine`'s `000_bootstrap.sql` semantic

The `000_bootstrap.sql` file appears to be a pre-existing schema setup
file, not a numbered migration. Decision: keep it as a "pre-migration
init" step, OR convert it to `v001__bootstrap`. **Recommendation:**
inspect the SQL content first; if it's `CREATE TABLE schema_migrations`,
absorb into octo-storage-core as the tracker-table DDL; otherwise keep
as a separate pre-migration.

### DP-3: `octo-reputation`'s `v003__schema_migrations.sql` — same as DP-2

The file `v003__schema_migrations.sql` IS the tracker-table DDL.
**Recommendation:** rename to `_internal_tracking_table.sql` (not a
"migration"), keep in octo-storage-core. Remove from the migration list.

## Acceptance Criteria

- [ ] `crates/octo-storage-core/` exists with `Cargo.toml` + `src/lib.rs`
- [ ] `Migration` trait + `MigrationsHandle` + `apply_pending` + `open` /
      `open_in_memory` + `StorageError` all public
- [ ] `cargo test -p octo-storage-core --lib` green (≥10 unit tests)
- [ ] `crates/octo-storage/` exists as Layer B facade
- [ ] `cargo test -p octo-storage --lib` green
- [ ] `quota-router-storage` migrated: `Cargo.toml` adds
      `octo-storage-core` dep; `src/migrations.rs` re-uses substrate
      types; no direct `stoolap::Database::open*` in src code
- [ ] `cargo test -p quota-router-storage --lib` + `--tests` green
      (no regressions; same test count or more)
- [ ] `octo-reputation` migrated: `src/migrations.rs` uses
      `octo_storage_core::apply_pending`; no direct `stoolap::Database::open*`
- [ ] `cargo test -p octo-reputation --lib --features stoolap` green
- [ ] `quota-router-sm-engine` migrated: `src/schema.rs` re-uses
      substrate types; `MigratableDatabase` trait removed or delegated
- [ ] `cargo test -p quota-router-sm-engine --lib` green
- [ ] 5 adapter trait TV fixtures pass (per plan §24)
- [ ] `cargo build --workspace --all-targets` green
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` clean
      (per `feedback_clippy_zero_warnings`)
- [ ] `cargo fmt --all -- --check` clean (per `cargo-fmt-workflow`)
- [ ] RFC `octo-storage-split` body drafted (target: `rfcs/draft/` in S7)

## Dependencies

- Plan S1 (`stoolap-fork-stability-audit`) — LANDED 2026-08-16 (audit
  doc + HOLD recommendation pin canonical). S2 unblocked.
- S3 (octo-vault crate) — gated on S2 (per plan §3 dependency graph).
- S4 (Dqa codemod) — gated on S2.

## Subsidiaries (none yet)

No claimed/archived missions cite this slug. Future claims will
auto-list here.

## Location

- New crate: `crates/octo-storage-core/` (Layer A)
- New crate: `crates/octo-storage/` (Layer B facade)
- Owner migration sites: 3 (above)
- RFC body: `rfcs/draft/octo-storage-split.md` (S7)
- Memory card: `memory/mission-octo-storage-split-status.md` (on land)

## Complexity

**HIGH** — not MED as plan suggests. New crate + cross-crate refactor
across 3 owner crates + 5 TV fixtures + RFC body. Estimated 6-10 hours
wall-clock across 1-2 sessions.

## Implementation Notes

- Layer A → Layer B direction is fine (substrate to facade). Layer B →
  Layer A is the wrong direction and would invert the architecture.
- Avoid `Arc<Database>` typing in the substrate — let owner crates
  choose their own concurrency model. `apply_pending` takes `&Database`.
- Use `&'static dyn Migration` for the trait object form so
  `BUILTIN_MIGRATIONS: &[&'static dyn Migration]` is a const slice.
- Tracker table DDL must be idempotent (`CREATE TABLE IF NOT EXISTS`).
- Migration runner must be idempotent (per-migration guard in
  schema_migrations table).
- Pre-existing API surface (`pub use ::migrations::apply_pending`) is
  internal; public API changes need a brief migration note.

## Reference

- [[stoolap-general-purpose-db]] — CipherOcto fork convention
- [[cipherocto-design-principles]] — Layer A/B/C/D/E stability model
- [[mission-stoolap-fork-stability-audit-status]] — S1 LANDED
- `docs/reviews/2026-08-15-storage-layer-restructuring-analysis.md` §4.6 / §4.6.1
- `docs/plans/2026-08-16-storage-layer-restructuring-execution-plan.md` §2 B.2 + §3 S2 + §4 S2 (verification gates)
- `docs/BLUEPRINT.md` §RFC Process (Draft → Accepted lifecycle)
- [[git-workflow]] — push + remote writes await user instruction
- [[feedback_initiation_user_only]] — local commits free; push + remote writes need explicit user instruction
- [[feedback_clippy_zero_warnings]] — clippy invariant
- [[cargo-fmt-workflow]] — fmt invariant

## Version History

| Version | Date       | Change                                                                                                                                                                                                                 |
| ------- | ---------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| v0.1    | 2026-08-16 | Mission filed. R1-F5 placeholder corrected (no existing `octo-storage` crate; scope = NEW crate family + migrate 3 owner crates). Preflight inventory attached. 3 Decision Points identified. RFC body deferred to S7. |
