# Audit — 0206-002 Layer B TYPE Renames v3.0

**Mission:** 0206-002-layer-b-type-renames-v3.0
**RFC:** RFC-0206 v2.1 §Layer B TYPE Renames (RFC table had wrong line refs — see Notes)
**Date:** 2026-08-20
**Status:** LANDED (audit pass)

## Summary

42 explicit `stoolap::Database` reference sites renamed to
`octo_storage_core::Database` across 9 files (35 TYPE positions + 7 doc
comments). TV-0206-A7 paths 1 (quota-router-storage) + 2 (octo-vault) closed.
Legacy `Migration` trait + `apply_pending` runner + `StaticMigration` /
`ApplyConfig` / `StorageError` references routed through the substrate's
`_legacy_*` deprecated aliases per RFC-0206 v2.1 §Migration Order.

## TV gate verification

| Gate | Command | Result |
|------|---------|--------|
| TV-A7 path 1 | `rg 'stoolap::Database' crates/quota-router-storage/src` | exit 1 (zero hits) ✓ |
| TV-A7 path 2 | `rg 'stoolap::Database' crates/octo-vault/src` | exit 1 (zero hits) ✓ |
| Cargo.toml stoolap dep | `rg '^\s*stoolap\s*=' crates/quota-router-storage/Cargo.toml crates/octo-vault/Cargo.toml` | 2 hits — v3.0 deviation (see §Deviations) |

## Files modified (9 source + 2 Cargo.toml + 3 test)

### Source renames (42 sites → octo_storage_core::Database)

- `crates/octo-vault/src/lib.rs` — 4 sites (lines 351, 371, 378, 395)
- `crates/quota-router-storage/src/ask_repo.rs` — 5 sites (lines 200, 209, 219, 228, 787)
- `crates/quota-router-storage/src/consumed_receipt_repo.rs` — 4 sites (57, 66, 76, 85)
- `crates/quota-router-storage/src/migrations.rs` — 5 sites (185, 274, 327, 339, 451)
- `crates/quota-router-storage/src/settlement_event_repo.rs` — 4 sites (26, 69, 79, 99)
- `crates/quota-router-storage/src/slash_store.rs` — 5 sites (113, 117, 125, 136, 145)
- `crates/quota-router-storage/src/stoolap_did_registry.rs` — 4 sites (3, 95, 110, 123)
- `crates/quota-router-storage/src/stoolap_holder_registry.rs` — 7 sites (3, 81, 82, 101, 114, 120, 121)
- `crates/quota-router-storage/src/stoolap_spend_ledger.rs` — 4 sites (3, 195, 248, 295)

### Cargo.toml edits

- `crates/octo-vault/Cargo.toml` — stoolap dep retained (see §Deviations); comment block updated to document v3.0 deviation rationale
- `crates/quota-router-storage/Cargo.toml` — same

### Test renames (3 test files)

- `crates/octo-vault/tests/apply_migrations.rs` — `octo_storage_core::applied_version` → `octo_storage_core::migrations::applied_version`; `use` blocks reformatted
- `crates/quota-router-storage/tests/stoolap_idempotent_alter.rs` — `stoolap::Database::open("memory://")` → `octo_storage_core::open_in_memory()` (2 sites)
- `crates/quota-router-storage/tests/stoolap_migration_chain.rs` — same (3 sites)

### Substrate cross-fix (1 file)

- `crates/octo-storage-core/src/database.rs` — added `#[derive(Clone)]` (consumer crates embed `Database` in `#[derive(Clone)]` structs)
- `crates/octo-storage-core/src/open.rs` — free fns `open()` + `open_in_memory()` updated to return `Result<Database, SubstrateError>` (was `Result<stoolap::Database, SubstrateError>`; legacy signature leaked the inner type)

## Deviations from v2.1 AC gate (filed in v3.0 §Deviations)

### D1 — `stoolap` direct dep RETAINED in consumer crates

