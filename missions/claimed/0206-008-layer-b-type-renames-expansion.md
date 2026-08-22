---
name: 0206-008-layer-b-type-renames-expansion
description: Apply 8+ other crates (octo-reputation, octo-matrix-session-store, octo-cap-macaroon-vault, octo-adapter-whatsapp, octo-adapter-telegram-mtproto, quota-router-core, quota-router-sm-engine, quota-router-cli, octo-whatsapp). 60+ TYPE sites + Cargo.toml dep drops.
metadata:
  node_type: mission
  type: project
  originSessionId: 9a316ae1-cb15-46f4-801f-99c2545bccf7
  created: 2026-08-20T00:00:00.000Z
  v: "1.0"
  supersedes: null
  depends_on:
    - 0206-001-substrate-newtype
    - 0206-002-layer-b-type-renames
    - RFC-0205
    - RFC-0206
phase: 1.4 non-RFC part
layer: B
rfc_authority: RFC-0206
tvs:
  - TV-0206-A7
  - TV-0206-A9
status: done
---

# Mission `0206-008-layer-b-type-renames-expansion` v1.0 — CLAIMED 2026-08-20

## Scope

Apply 8+ other consumer crates
beyond the quota-router-storage + octo-vault scope of mission 0206-002.
Closes TV-0206-A7 paths 5-13 (the remaining crate paths).

### Explicit Sites (regenerated from disk 2026-08-20)

**89 `stoolap::Database` sites across 38 files** (rg count run 2026-08-20).
Plan estimated 60+ sites; actual is 89 (undercount by ~50%).

**octo-reputation (9 sites / 2 files)**:

- `src/store/stoolap.rs` — 6
- `src/migrations.rs` — 3

**octo-matrix-session-store (5 sites / 2 files)**:

- `src/store.rs` — 4
- `src/schema.rs` — 1

**octo-cap-macaroon-vault (2 sites / 2 files)**:

- `src/lib.rs` — 1
- `src/octo_vault_lookup.rs` — 1

**octo-adapter-whatsapp (17 sites / 7 files)**:

- `src/store.rs` — 6
- `src/bin/whatsapp_connect_trace.rs` — 3
- `src/bin/whatsapp_session_introspect.rs` — 3
- `src/bin/cleanup_test_groups.rs` — 1
- `src/bin/inspect_session_db.rs` — 1
- `src/bin/whatsapp_ik_session_probe.rs` — 1
- `tests/r14_h1_upsert_verify_test.rs` — 1

**octo-adapter-telegram-mtproto (1 site / 1 file)**:

- `src/session.rs` — 1

**octo-whatsapp (10 sites / 9 files)**:

- `examples/diag_count.rs` — 1
- `src/ipc/handlers/daemon_search.rs` — 1
- `src/ipc/handlers/messages_list_ephemeral.rs` — 1
- `src/ipc/handlers/messages_list_unavailable.rs` — 1
- `src/ipc/handlers/messages_read_view_once.rs` — 2
- `src/ipc/handlers/sql.rs` — 1
- `src/query/schema.rs` — 1
- `src/query/service.rs` — 1
- `src/query/subsystem.rs` — 1

**quota-router-cli (1 site / 1 file)**:

- `src/commands.rs` — 1

**quota-router-core (45 sites / 10 files)**:

- `src/admin.rs` — 2
- `src/balance.rs` — 1
- `src/cache.rs` — 7
- `src/health.rs` — 1
- `src/middleware.rs` — 1
- `src/proxy.rs` — 18
- `src/schema.rs` — 2
- `src/storage.rs` — 5
- `src/auth/sso/blacklist_stoolap.rs` — 5
- `src/auth/sso/mapper_stoolap.rs` — 1
- `benches/key_hash_storage_bench.rs` — 2
- `tests/e2e_proxy.rs` — 1
- `tests/e2e_wiremock_faults.rs` — 3

**quota-router-sm-engine (3 sites / 2 files)**:

- `src/store.rs` — 1
- `src/schema.rs` — 2

### Cargo.toml Deps Update

Per D1 deviation from mission 0206-002: `stoolap` direct dep RETAINED in
consumer crates that need `stoolap::ResultRow` / `stoolap::ApiTransaction`
/ `stoolap::Rows` / `stoolap::Error` access. The substrate redesign v3.0
does not yet re-export these types; `0206-011b` v2.2 amendment is proper
scope to add `pub mod stoolap` re-export. Until then, retain direct deps.

**Affected Cargo.toml files (10)**:

- `octo-adapter-telegram-mtproto/Cargo.toml`
- `octo-adapter-whatsapp/Cargo.toml`
- `octo-matrix-session-store/Cargo.toml`
- `octo-reputation/Cargo.toml`
- `octo-whatsapp/Cargo.toml`
- `quota-router-cli/Cargo.toml`
- `quota-router-core/Cargo.toml`
- `quota-router-sm-engine/Cargo.toml`
- (`octo-core/Cargo.toml` — has stoolap dep but no `stoolap::Database` code ref; verify during audit)
- (`octo-cap-macaroon-vault/Cargo.toml` — no stoolap direct dep; uses octo-storage-core)

