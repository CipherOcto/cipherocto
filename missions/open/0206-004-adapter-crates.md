---
name: 0206-004-adapter-crates
description: Open 2026-08-20; RFC-0206 v2.0 §Adapter Crate List — 5 adapter crates (octo-vault-storage, octo-reputation-storage, octo-cap-macaroon-vault-storage, octo-matrix-session-store-storage, octo-policy-storage) + facade + 4 trait declarations + per-adapter fixtures.
metadata:
  node_type: mission
  type: project
  originSessionId: 9a316ae1-cb15-46f4-801f-834acacd23ae
  created: 2026-08-20T00:00:00.000Z
---

# Mission `0206-004-adapter-crates` — OPEN 2026-08-20

## Scope

Land RFC-0206 v2.0 §Adapter Crate List: 5 per-owner adapter crates + `crates/octo-storage/` facade + 4 trait declarations + per-adapter test fixtures. Closes TV-0206-A6, TV-0206-A10, TV-0206-A11, TV-0206-A12, TV-0206-A14 gates.

### A. Facade

- `crates/octo-storage/Cargo.toml`: declares `octo-storage-core = { path = "../octo-storage-core" }` ONLY
- `crates/octo-storage/src/lib.rs`: 4-item re-export set per RFC-0206 v2.0 §Cargo.toml Templates Layer B facade: `Database`, `TypedStatement`, `AdapterAllowlist`, `register`
- Wildcard detector: `rg '\b\*\s*[,;}]' crates/octo-storage/src/lib.rs` MUST equal 0 (TV-0206-A14)
- `register<V: OwnerTrait>(db, store)` helper (or 5 per-trait register fns if generics awkward)

### B. 5 Adapter Crates

For each of `octo-vault-storage`, `octo-reputation-storage`, `octo-cap-macaroon-vault-storage`, `octo-matrix-session-store-storage`, `octo-policy-storage`:

- `Cargo.toml`: declares `octo-storage = { path = "../octo-storage" }` + `octo-storage-core = { path = "../octo-storage-core" }`; NO direct `stoolap` dep (TV-0206-A9(a))
- `src/lib.rs`: declares trait (NEW) + impl
- `tests/register_roundtrip.rs`: register + select + insert round-trip TV
- `tests/drop_table_rejected.rs`: `DdlRegistered(DropTable(...))` → `SubstrateError::DdlNotInAllowlist`
- `tests/namespace_guard.rs`: workspace query outside adapter namespace → `SubstrateError::TableNotInNamespace`

### C. 4 Trait Declarations

Per RFC-0206 v2.0 §Adapter Crate List:

- `VaultStore` (declarer: octo-vault, impl: octo-vault-storage)
- `ReputationStore` (declarer: octo-reputation, impl: octo-reputation-storage)
- `VaultLookup` (declarer: octo-cap-macaroon, impl: octo-cap-macaroon-vault-storage — trait move from `octo-cap-macaroon/src/vault_lookup.rs`)
- `SessionStore` (declarer: octo-matrix-session-store, impl: octo-matrix-session-store-storage)
- `PolicyStore` (declarer: octo-policy, impl: octo-policy-storage)

(NOTE: 5 adapters, but `VaultLookup` already declared today in `octo-cap-macaroon/src/vault_lookup.rs:33`; count = 4 NEW + 1 move.)

### D. Workspace Registration

- Update workspace root `Cargo.toml` `[workspace] members` to include 5 new adapter crates
- Naming resolution: crate name is `octo-policy` (per workspace `[workspace] members`); `cipherocto-policy` is internal alias only (per RFC-0206 v2.0 §Summary Updates vs v1.8)

## Acceptance Criterion

- 5 adapter crate directories on disk (`crates/octo-{vault,reputation,cap-macaroon-vault,matrix-session-store,policy}-storage/`)
- `crates/octo-storage/` facade exists with 4-item re-export set
- TV-0206-A6 gate: 5 directory existence check green
- TV-0206-A10 gate: 5 `register_roundtrip.rs` test files on disk
- TV-0206-A11 gate: 5 `drop_table_rejected.rs` test files
- TV-0206-A12 gate: 5 `namespace_guard.rs` test files
- TV-0206-A14 gate: 0 wildcard `pub use` in facade
- Workspace `[workspace] members` updated; `cargo build --workspace --all-targets` green
- `cargo test --workspace --lib` green
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` green
- `cargo fmt --all -- --check` green

## Files / Artifacts

- New: `crates/octo-storage/Cargo.toml` + `src/lib.rs`
- New: `crates/octo-vault-storage/Cargo.toml` + `src/lib.rs` + `tests/{register_roundtrip,drop_table_rejected,namespace_guard}.rs`
- New: `crates/octo-reputation-storage/Cargo.toml` + `src/lib.rs` + 3 tests
- New: `crates/octo-cap-macaroon-vault-storage/Cargo.toml` + `src/lib.rs` + 3 tests
- New: `crates/octo-matrix-session-store-storage/Cargo.toml` + `src/lib.rs` + 3 tests
- New: `crates/octo-policy-storage/Cargo.toml` + `src/lib.rs` + 3 tests
- Edit: workspace root `Cargo.toml` `[workspace] members`
- Edit: `crates/octo-vault/src/lib.rs` (declare `VaultStore` trait)
- Edit: `crates/octo-reputation/src/lib.rs` (declare `ReputationStore` trait)
- Edit: `crates/octo-cap-macaroon/src/vault_lookup.rs` (move trait to new adapter crate)
- Edit: `crates/octo-matrix-session-store/src/lib.rs` (declare `SessionStore` trait)
- Edit: `crates/octo-policy/src/lib.rs` (declare `PolicyStore` trait)

## Cross-references

- RFC-0206 v2.0 §Three-Tier Architecture
- RFC-0206 v2.0 §Cargo.toml Templates Layer B facade
- RFC-0206 v2.0 §Adapter Crate List
- RFC-0206 v2.0 §Wiring Pattern
- RFC-0206 v2.0 TV-0206-A6, A10, A11, A12, A14
- Mission `0206-001-substrate-newtype` (substrate Database type)
- Mission `0206-003-trait-moves` (HolderRegistry + StoolapDidRegistry moves — parallel concern)

## Out of scope

- Substrate newtype impl (owned by `0206-001`)
- 29 Layer B TYPE renames (owned by `0206-002`)
- HolderRegistry + StoolapDidRegistry moves (owned by `0206-003`)

## Dependencies

- `0206-001-substrate-newtype` (substrate must exist for `Database` type)
