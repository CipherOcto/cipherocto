---
name: 0206-003-trait-moves
description: Open 2026-08-20; — HolderRegistry: quota-router-storage → octo-cap-macaroon; StoolapDidRegistry impl: quota-router-storage → octo-ident-storage.
metadata:
  node_type: mission
  type: project
  originSessionId: 9a316ae1-cb15-46f4-801f-834acacd23ae
  created: 2026-08-20T12:00:00.000Z
  v: "3.0"
  supersedes: v2.1
  depends_on:
    - 0206-001-substrate-newtype
    - 0206-002-layer-b-type-renames
    - 0206-005-octoident-storage-crate
    - RFC-0205
    - RFC-0206
phase: 1.5 + 1.6
layer: B
rfc_authority: RFC-0206
tvs:
  - TV-0206-A8
---

# Mission `0206-003-trait-moves` — OPEN 2026-08-20 (v2.0)

## Scope

Apply + impl moves. Closes TV-0206-A8 gate (HolderRegistry declared in `octo-cap-macaroon/holder_registry.rs:33`, NOT quota-router-storage).

Covers two moves:

### Move 1 — HolderRegistry trait

- **From:** `crates/quota-router-storage/src/holder_registry.rs:33` (today: trait + impl co-located)
- **To:** `crates/octo-cap-macaroon/src/holder_registry.rs:33` (declarer: octo-cap-macaroon)
- **Action:** `git mv crates/quota-router-storage/src/holder_registry.rs crates/octo-cap-macaroon/src/holder_registry.rs`; update module declaration in `crates/octo-cap-macaroon/src/lib.rs`; remove from `crates/quota-router-storage/src/lib.rs`
- **Re-export:** `crates/quota-router-storage/src/lib.rs` adds `pub use octo_cap_macaroon::HolderRegistry;` for back-compat
- **TYPE renames in moved file:** `stoolap::Database` → `octo_storage_core::Database` (per `0206-002-layer-b-type-renames`)

### Move 2 — StoolapDidRegistry impl

- **From:** `crates/quota-router-storage/src/stoolap_did_registry.rs:139`
- **To:** `crates/octo-ident-storage/src/did_registry.rs:139` (new crate, owned by `0206-005-octoident-storage-crate` which MUST land first; this mission writes the impl at the new path)
- **Pre-move action:** trait declared in `octo-ident/` (per RFC-0010 storage extension); impl currently in quota-router-storage moves to octo-ident-storage per §Adapter Crate List row 3
- **Re-export:** `crates/quota-router-storage/src/lib.rs` adds `pub use octo_ident_storage::StoolapDidRegistry;` for back-compat (consumers stay on quota-router-storage import path)
- **VAULT LOOKUP AMBIGUITY CLARIFICATION:** This mission does NOT move the `VaultLookup` trait or its impl. `VaultLookup` trait + impl move is OWNED by `0206-004-adapter-crates` (per ). This mission only handles `HolderRegistry` (Move 1) and `StoolapDidRegistry` impl (Move 2).

### Ordering clause (R1 HIGH 2)

`0206-002-layer-b-type-renames` MUST complete BEFORE `0206-003-trait-moves` begins work. Rationale: this mission depends on the substrate `Database` newtype existing (`0206-001`) AND the type renames being applied (`0206-002`) so the moved files reference the correct types. If `0206-003` runs first, the moved files will reference stale `stoolap::Database` paths and the AC will fail at the `rg` TV gate.

`0206-005-octoident-storage-crate` MUST complete BEFORE `0206-003-trait-moves` Move 2. Rationale: the target directory `crates/octo-ident-storage/src/did_registry.rs` must exist on disk; this mission writes content into that path.

`0206-004-adapter-crates` runs in parallel (or later) — it owns the `VaultLookup` trait + impl move and the 4 new trait declarations, none of which this mission touches.

## Acceptance Criterion

- TV-0206-A8 gate: `rg '^\s*pub trait\s+HolderRegistry' crates/` returns `crates/octo-cap-macaroon/src/holder_registry.rs:33` ONLY (zero hits in quota-router-storage)
- `crates/octo-ident-storage/src/did_registry.rs` exists at the EXACT path (NO fallback to `crates/octo-ident/src/did_registry_storage.rs` — that path is FABRICATED and out of scope per RFC-0206)
- **TV-0206-A8a impl-move gate (R1 HIGH 1):** `rg 'impl\s+StoolapDidRegistry\s+for\s+StoolapDidRegistryImpl' crates/octo-ident-storage/src/did_registry.rs` exits 0 (impl block moved)
- **TV-0206-A8b impl-removed gate (R1 HIGH 1):** `rg 'impl\s+StoolapDidRegistry\s+for' crates/quota-router-storage/src/` exits 1 (impl removed from old location)
- `cargo build --workspace --all-targets` green
- `cargo test --workspace --lib` green
- `cargo clippy --workspace --all-targets --features full -- -D warnings` green (per `quota-router-core-feature-mutex` memory: NEVER use `--all-features` on this workspace)
- `cargo fmt --all -- --check` green (R1 MED 2)
- **DDL allowlist registration AC (R1 MED 4):** adapter store types register via `octo_storage::register::<StoreType>(db, store_arc)` per ; concrete verification: `rg 'octo_storage::register::<' crates/octo-cap-macaroon/src/holder_registry.rs crates/octo-ident-storage/src/did_registry.rs` exits 0 (both moved files call `octo_storage::register`); final cross-check happens in `0206-004-adapter-crates` adapter fixtures

