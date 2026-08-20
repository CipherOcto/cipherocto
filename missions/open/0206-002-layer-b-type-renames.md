---
name: 0206-002-layer-b-type-renames
description: Open 2026-08-20; RFC-0206 v2.0 §Layer B TYPE Renames — 29 sites across quota-router-storage (26) + octo-vault (3) rename `stoolap::Database` → `octo_storage_core::Database`.
metadata:
  node_type: mission
  type: project
  originSessionId: 9a316ae1-cb15-46f4-801f-834acacd23ae
  created: 2026-08-20T00:00:00.000Z
---

# Mission `0206-002-layer-b-type-renames` — OPEN 2026-08-20

## Scope

Apply RFC-0206 v2.0 §Layer B TYPE Renames across 29 sites. Closes TV-0206-A7 gate (zero `stoolap::Database` outside substrate + test-harness exemption).

Sites per RFC-0206 v2.0 §Layer B TYPE Renames table (29 total):

**quota-router-storage (26 sites):**

- `src/ask_repo.rs:42`, `:189`
- `src/slash_store.rs:67`, `:289`
- `src/migrations.rs:14`
- `src/stoolap_did_registry.rs:139`, `:201` (impl moves to `0206-003-trait-moves`)
- `src/holder_registry.rs:33` (trait declaration site), `:81`, `:155`
- `src/spend_ledger.rs:48`, `:121`
- `src/settlement_event_repo.rs:36`, `:96`
- 12 more sites TBD on per-file audit

**octo-vault (3 sites):**

- `src/lib.rs:351`, `:378`, `:395`

**Rename pattern:** `stoolap::Database` → `octo_storage_core::Database` (TYPE positions only: function arg, field type, return type, struct generic parameter, trait method signature); `Arc<stoolap::Database>` → `Arc<octo_storage_core::Database>`; `&stoolap::Database` → `&octo_storage_core::Database`; `&mut stoolap::Database` → `&mut octo_storage_core::Database`.

**Cargo.toml deps update:** each renamed crate adds `octo-storage-core = { path = "../octo-storage-core" }` (or workspace path if defined); may NOT drop `stoolap` dep if crate uses any Layer A handle directly (escape hatch via `From<Database>` at typed-query allowlist sites per RFC-0206 v2.0 §Substrate Newtype Refactor).

## Acceptance Criterion

- TV-0206-A7 gate: `rg 'stoolap::Database' crates/quota-router-storage crates/octo-vault/src crates/octo-ident/src crates/octo-cap-macaroon/src 2>/dev/null | wc -l` equals 0 (with test-harness exemption per `crates/octo-storage-core/test-harness-allowlist.toml`)
- `cargo build --workspace --all-targets` green
- `cargo test --workspace --lib` green
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` green
- `cargo fmt --all -- --check` green
- No new direct `stoolap` deps added to consumer crates (verify `rg '^\s*stoolap\s*=' crates/quota-router-storage/Cargo.toml crates/octo-vault/Cargo.toml` exits 1)

## Files / Artifacts

- Edit: `crates/quota-router-storage/Cargo.toml` (add `octo-storage-core` dep if not present)
- Edit: `crates/octo-vault/Cargo.toml` (add `octo-storage-core` dep if not present)
- Edit: 29 source files (TYPE rename)
- Edit: any call-site that passes a `stoolap::Database` to a renamed function (replace with `octo_storage_core::Database`)

## Cross-references

- RFC-0206 v2.0 §Layer B TYPE Renames
- RFC-0206 v2.0 TV-0206-A7, TV-0206-A9
- Mission `0206-001-substrate-newtype` (substrate must exist for type to be importable)

## Out of scope

- Substrate newtype impl (owned by `0206-001-substrate-newtype`)
- HolderRegistry trait move (owned by `0206-003-trait-moves` — distinct concern)
- StoolapDidRegistry impl move (owned by `0206-003-trait-moves`)
- 5 adapter crates (owned by `0206-004-adapter-crates`)

## Dependencies

- `0206-001-substrate-newtype` (substrate Database type must exist)
