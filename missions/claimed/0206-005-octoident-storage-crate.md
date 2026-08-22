---
name: 0206-005-octoident-storage-crate
description: Open 2026-08-20; 3 — create new adapter crate `crates/octo-ident-storage/` to hold StoolapDidRegistry impl at `src/did_registry.rs:139`. Required target for 0206-003-trait-moves.
metadata:
  node_type: mission
  type: project
  originSessionId: 9a316ae1-cb15-46f4-801f-99c2545bccf7
  created: 2026-08-20T00:00:00.000Z
  v: "1.0"
  supersedes: null
  depends_on:
    - 0206-001-substrate-newtype
    - RFC-0206
phase: orthogonal
layer: B
rfc_authority: RFC-0206
tvs:
  - TV-0206-A6
  - TV-0206-A9
  - TV-0206-A10
  - TV-0206-A11
  - TV-0206-A12
status: done
---

# Mission `0206-005-octoident-storage-crate` — OPEN 2026-08-20

## Scope

Create new adapter crate `crates/octo-ident-storage/` per 3. This is the SOLE target crate for the StoolapDidRegistry impl move (owned by `0206-003-trait-moves`).

Covers:

- **New crate directory** `crates/octo-ident-storage/{Cargo.toml,src/lib.rs,src/did_registry.rs}`
- **Cargo.toml** declares `octo-storage = { path = "../octo-storage" }` (facade, per RFC §Cargo.toml Cross-Cuts — NOT direct substrate) + `octo-ident = { path = "../octo-ident" }` (trait declarer per RFC-0010 storage extension) + workspace deps as needed
- **NO direct `stoolap` dep** (per — substrate is sole fork consumer; verified by TV-0206-A9(a))
- **`src/lib.rs`** re-exports `StoolapDidRegistry` from `crate::did_registry` for back-compat with quota-router-storage consumers
- **`src/did_registry.rs:139`** hosts the impl block `impl StoolapDidRegistry for StoolapDidRegistryImpl` (moved from `crates/quota-router-storage/src/stoolap_did_registry.rs:139` per `0206-003-trait-moves`)
- **Workspace registration** via `Cargo.toml [workspace] members = ["crates/*"]` glob — directory creation alone is sufficient
- **Per-adapter fixture** at `tests/register_roundtrip.rs` per RFC §Format Bypass Defense substrate-level guard

## Acceptance Criterion

- `crates/octo-ident-storage/` directory on disk with `{Cargo.toml,src/lib.rs,src/did_registry.rs}`
- TV-0206-A9(a) gate: `rg '^\s*stoolap\s*=' crates/octo-ident-storage/Cargo.toml` exits 1
- TV-0206-A6 gate: `test -d crates/octo-ident-storage` exits 0
- TV-0206-A10 gate: `ls crates/octo-ident-storage/tests/ | grep -c register_roundtrip` equals 1
- Workspace build green: `cargo build --workspace --all-targets`
- `cargo test --workspace --lib` green
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` green (per `feedback_clippy_zero_warnings`)
- `cargo fmt --all -- --check` green
- TV-0206-A2 substrate newtype verified to exist (per `0206-001` dep)

## Files / Artifacts

- New: `crates/octo-ident-storage/Cargo.toml`
- New: `crates/octo-ident-storage/src/lib.rs`
- New: `crates/octo-ident-storage/src/did_registry.rs` (impl block lands here per `0206-003-trait-moves`)
- New: `crates/octo-ident-storage/tests/register_roundtrip.rs`
- New: `crates/octo-ident-storage/tests/drop_table_rejected.rs`
- New: `crates/octo-ident-storage/tests/namespace_guard.rs`

## Cross-references

- 3 (StoolapDidRegistry impl target)
- 
- RFC-0206 TV-0206-A6, A9(a), A10, A11, A12
- -Cuts (adapter deps via facade, not substrate)
- RFC-0010 storage extension (DidRegistry trait origin)
- RFC-0205 (coupled pair per BLUEPRINT.md §Dependency Validation Rules → 2-Cycle Atomic Promotion rule 5)
- Mission `0206-001-substrate-newtype` (substrate `Database` type must exist)
- Mission `0206-003-trait-moves` (writes impl at this crate's path)

## Out of scope

- StoolapDidRegistry impl block content (owned by `0206-003-trait-moves` — this mission creates the target directory only)
- 4 other adapter crates (owned by `0206-004-adapter-crates`)
- DidRegistry trait declaration (lives in `octo-ident/` per RFC-0010)

## Dependencies

- `0206-001-substrate-newtype` (substrate `Database` newtype must exist for impl signature)
- RFC-0206 (acceptance precondition per BLUEPRINT.md rule 5)
- RFC-0205 (coupled pair; substrate Cargo.toml pin lands before this crate's deps resolve)

## Version History

| Version | Date       | Change                                                            |
| ------- | ---------- | ----------------------------------------------------------------- |
| v1.0    | 2026-08-20 | Initial filing per R1 finding from `0206-003-trait-moves` CRIT #1 |
| v3.0    | 2026-08-22 | Phase 3 close-out per long-horizon plan v1.5 §Mission layout. AC verification per memory card : LANDED 7833125e (2026-08-20). octo-ident-storage adapter crate on disk at  (NO direct stoolap dep). Mission YAML edits per R10.5 scope discipline. Status transitions open→done. |