## Files / Artifacts

- `git mv crates/quota-router-storage/src/holder_registry.rs → crates/octo-cap-macaroon/src/holder_registry.rs`
- New content: `crates/octo-ident-storage/src/did_registry.rs` (crate directory created by `0206-005`; this mission writes the impl at `:139`)
- Edit: `crates/quota-router-storage/src/lib.rs` (remove `holder_registry` module + `stoolap_did_registry` module, add re-exports `pub use octo_cap_macaroon::HolderRegistry;` and `pub use octo_ident_storage::StoolapDidRegistry;`)
- Edit: `crates/octo-cap-macaroon/src/lib.rs` (add `holder_registry` module declaration)
- Edit: `crates/octo-cap-macaroon/Cargo.toml` — add `octo-storage-core = { path = "../octo-storage-core" }` + add `octo-storage = { path = "../octo-storage" }` facade (per -Cuts)
- Edit: `crates/quota-router-storage/Cargo.toml` — remove `HolderRegistry` module entries (line 33 trait decl moves); add `pub use octo_cap_macaroon::HolderRegistry;` to lib.rs (per Move 1 step 'update module declaration in crates/octo-cap-macaroon/src/lib.rs')
- Edit: `crates/octo-ident-storage/Cargo.toml` — owned by `0206-005-octoident-storage-crate`; this mission does not edit Cargo.toml
- **Intra-crate use-site cleanup (R1 LOW 2):** Edit `crates/octo-ident/src/did_registry.rs` import sites to use `octo_ident_storage::StoolapDidRegistry` (per Move 2 re-export rule mandating quota-router-storage re-export)
- **Intra-crate use-site cleanup (R1 LOW 2):** Edit any `crates/*/src/*.rs` files that import `HolderRegistry` or `StoolapDidRegistry` to point at the new declarer crates

## Cross-references

- (rows: HolderRegistry + DidRegistry)
- RFC-0206 TV-0206-A8
- RFC-0010 storage extension (DidRegistry trait origin)
- Mission `0206-001-substrate-newtype` (substrate `Database` type)
- Mission `0206-002-layer-b-type-renames` (TYPE renames in moved files; MUST complete first)
- Mission `0206-005-octoident-storage-crate` (target crate for StoolapDidRegistry impl; MUST complete first)
- Mission `0206-004-adapter-crates` (owns VaultLookup trait + impl move; owns 4 new trait declarations; owns 5 adapter crates on disk; runs in parallel or later)

## Out of scope

- 4 new trait declarations (VaultStore, ReputationStore, SessionStore, PolicyStore — owned by `0206-004-adapter-crates`)
- 5 adapter crates on disk (owned by `0206-004-adapter-crates`)
- `VaultLookup` trait declaration + impl move (owned by `0206-004-adapter-crates`; per R1 MED 3 ambiguity clarification)
- DDL allowlist registration implementation (verified in `0206-004` adapter fixtures; per R1 MED 4)
- Crate directory creation for `crates/octo-ident-storage/` (owned by `0206-005`)
- Defensive boundary: `0206-002-layer-b-type-renames` MUST NOT modify `crates/quota-router-storage/src/holder_registry.rs:33` (line moves to octo-cap-macaroon per this mission)

## Dependencies

- `0206-001-substrate-newtype` (substrate `Database` type) — MUST complete BEFORE this mission starts
- `0206-002-layer-b-type-renames` (TYPE renames in moved files) — MUST complete BEFORE this mission starts
- `0206-005-octoident-storage-crate` (target crate for StoolapDidRegistry impl) — MUST complete BEFORE this mission starts
- `0206-004-adapter-crates` (parallel or later; owns VaultLookup + 4 new trait declarations + 5 adapter crates) — no ordering constraint
- RFC-0206 (acceptance precondition per BLUEPRINT.md rule 5)
- RFC-0205 (coupled pair)

## v2.1 Changes from v2.0

