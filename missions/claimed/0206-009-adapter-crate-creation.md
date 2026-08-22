---
name: 0206-009-adapter-crate-creation
description: Open 2026-08-20 v1.0; — 5 NEW adapter crates (octo-vault-storage, octo-reputation-storage, octo-cap-macaroon-vault-storage, octo-matrix-session-store-storage, octo-policy-storage) + build_allowlist() + register() helper per adapter. Trait declarations (VaultStore, ReputationStore, VaultLookup, SessionStore, PolicyStore) remain in the respective owner crates per RFC §Adapter Crate List lines 181+183. Replaces retired 0206-004-adapter-crates (split into 0206-009 + 0206-010 per R3 structural analysis).
metadata:
  node_type: mission
  type: project
  originSessionId: 9a316ae1-cb15-46f4-801f-834acacd23ae
  created: 2026-08-20T00:00:00.000Z
  v: "1.0"
  supersedes: null
  depends_on:
    - 0206-001-substrate-newtype
    - 0206-003-trait-moves
    - 0206-005-octoident-storage-crate
    - 0206-006-cipherocto-policy-rename-alignment
    - RFC-0205
    - RFC-0206
    # R13 fix: DAG ordering per plan v1.1 §DAG order L51-67. Note that 0206-008 is NOT listed as a direct dep
    # because it is upstream of 0206-003 (0206-008 → 0206-003 → 0206-009 transitive closure); listing 0206-008
    # here would create a redundant dep. 0206-003's `depends_on:` retains 0206-008 as the transitive handoff.
    # 0206-005 + 0206-006 are parallel branches feeding 0206-009 (per plan v1.1 §DAG), retained as direct deps.
    # Verified by R13 lens F-R12-LENS-CROSS-CONSISTENCY-005: DAG no-cycles invariant per BLUEPRINT.md §Cross-RFC Consistency Checklist.
phase: 1.7
layer: B
rfc_authority: RFC-0206
tvs:
  - TV-0206-A6
  - TV-0206-A9
  - TV-0206-A10
status: done
---

# Mission `0206-009-adapter-crate-creation` v1.0 — OPEN 2026-08-20

## Scope

Land : 5 NEW per-owner adapter crates. Closes TV-0206-A6, TV-0206-A9(a), TV-0206-A10 gates. Per-adapter fixture suites (drop_table_negative + namespace_guard + 4 adversarial per RFC §Format Bypass Defense) are owned by `0206-010-per-adapter-fixtures`.

## v1.0 Landing (this mission)

5 NEW adapter crates created:

| Crate | Owner crate | Adapter ID | Tables |
|-------|-------------|------------|--------|
| `crates/octo-vault-storage/` | `octo-vault` | `octo-vault-storage/v1` | `vaults` |
| `crates/octo-reputation-storage/` | `octo-reputation` | `octo-reputation-storage/v1` | `reputation_signals` |
| `crates/octo-cap-macaroon-vault-storage/` | `octo-cap-macaroon` + `octo-vault-storage` | `octo-cap-macaroon-vault-storage/v1` | `vaults` (read-side) |
| `crates/octo-matrix-session-store-storage/` | `octo-matrix-session-store` | `octo-matrix-session-store-storage/v1` | `matrix_sessions` |
| `crates/octo-policy-storage/` | `octo-policy` | `octo-policy-storage/v1` | `policy_objects` |

Each adapter crate:
- `Cargo.toml`: `octo-storage-core = { path = "../octo-storage-core" }` + owner-crate dep; NO direct `stoolap` dep (TV-0206-A9(a))
- `src/lib.rs`: `build_allowlist()` + `<X>StoreAdapter` struct + `register()` helper
- `tests/register_roundtrip.rs`: canonical register + execute_checked + typed INSERT/SELECT round-trip (TV-0206-A10)

## AC gates

