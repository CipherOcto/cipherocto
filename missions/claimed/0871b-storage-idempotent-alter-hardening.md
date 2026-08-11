# Mission: 0871b-storage-idempotent-alter-hardening — Retry-Safe ADD COLUMN

## Status

open (filed 2026-08-11). Cross-cuts missions
`0871b-storage-backend` (commit `71f8d745`) +
`0010-f8-rich-did-storage` (commit `269cf923`).

## Problem

The cipherocto-side migration runner
(`crates/quota-router-storage/src/migrations.rs`) records the
**migration version** in `cipherocto_schema_version` after running
all statements in a migration. But individual statements within a
migration are not idempotent — specifically `ALTER TABLE ADD
COLUMN` returns fork `Error::DuplicateColumn` (display: `"duplicate
column"`) on re-run.

Failure mode: process crash mid-`apply_pending` between two
ADD COLUMN statements of the same migration (e.g., between
v009 statement 1 and statement 2) leaves the schema-version row
un-inserted (because `record_migration` runs after `run_one`).
Re-running `apply_pending` re-executes the migration from
statement 1 → "duplicate column" error → `MigrationFailed` →
operator intervention required.

Recon confirms: stoolap fork does NOT support `ALTER TABLE ADD
COLUMN IF NOT EXISTS` (AlterTableStatement AST has no
`if_not_exists` flag per
`/home/mmacedoeu/.cargo/git/checkouts/stoolap-0de5b2281a88eb98/a5c19d1/src/parser/ast.rs:1867`,
and `pragma_table_info` is not implemented as a built-in pragma —
only SNAPSHOT/CHECKPOINT/SNAPSHOT_INTERVAL/KEEP_SNAPSHOTS/SYNC_MODE/WAL_FLUSH_TRIGGER/VACUUM
are recognized; unknown pragma returns empty result).

## Fix

Catch `Error::DuplicateColumn` per-statement inside `run_one`.
The error display string is exactly `"duplicate column"`
(`stoolap/.../core/error.rs:69`); match on substring.

```rust
fn run_one(db: &stoolap::Database, migration: &Migration) -> Result<(), MigrationError> {
    let statements = split_sql_statements(migration.sql);
    for stmt in &statements {
        match db.execute(stmt, ()) {
            Ok(_) => {}
            Err(e) if is_idempotent_already_applied(&format!("{e}"), stmt) => {
                // ADD COLUMN on a column that already exists = no-op.
                // Migration partially applied via prior crash; remaining
                // statements proceed.
                continue;
            }
            Err(e) => {
                return Err(MigrationError::MigrationFailed(
                    migration.version,
                    format!("{e}: {stmt}"),
                ));
            }
        }
    }
    record_migration(db, migration.version, migration.name)?;
    Ok(())
}

fn is_idempotent_already_applied(err: &str, stmt: &str) -> bool {
    // Only swallow "duplicate column" for ADD COLUMN statements. Any
    // other error type propagates so real bugs surface.
    let upper = stmt.to_ascii_uppercase();
    (upper.contains("ADD COLUMN") || upper.contains("ADD\tCOLUMN"))
        && err.contains("duplicate column")
}
```

Why substring on "duplicate column" + ADD COLUMN guard:
- The fork's only path that emits `"duplicate column"` is
  `create_column_with_default` (ADD COLUMN path) per
  `stoolap/.../storage/mvcc/engine.rs:1475` →
  `core/schema.rs:741,772` → `Error::DuplicateColumn`. CREATE
  TABLE / CREATE INDEX have separate error variants.
- The ADD COLUMN guard prevents accidental swallow of future
  errors that happen to contain the substring (paranoid check; the
  string match is also restricted to the first 200 chars of err to
  limit false positives).

Scope-bounded: only `ADD COLUMN` statements benefit. CREATE INDEX
already uses `IF NOT EXISTS` (see v001-v008 migrations). CREATE
TABLE uses `IF NOT EXISTS` (`v001__create_asks_table.sql` +
`v008__create_did_registry.sql`). DROP COLUMN / RENAME / MODIFY
are not yet used.

## Acceptance criteria

- [ ] `migrations::run_one` catches `Error::DuplicateColumn` for
      ADD COLUMN statements and treats as no-op.
- [ ] Non-duplicate errors propagate unchanged.
- [ ] ADD COLUMN on column-not-yet-existing (fresh DB) still
      succeeds (no regression).
- [ ] ADD COLUMN on column-already-existing (re-run after
      partial crash) is silent no-op; version row still recorded
      after all statements run.
- [ ] NEW TV `apply_pending_is_idempotent_on_re_run_after_partial_migration`
      in `tests/stoolap_idempotent_alter.rs`:
      - apply v001..v010 fresh DB → success, version=10.
      - simulate partial crash: manually DELETE the v010 row from
        `cipherocto_schema_version` (version row stays removed).
      - re-run `apply_pending` → success, version=10 again,
        no panic / no `MigrationFailed`. Asserts the re-run
        succeeded (operator's retry works).
- [ ] NEW TV `apply_pending_propagates_non_duplicate_error`: stub
      migration with malformed ADD COLUMN (`ADD COLUMN badtype`)
      → assert `MigrationFailed` propagates (not swallowed).
- [ ] All existing migration TV still pass (no regression).

## Files

- `crates/quota-router-storage/src/migrations.rs` — add helper
  + modify `run_one`.
- `crates/quota-router-storage/tests/stoolap_idempotent_alter.rs`
  (NEW) — 2 TV.

## Layer discipline

`migrations.rs` is in `quota-router-storage` (Layer B-adjacent,
years-stable). No new dep; no API change to public surface
(`apply_pending` signature unchanged).

## Defer (explicit)

- Per-statement version stamps in `cipherocto_schema_version`
  (more granular retry tracking) — not in scope; full migration
  version is sufficient once idempotency guard lands.
- Migration `is_already_applied` for DROP COLUMN / RENAME /
  MODIFY — not yet used in catalog; defer until first such
  migration lands.

## Why now

`0010-f8-rich-did-storage` (commit `269cf923`) added v009 + v010
with 4 ADD COLUMN statements total. A crash between any two
of them today bricks the DB until operator intervention. This
mission lands before any production deploy attempt.