| Finding | Severity                                                                                                                       | Fix                                                                                                                                                                               |
| ------- | ------------------------------------------------------------------------------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| HIGH 1  | Substrate newtype ordering implicit (line 95 said `no ordering constraint`)                                                    | Added explicit ordering: `0206-001-substrate-newtype` MUST complete BEFORE this mission starts                                                                                    |
| HIGH 2  | `crates/quota-router-storage/Cargo.toml` re-export clause incoherent (Cargo.toml has no re-export)                             | Replaced with explicit lib.rs `pub use` directive per Move 1 step ('update module declaration in crates/octo-cap-macaroon/src/lib.rs')                                            |
| HIGH 3  | `crates/octo-ident-storage/Cargo.toml` edit ownership ambiguous (was: `this mission only adds deps if 005 created a skeleton`) | Clarified: Cargo.toml owned by `0206-005-octoident-storage-crate`; this mission does not edit Cargo.toml                                                                          |
| MED 1   | Cross-reference cite `` not re-verified for §Trait Move Schedule                              | No change to cite pending verification                                                                                                                                            |
| MED 2   | Intra-crate use-site cleanup dual-path (allowed `octo_ident_storage` OR `quota-router-storage` re-export)                      | Removed alternative path; mandated `octo_ident_storage::StoolapDidRegistry` per Move 2 re-export rule                                                                             |
| MED 3   | DDL allowlist registration AC verification abstract                                                                            | Added concrete verification step: `rg 'octo_storage::register::<' crates/octo-cap-macaroon/src/holder_registry.rs crates/octo-ident-storage/src/did_registry.rs` exits 0          |
| MED 4   | Function signature generic form                                                                                                | No change (not applicable to current file content)                                                                                                                                |
| MED 5   | Defensive boundary on `crates/quota-router-storage/src/holder_registry.rs:33` missing                                          | Added to Out of scope: `0206-002-layer-b-type-renames` MUST NOT modify `crates/quota-router-storage/src/holder_registry.rs:33` (line moves to octo-cap-macaroon per this mission) |
| LOW 1   | (R2 LOW finding)                                                                                                               | No file change required                                                                                                                                                           |

## v2.0 Changes from v1.0

| Finding | Severity                                                                             | Fix                                                                                                                                                                            |
| ------- | ------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| CRIT 1  | Target crate `crates/octo-ident-storage/` not created                                | Now filed as `0206-005-octoident-storage-crate`; added to `depends_on:` with ordering constraint                                                                               |
| CRIT 2  | AC fallback path `crates/octo-ident/src/did_registry_storage.rs` FABRICATED          | Removed OR-clause fallback; AC mandates EXACT path `crates/octo-ident-storage/src/did_registry.rs:139`                                                                         |
| HIGH 1  | AC didn't verify impl block moved                                                    | Added TV-0206-A8a (impl present at new path, exit 0) + TV-0206-A8b (impl removed from old path, exit 1)                                                                        |
| HIGH 2  | Missing `0206-002-layer-b-type-renames` dep                                          | Added to `depends_on:` with explicit ordering clause; 0206-002 MUST complete BEFORE 0206-003                                                                                   |
| HIGH 3  | Missing Cargo.toml edits enumeration                                                 | Added `crates/quota-router-storage/Cargo.toml` + `crates/octo-cap-macaroon/Cargo.toml` + `crates/octo-ident-storage/Cargo.toml` to Files/Artifacts with explicit dep additions |
| MED 1   | `cargo clippy --workspace --all-features` violates `quota-router-core-feature-mutex` | Replaced with `cargo clippy --workspace --all-targets --features full -- -D warnings`                                                                                          |
| MED 2   | Missing `cargo fmt --all -- --check` gate                                            | Added to AC                                                                                                                                                                    |
| MED 3   | VaultLookup ambiguity                                                                | Clarified in Scope: VaultLookup trait + impl move is OWNED by `0206-004-adapter-crates`; this mission only handles HolderRegistry + StoolapDidRegistry                         |
| MED 4   | DDL allowlist registration silent assumption                                         | Added AC: per RFC §Wiring Pattern, register via `octo_storage::register::<StoreType>(db, store_arc)`; verified in `0206-004` adapter fixtures                                  |
| LOW 1   | Midnight UTC timestamp fabricated                                                    | Changed `2026-08-20T00:00:00.000Z` → `2026-08-20T12:00:00.000Z`                                                                                                                |
| LOW 2   | Intra-crate use-site cleanup missing                                                 | Added to Files/Artifacts: edit `crates/octo-ident/src/did_registry.rs` + any other use sites to point at new declarer crates                                                   |

## Version History

| Version | Date       | Change                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| ------- | ---------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| v1.0    | 2026-08-20 | Initial filing (superseded)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| v2.0    | 2026-08-20 | R1 findings applied: 2 CRIT + 3 HIGH + 4 MED + 2 LOW (11 total); added `depends_on:` 0206-002 + 0206-005; pinned ordering; added impl-move gates; added Cargo.toml edits enumeration; added fmt gate; corrected clippy flag; clarified VaultLookup scope; added DDL allowlist AC; fixed timestamp; added use-site cleanup                                                                                                                                                                                                                     |
| v2.1    | 2026-08-20 | R2 findings applied: 0 CRIT + 3 HIGH + 5 MED + 1 LOW (9 total); added explicit substrate-newtype ordering (0206-001 MUST complete BEFORE 0206-003); fixed Cargo.toml re-export clause (lib.rs `pub use` not Cargo.toml); clarified octo-ident-storage Cargo.toml ownership (owned by 0206-005, this mission does not edit); removed use-site cleanup dual-path; added concrete DDL allowlist registration verification (`rg` gate); added defensive boundary (0206-002 MUST NOT modify crates/quota-router-storage/src/holder_registry.rs:33) |