**v2.1 AC gate** (original): `rg '^\s*stoolap\s*=' crates/quota-router-storage/Cargo.toml crates/octo-vault/Cargo.toml` returns zero lines

**v3.0 actual**: 2 hits (one per Cargo.toml) — dep retained

**Rationale**: The substrate redesign v3.0 wraps `stoolap::Database` behind the
`Database` newtype but does NOT re-export `stoolap::ResultRow` /
`stoolap::ApiTransaction` / `stoolap::Rows` / `stoolap::Error`. Consumer crates
that decode rows (`row_to_ask`, `decode_event_row`, etc.) or run transactions
(`put_in_tx`, `try_deduct`) need direct fork access for these types.

A v2.2 RFC-0206 amendment (filed as `0206-011b`) is the proper scope to add
a `pub mod stoolap` re-export block to the substrate, enabling consumer crates
to drop the direct dep. Until that amendment lands, the consumer crates'
direct `stoolap` dep is required per HIGH 6 consumer-crate exemption.

Cargo.toml comments in both crates document this deviation inline.

### D2 — substrate `open_in_memory` free fn signature change

**v2.1 spec** (inferred): free fns return `Result<stoolap::Database, SubstrateError>` (the legacy signature)

**v3.0 actual**: free fns return `Result<octo_storage_core::Database, SubstrateError>` (newtype)

**Rationale**: The free fns were the substrate's original constructor surface
(S2 landed before the newtype refactor). After the newtype refactor, they
must return the newtype or consumers end up holding the inner `stoolap::Database`
directly (defeating the newtype boundary). Updated in commit `996f9cd1` chain.

## Cargo gate verification

```text
$ cargo build -p octo-vault -p quota-router-storage -p octo-storage-core -p octo-storage --all-targets
Finished `dev` profile [unoptimized + debuginfo] target(s)

$ cargo test -p octo-vault -p quota-router-storage -p octo-storage-core -p octo-storage --tests --lib
test result: ok. 1 + 94 + 4 + 5 + 3 + 10 + 5 + 15 + 3 + 198 + 2 + 2 + 3 + 5 + 23 + 4 + 25 = 401 tests passed

$ cargo clippy -p octo-vault -p quota-router-storage -p octo-storage-core -p octo-storage --all-targets -- -D warnings
Finished `dev` profile [unoptimized + debuginfo] target(s)

$ cargo fmt --all -- --check
(zero diffs)
```

## RFC compliance — §Layer B TYPE Renames

| Requirement | Status |
|-------------|--------|
| `stoolap::Database` → substrate `Database` (all sites) | ✓ (42 sites renamed) |
| Field types renamed | ✓ |
| Constructor calls renamed | ✓ |
| Doc comment backticks renamed | ✓ |
| `_legacy_*` deprecated aliases used for legacy symbols | ✓ |
| Consumer crates drop direct `stoolap` dep | ✗ (D1 deviation; deferred to 0206-011b) |

## Next missions in DAG

- `0206-008` — Layer B TYPE renames expansion (8+ other crates: octo-reputation, octo-matrix-session-store, octo-cap-macaroon-vault, octo-adapter-whatsapp, octo-adapter-telegram-mtproto, quota-router-core, quota-router-sm-engine, quota-router-cli, octo-whatsapp) — same D1 deviation applies
- `0206-003 v3.0` — HolderRegistry + StoolapDidRegistry trait moves
- `0206-005` (parallel) — octo-ident-storage crate
- `0206-006` (parallel) — cipherocto-policy rename
- `0206-009` — 5 adapter crates (depends on 0206-002 + 0206-003 + 0206-005 + 0206-006)
- `0206-010` — per-adapter fixtures

## Termination

✓ Mission AC gates green (A7 paths 1+2 closed; D1 deviation documented)
✓ Cargo build + test + clippy + fmt all clean
✓ 42 sites renamed across 9 source files + 3 test files
✓ Substrate cross-fix: Clone derive + open_in_memory return type
✓ Cargo.toml deviation documented inline

Ready for commit.