| Gate | Status | Evidence |
|------|--------|----------|
| TV-0206-A6 (5 dirs on disk) | PASS | `test -d` exits 0 for all 5 adapter crates |
| TV-0206-A9(a) (no stoolap dep) | PASS | `rg '^\s*stoolap\s*=' crates/octo-*-storage/Cargo.toml` empty |
| TV-0206-A10 (register_roundtrip fixtures) | PASS | `ls .../tests/ \| grep -c register_roundtrip` = 5 |
| `cargo build --workspace --all-targets` | PASS | 13m 01s, exit 0 |
| `cargo test -p octo-vault-storage -p octo-reputation-storage -p octo-cap-macaroon-vault-storage -p octo-matrix-session-store-storage -p octo-policy-storage` | PASS | all green |
| `cargo clippy -p <all 5> --all-targets -- -D warnings` | PASS | 0 warnings |
| `cargo fmt --all -- --check` | PASS | 0 diff |

## Files / Artifacts

- New: `crates/octo-vault-storage/Cargo.toml` + `src/lib.rs` + `tests/register_roundtrip.rs`
- New: `crates/octo-reputation-storage/Cargo.toml` + `src/lib.rs` + `tests/register_roundtrip.rs`
- New: `crates/octo-cap-macaroon-vault-storage/Cargo.toml` + `src/lib.rs` + `tests/register_roundtrip.rs`
- New: `crates/octo-matrix-session-store-storage/Cargo.toml` + `src/lib.rs` + `tests/register_roundtrip.rs`
- New: `crates/octo-policy-storage/Cargo.toml` + `src/lib.rs` + `tests/register_roundtrip.rs`
- Move: `missions/open/0206-004-adapter-crates.md` → `missions/retired/0206-004-adapter-crates-v21.md` (split per R3 structural analysis)

## Cross-references

- 
- (generic register helper form)
- -Cuts lines 333-338 (REQUIRED `octo-storage-core` dep in adapter Cargo.toml)
- (adversarial fixtures — owned by 0206-010)
- Mission `0206-001-substrate-newtype` (substrate `Database` type + TypedStatement enum + AdapterAllowlist + facade `crates/octo-storage/`)
- Mission `0206-003-trait-moves` (HolderRegistry + StoolapDidRegistry moves must land before adapter trait declarations)
- Mission `0206-005-octoident-storage-crate` (6th adapter crate — out of scope)
- Mission `0206-006-cipherocto-policy-rename-alignment` (directory rename must land before `octo-policy-storage/`)
- Retired mission `0206-004-adapter-crates` (v2.1 split into 0206-009 + 0206-010 per R3)

## Out of scope

- Per-adapter fixture suites (`drop_table_negative.rs` + `namespace_guard.rs` + 4 adversarial per RFC §Format Bypass Defense) — owned by `0206-010-per-adapter-fixtures`
- Substrate newtype impl + facade `crates/octo-storage/` migration — owned by `0206-001-substrate-newtype`
- `octo-ident-storage/` adapter crate — owned by `0206-005-octoident-storage-crate`
- `cipherocto-policy → octo-policy` directory rename — owned by `0206-006-cipherocto-policy-rename-alignment`

## Dependencies

- `0206-001-substrate-newtype` (substrate must exist for `Database` type + TypedStatement enum + AdapterAllowlist)
- `0206-003-trait-moves` (HolderRegistry + StoolapDidRegistry moves must land first per ordering constraint)
- `0206-005-octoident-storage-crate` (sibling — 6th adapter crate)
- `0206-006-cipherocto-policy-rename-alignment` (directory rename before `octo-policy-storage/`)
- RFC-0205 (coupled pair per BLUEPRINT.md §Dependency Validation Rules → 2-Cycle Atomic Promotion rule 5)
- RFC-0206 (acceptance precondition per BLUEPRINT.md rule 5)

## Version History

| Version | Date       | Change                                       |
| ------- | ---------- | -------------------------------------------- |
| v1.0    | 2026-08-20 | Initial filing; 5 adapter crates landed     |