---
name: 0206-003-trait-moves
description: Open 2026-08-20; RFC-0206 v2.0 §Adapter Crate List trait moves — HolderRegistry: quota-router-storage → octo-cap-macaroon; StoolapDidRegistry impl: quota-router-storage → octo-ident-storage.
metadata:
  node_type: mission
  type: project
  originSessionId: 9a316ae1-cb15-46f4-801f-834acacd23ae
  created: 2026-08-20T00:00:00.000Z
---

# Mission `0206-003-trait-moves` — OPEN 2026-08-20

## Scope

Apply RFC-0206 v2.0 §Adapter Crate List trait + impl moves. Closes TV-0206-A8 gate (HolderRegistry declared in `octo-cap-macaroon/holder_registry.rs:33`, NOT quota-router-storage).

Covers two moves:

### Move 1 — HolderRegistry trait

- **From:** `crates/quota-router-storage/src/holder_registry.rs:33` (today: trait + impl co-located)
- **To:** `crates/octo-cap-macaroon/src/holder_registry.rs:33` (declarer: octo-cap-macaroon)
- **Action:** `git mv crates/quota-router-storage/src/holder_registry.rs crates/octo-cap-macaroon/src/holder_registry.rs`; update module declaration in `crates/octo-cap-macaroon/src/lib.rs`; remove from `crates/quota-router-storage/src/lib.rs`
- **Re-export:** `crates/quota-router-storage/src/lib.rs` adds `pub use octo_cap_macaroon::HolderRegistry;` for back-compat
- **TYPE renames in moved file:** `stoolap::Database` → `octo_storage_core::Database` (per `0206-002-layer-b-type-renames`)

### Move 2 — StoolapDidRegistry impl

- **From:** `crates/quota-router-storage/src/stoolap_did_registry.rs:139`
- **To:** `crates/octo-ident-storage/src/did_registry.rs:139` (new crate, owned by `0206-004-adapter-crates` Mission; this mission writes the impl at the new path assuming crate exists)
- **Pre-move action:** trait declared in `octo-ident/` (per RFC-0010 v1.3 storage extension); impl currently in quota-router-storage moves to octo-ident-storage per §Adapter Crate List row 3 note
- **Re-export:** `crates/quota-router-storage/src/lib.rs` adds `pub use octo_ident_storage::StoolapDidRegistry;` for back-compat (consumers stay on quota-router-storage import path)

## Acceptance Criterion

- TV-0206-A8 gate: `rg '^\s*pub trait\s+HolderRegistry' crates/` returns `crates/octo-cap-macaroon/src/holder_registry.rs:33` ONLY (zero hits in quota-router-storage)
- `crates/octo-ident-storage/src/did_registry.rs` exists (or co-located `crates/octo-ident/src/did_registry_storage.rs` if octo-ident-storage crate not yet landed); `StoolapDidRegistry` impl present at line 139 (or equivalent path)
- `cargo build --workspace --all-targets` green
- `cargo test --workspace --lib` green
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` green

## Files / Artifacts

- `git mv crates/quota-router-storage/src/holder_registry.rs → crates/octo-cap-macaroon/src/holder_registry.rs`
- New: `crates/octo-ident-storage/src/did_registry.rs` (or equivalent — depends on `0206-004-adapter-crates` ordering)
- Edit: `crates/quota-router-storage/src/lib.rs` (remove module, add re-export)
- Edit: `crates/octo-cap-macaroon/src/lib.rs` (add module declaration)
- Edit: `crates/octo-cap-macaroon/Cargo.toml` (add `octo-storage-core` dep)

## Cross-references

- RFC-0206 v2.0 §Adapter Crate List (rows: HolderRegistry + DidRegistry)
- RFC-0206 v2.0 TV-0206-A8
- RFC-0010 v1.3 storage extension (DidRegistry trait origin)
- Mission `0206-001-substrate-newtype` (substrate must exist)
- Mission `0206-002-layer-b-type-renames` (TYPE renames in moved files)

## Out of scope

- 4 new trait declarations (VaultStore, ReputationStore, SessionStore, PolicyStore — owned by `0206-004-adapter-crates`)
- 5 adapter crates on disk (owned by `0206-004-adapter-crates`)

## Dependencies

- `0206-001-substrate-newtype` (substrate Database type)
- `0206-004-adapter-crates` (target crate for StoolapDidRegistry impl — parallel or earlier)