Each Cargo.toml dep-block updated with inline comment documenting D1
deviation rationale (matching the pattern in quota-router-storage +
octo-vault from mission 0206-002).

### Rename Pattern

- `stoolap::Database` → `octo_storage_core::Database` (TYPE positions AND doc backticks)
- `Arc<stoolap::Database>` → `Arc<octo_storage_core::Database>`
- `&stoolap::Database` → `&octo_storage_core::Database`
- `&mut stoolap::Database` → `&mut octo_storage_core::Database`
- `stoolap::Database::open(path)` → `Database::open(path)` or `octo_storage_core::open(path)`
- `stoolap::Database::open_in_memory()` → `Database::open_in_memory()` or `octo_storage_core::open_in_memory()`

### Other References

- `stoolap::ResultRow`, `stoolap::ApiTransaction`, `stoolap::Rows`, `stoolap::Error` — RETAINED as-is (D1 deviation; substrate does not re-export)
- `FromValue` trait (doc-comment only) — RETAINED
- `stoolap::Value` (column-typed) — RETAINED

## Acceptance Criterion

- TV-0206-A7 paths 5-13 closed (zero `stoolap::Database` in 9 crate src dirs):
  - Path 5: `rg 'stoolap::Database' crates/octo-reputation/src | wc -l` equals 0
  - Path 6: `rg 'stoolap::Database' crates/octo-matrix-session-store/src | wc -l` equals 0
  - Path 7: `rg 'stoolap::Database' crates/octo-cap-macaroon-vault/src | wc -l` equals 0
  - Path 8: `rg 'stoolap::Database' crates/octo-adapter-whatsapp/src | wc -l` equals 0
  - Path 9: `rg 'stoolap::Database' crates/octo-adapter-whatsapp/tests | wc -l` equals 0
  - Path 10: `rg 'stoolap::Database' crates/octo-adapter-telegram-mtproto/src | wc -l` equals 0
  - Path 11: `rg 'stoolap::Database' crates/octo-whatsapp/src | wc -l` equals 0
  - Path 12: `rg 'stoolap::Database' crates/octo-whatsapp/examples | wc -l` equals 0
  - Path 13: `rg 'stoolap::Database' crates/quota-router-cli/src | wc -l` equals 0
  - Path 14: `rg 'stoolap::Database' crates/quota-router-core/src | wc -l` equals 0
  - Path 15: `rg 'stoolap::Database' crates/quota-router-core/tests | wc -l` equals 0
  - Path 16: `rg 'stoolap::Database' crates/quota-router-core/benches | wc -l` equals 0
  - Path 17: `rg 'stoolap::Database' crates/quota-router-sm-engine/src | wc -l` equals 0
- Cargo.toml dep-block comments updated per D1 deviation pattern (inline doc explaining 0206-011b amendment scope)
- 89 sites renamed across 38 files
- `cargo build --workspace --all-targets` green (workspace-wide; substrate redesign cascades to all consumers)
- `cargo test --workspace --lib` green
- `cargo clippy --workspace --all-targets --features full -- -D warnings` green (per `quota-router-core-feature-mutex` memory)
- `cargo fmt --all -- --check` green

## Files / Artifacts

- Edit: 10 Cargo.toml files (add D1 deviation comment blocks; retain direct dep)
- Edit: 38 source/test/bench files (rename `stoolap::Database` → `octo_storage_core::Database`)
- New: `docs/audits/0206-008-layer-b-type-renames-expansion-audit.md`

## Cross-references

- Mission `0206-002-layer-b-type-renames` (sibling; same D1 deviation pattern)
- Mission `0206-001-substrate-newtype` (substrate `Database` newtype)

## Out of scope

- Substrate newtype impl (owned by 0206-001)
- Trait moves (owned by 0206-003)
- Adapter crate creation (owned by 0206-009)
- `stoolap::ResultRow`/`stoolap::ApiTransaction` re-export (owned by 0206-011b v2.2 amendment)
- Dropping direct `stoolap` dep from consumer crates (D1 deviation; deferred to 0206-011b)

## Dependencies

- `0206-001-substrate-newtype` (substrate `Database` type must exist)
- `0206-002-layer-b-type-renames` (sibling mission; same rename pattern)
- RFC-0206 (acceptance precondition)

## Version History

| Version | Date | Change |
|---------|------|--------|
| v1.0 | 2026-08-20 | Initial filing (NEW per substrate redesign plan); 89 sites / 38 files enumerated from disk; D1 deviation pattern inherited from 0206-002 |
| v3.0    | 2026-08-22 | Phase 3 close-out per long-horizon plan v1.5 §Mission layout. AC verification per memory card `mission-0206-008-layer-b-type-renames-expansion-status.md`: LANDED 927008d6 (2026-08-20). 89 sites / 38 files / 9 consumer crates TYPE renames applied; TV-A7 paths 5-17 closed. Mission YAML edits per R10.5 scope discipline. Status transitions open→done. |
