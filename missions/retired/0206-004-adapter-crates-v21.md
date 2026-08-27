---
name: 0206-004-adapter-crates-v21
description: Open 2026-08-20 v2.1; RFC-0206 v2.0 §Adapter Crate List — 5 adapter crates (octo-vault-storage, octo-reputation-storage, octo-cap-macaroon-vault-storage, octo-matrix-session-store-storage, octo-policy-storage) + 5 trait declarations (4 NEW + 1 MOVE: VaultLookup) + per-adapter fixtures. `octo-ident-storage` is the 6th adapter crate but owned by `0206-005-octoident-storage-crate`. Facade (`crates/octo-storage/`) migration owned by `0206-001-substrate-newtype` (out of scope).
metadata:
  node_type: mission
  type: project
  originSessionId: 9a316ae1-cb15-46f4-801f-834acacd23ae
  created: 2026-08-20T00:00:00.000Z
  v: "2.1"
  supersedes: v2.0
  depends_on:
    - 0206-001-substrate-newtype
    - 0206-003-trait-moves
    - 0206-005-octoident-storage-crate
    - 0206-006-cipherocto-policy-rename-alignment
    - RFC-0205
    - RFC-0206
status: retired
phase: retired
layer: B
rfc_authority: RFC-0206 v2.0
tvs:
  - TV-0206-A6
  - TV-0206-A8
  - TV-0206-A9
  - TV-0206-A10
  - TV-0206-A11
  - TV-0206-A12
  - TV-0206-A14
---

# Mission `0206-004-adapter-crates` v2.1 — OPEN 2026-08-20

## v2.0 Changes from v1.0

R1 findings applied (19 total: 4 CRIT + 4 HIGH + 8 MED + 3 LOW):

- **CRIT 1**: `VaultLookup` is declared at `octo-cap-macaroon/src/vault_lookup.rs:62` (NOT `:33`). Line ref updated.
- **CRIT 2**: `crates/octo-policy/src/lib.rs` does NOT exist on disk — on-disk is `crates/cipherocto-policy/`. `0206-006-cipherocto-policy-rename-alignment` added to `depends_on:` and referenced in Cross-references + Files/Artifacts.
- **CRIT 3**: `depends_on:` lacked `0206-006-cipherocto-policy-rename-alignment`. Added.
- **CRIT 4**: Facade assumed 4-item re-export set but current `crates/octo-storage/src/lib.rs` re-exports 8 items (`apply_pending`, `open`, `open_in_memory`, `ApplyConfig`, `Migration`, `StaticMigration`, `StorageError`, `DEFAULT_TRACKER_TABLE`). Added precondition: migration from current `octo-storage-core` surface (migration runner + `StorageError`) to RFC v2.0 surface (`Database` + `TypedStatement` + `AdapterAllowlist` + `register`) is the breaking-change boundary owned by `0206-001`. Acknowledged in §Files/Artifacts.
- **HIGH 1**: Naming resolution claim was wrong — workspace `Cargo.toml` uses `members = ["crates/*"]` glob (NOT `[workspace] members` per-crate list). Replaced claim with package-name-only reference + 0206-006 dep acknowledged.
- **HIGH 2**: Mission cited "RFC §Summary Updates vs v1.8" as authority for naming. Wrong cite — actual source is RFC §Adapter Crate List line 181 + 183. Replaced.
- **HIGH 3**: Mission cross-references only listed §Adapter Crate List. RFC §Adapter Crate List does NOT include facade (facade is in §Cargo.toml Templates Layer B / Tier 2 in §Three-Tier Architecture). Both added to cross-references.
- **HIGH 4**: Mission added redundant `octo-storage-core` dep on adapter crates (facade re-exports substrate). Per RFC §Cargo.toml Cross-Cuts "NOT direct substrate" rule, adapter crates use ONLY facade. Dropped `octo-storage-core` from adapter Cargo.toml template; escape hatch documented as exception.
- **MED 1**: AC items said "5 directory existence check green" without reproducing exact `test -d` command per RFC TV-0206-A6. Replaced with RFC §Test Vectors exact gate commands for A6, A10, A11, A12.
- **MED 2**: Mission §A cited only TV-0206-A14. RFC TV-0206-A3 covers wildcard detector on BOTH substrate + facade. Substrate side owned by 0206-001 but cross-TV acknowledged. Added cross-reference.
- **MED 3**: Mission did not acknowledge TV-0206-A9 (no stoolap dep in adapter Cargo.toml) — partially in scope per §B. Added explicit cross-reference to TV-0206-A9 + TV-0206-A8 (HolderRegistry declaration owned by 0206-003).
- **MED 4**: Test fixtures missing adversarial cases. Added: (a) two adapters registering same adapter_id; (b) adapter with empty allowlist; (c) `format!()` injection in DdlRegistered template; (d) `From<Database> for stoolap::Database` escape-hatch misuse. Per RFC §Format Bypass Defense.
- **MED 5**: Mission §A cited `register<V: OwnerTrait>(db, store)` helper but no test fixture for `register()` dispatch. RFC §Wiring Pattern uses per-trait register fns. Added fixture testing one per-trait register fn returns trait-object Arc.
- **MED 6**: Mission §A ambiguous about register form ("(or 5 per-trait register fns if generics awkward)"). Picked per-trait register fns form per RFC §Wiring Pattern and gated it.
- **MED 7**: Cross-RFC atomicity missing. Added RFC-0205 v2.0 + BLUEPRINT.md §Dependency Validation Rules → 2-Cycle Atomic Promotion rule 5 cite.
- **MED 8**: §C label "4 Trait Declarations" but lists 5. Renamed to "5 Trait Declarations (4 NEW + 1 MOVE)".
- **LOW 1**: Cross-references list 0206-001 + 0206-003 but NOT 0206-002. Added 0206-002 to cross-references.
- **LOW 2**: Mission description said "4 trait declarations" but body lists 5 (4 NEW + 1 move). Updated description to "5 trait declarations (4 NEW + 1 MOVE: VaultLookup)".
- **LOW 3**: §C label inconsistency (overlaps with MED 8).

## v2.1 Changes from v2.0

R2 findings applied (12 total: 3 CRIT + 4 HIGH + 3 MED + 2 LOW):

- **CRIT 1**: Restored `octo-storage-core = { path = "../octo-storage-core" }` to adapter Cargo.toml template per RFC §Cargo.toml Cross-Cuts lines 333-338 (REVERTS v2.0 HIGH 4; RFC explicitly REQUIRES substrate dep in adapter Cargo.toml).
- **CRIT 2**: §A "Per-trait register fns" claim DROPPED. Replaced with RFC §Wiring Pattern generic helper form `pub fn register<V: VaultStore>(db: Arc<octo_storage_core::Database>, store: Arc<V>) -> Arc<VaultStore>` (VaultStore shown as exemplar; one generic helper per trait per RFC §Wiring Pattern — NOT five per-trait register fns).
- **CRIT 3**: Resolved facade migration ownership contradiction. Line 50 Precondition (owned by `0206-001-substrate-newtype`) is authoritative. Section A title rewritten to "A. Register Wiring (Facade Cargo.toml + lib.rs owned by `0206-001`)"; Files/Artifacts facade edits removed entirely per HIGH 4.
- **HIGH 1**: AC clippy command updated `cargo clippy --workspace --all-targets --all-features -- -D warnings` → `--features full` per `quota-router-core-feature-mutex` memory (`--all-features` ALWAYS fails on this codebase).
- **HIGH 2**: DAG cycle with `0206-002-layer-b-type-renames` resolved by dropping `0206-002` from this mission's `depends_on:` (mission AC narrowed to specific crates; 0206-002 still referenced in Out of scope + Cross-references for traceability).
- **HIGH 3**: Added owner-crate dep per adapter Cargo.toml (e.g., `octo-vault-storage` needs `octo-vault = { path = "../octo-vault" }` to reference the `VaultStore` trait declared in owner crate per §C line 84-88).
- **HIGH 4**: Removed `crates/octo-storage/Cargo.toml` + `crates/octo-storage/src/lib.rs` facade edits from Files/Artifacts entirely per CRIT 3. Compile-time test at `crates/octo-storage/src/lib.rs:44-77` (references all 8 items) is now owned by `0206-001-substrate-newtype`.
- **MED 1**: DAG cycle `0206-002 ↔ 0206-003` cross-cutting — added "Known DAG cycle" note in Dependencies section; AC narrowed to omit cross-cutting crates.
- **MED 2**: Mission scope (5 adapter crates + 6th in 0206-005 = 6 total) exceeds RFC TV-0206-A9(b) ≤ 5 (4 adapter + substrate). RFC TV-0206-A9(b) undercounts the 6th adapter crate; flagged for v2.1 amendment.
- **MED 3**: Added `0206-003-trait-moves` to `depends_on:` with ordering constraint (must land before adapter trait declarations that depend on HolderRegistry + StoolapDidRegistry moves).

## Scope

Land RFC-0206 v2.0 §Adapter Crate List: 5 per-owner adapter crates + 5 trait declarations (4 NEW + 1 MOVE) + per-adapter test fixtures. Facade migration owned by `0206-001-substrate-newtype` (out of scope per CRIT 3). Closes TV-0206-A6, TV-0206-A8 (adapter side), TV-0206-A9(a/b), TV-0206-A10, TV-0206-A11, TV-0206-A12, TV-0206-A14 gates.

**Precondition (CRIT 4)**: Migration from current `octo-storage-core` surface (migration runner + `StorageError` + `ApplyConfig` + `Migration` + `StaticMigration` + `DEFAULT_TRACKER_TABLE` + `apply_pending` + `open` + `open_in_memory` = 8 items in `crates/octo-storage/src/lib.rs` today) to RFC v2.0 surface (`Database` + `TypedStatement` + `AdapterAllowlist` + `register` = 4 items) is the breaking-change boundary owned by `0206-001-substrate-newtype`. This mission assumes that migration has landed; it does not regress or extend the substrate's pre-v2.0 surface.

**Precondition (CRIT 2)**: `crates/octo-policy/` directory must exist on disk before `octo-policy-storage/` adapter crate can be created. The disk name is `cipherocto-policy/` today; `0206-006-cipherocto-policy-rename-alignment` is the load-bearing rename mission and runs as a hard dependency.

**Out of mission scope**: `octo-ident-storage/` is the 6th adapter crate (RFC §Adapter Crate List row 3 — StoolapDidRegistry impl target) but is owned by `0206-005-octoident-storage-crate`, NOT this mission. This mission creates 5 adapters + 5 trait declarations. Facade `crates/octo-storage/` migration is owned by `0206-001-substrate-newtype` per CRIT 3.

### A. Register Wiring (Facade Cargo.toml + lib.rs owned by `0206-001`)

- **Generic register helper** (per RFC §Wiring Pattern): `pub fn register<V: VaultStore>(db: Arc<octo_storage_core::Database>, store: Arc<V>) -> Arc<VaultStore>` — single generic helper form (VaultStore shown as exemplar; one generic helper per trait per RFC §Wiring Pattern)
- Wildcard detector gate (TV-0206-A14): `rg '\b\*\s*[,;}]' crates/octo-storage/src/lib.rs` MUST equal 0 — gated post-`0206-001` (facade owned by `0206-001`)
- Cross-TV: TV-0206-A3 (wildcard detector on substrate + facade) — substrate side owned by `0206-001`, facade side gated post-`0206-001`

### B. 5 Adapter Crates

For each of `octo-vault-storage`, `octo-reputation-storage`, `octo-cap-macaroon-vault-storage`, `octo-matrix-session-store-storage`, `octo-policy-storage`:

- `Cargo.toml`: declares `octo-storage-core = { path = "../octo-storage-core" }` (REQUIRED per RFC §Cargo.toml Cross-Cuts lines 333-338) + owner-crate dep (e.g., `octo-vault = { path = "../octo-vault" }` for `octo-vault-storage`, needed to reference trait declared in owner crate); NO direct `stoolap` dep (TV-0206-A9(a))
- `Cargo.toml` escape hatch: if a specific adapter needs to import a substrate-only type (e.g. `TypedStatement` variant) BEYOND what `octo-storage-core` exposes, document the exception in an inline `# WHY` comment referencing the RFC §Cargo.toml Cross-Cuts rule
- `src/lib.rs`: declares trait (NEW) + impl + calls generic register helper from facade
- `tests/register_roundtrip.rs`: generic register helper + select + insert round-trip TV
- `tests/drop_table_rejected.rs`: `DdlRegistered(DropTable(...))` → `SubstrateError::DdlNotInAllowlist`
- `tests/namespace_guard.rs`: workspace query outside adapter namespace → `SubstrateError::TableNotInNamespace`
- **Adversarial fixtures** (per RFC §Format Bypass Defense):
  - `tests/adversarial_double_register.rs`: two adapters registering same `adapter_id` → second registration rejected
  - `tests/adversarial_empty_allowlist.rs`: adapter with empty `AdapterAllowlist` → all DDL rejected
  - `tests/adversarial_format_injection.rs`: `format!()` injection attempt in `DdlRegistered` template (e.g. table name containing `; DROP TABLE`) → `SubstrateError::DdlNotInAllowlist`
  - `tests/adversarial_escape_hatch_misuse.rs`: misuse of `From<Database> for stoolap::Database` escape hatch from a non-typed-query allowlist site → compile-time or runtime refusal

### C. 5 Trait Declarations (4 NEW + 1 MOVE)

Per RFC-0206 v2.0 §Adapter Crate List:

- `VaultStore` (declarer: octo-vault, impl: octo-vault-storage) — NEW
- `ReputationStore` (declarer: octo-reputation, impl: octo-reputation-storage) — NEW
- `VaultLookup` (declarer: octo-cap-macaroon, impl: octo-cap-macaroon-vault-storage — trait move from `octo-cap-macaroon/src/vault_lookup.rs:62`) — MOVE
- `SessionStore` (declarer: octo-matrix-session-store, impl: octo-matrix-session-store-storage) — NEW
- `PolicyStore` (declarer: octo-policy, impl: octo-policy-storage) — NEW

(NOTE: 5 adapters, but `VaultLookup` already declared today in `octo-cap-macaroon/src/vault_lookup.rs:62`; count = 4 NEW + 1 MOVE.)

### D. Workspace Registration

- Workspace root `Cargo.toml` uses `members = ["crates/*"]` glob; creating the adapter directory is sufficient for workspace pickup — NO explicit `[workspace] members` edit needed
- Package name resolution: crate name is `octo-policy` per RFC-0206 v2.0 §Adapter Crate List line 181 + 183 (canonical naming) — directory rename owed by `0206-006-cipherocto-policy-rename-alignment`

## Acceptance Criterion

- 5 adapter crate directories on disk (verified via RFC §Test Vectors exact gate commands):
  - TV-0206-A6: `test -d crates/octo-vault-storage && test -d crates/octo-reputation-storage && test -d crates/octo-cap-macaroon-vault-storage && test -d crates/octo-matrix-session-store-storage && test -d crates/octo-policy-storage` exits 0
  - TV-0206-A10: `ls crates/octo-{vault,reputation,cap-macaroon-vault,matrix-session-store,policy}-storage/tests/ | grep -c register_roundtrip` equals 5
  - TV-0206-A11: `ls crates/octo-{vault,reputation,cap-macaroon-vault,matrix-session-store,policy}-storage/tests/ | grep -c drop_table_rejected` equals 5
  - TV-0206-A12: `ls crates/octo-{vault,reputation,cap-macaroon-vault,matrix-session-store,policy}-storage/tests/ | grep -c namespace_guard` equals 5
- `crates/octo-storage/` facade exists with 4-item re-export set per RFC §Cargo.toml Templates Layer B (gated post-`0206-001` — facade owned by `0206-001-substrate-newtype` per CRIT 3)
- TV-0206-A14 gate: `rg '\b\*\s*[,;}]' crates/octo-storage/src/lib.rs` output count equals 0 (gated post-`0206-001` per CRIT 3)
- TV-0206-A3 gate (facade side): `rg '\b\*\s*[,;}]' crates/octo-storage/src/lib.rs` output count equals 0 (substrate + facade both owned by `0206-001` per CRIT 3)
- TV-0206-A9(a) gate (per adapter): `rg '^\s*stoolap\s*=' crates/octo-{vault,reputation,cap-macaroon-vault,matrix-session-store,policy}-storage/Cargo.toml` exits 1
- TV-0206-A8 gate (HolderRegistry declaration owned by `0206-003`): cross-referenced and confirmed NOT in this mission's scope
- Adversarial fixtures present: `ls crates/octo-{vault,reputation,cap-macaroon-vault,matrix-session-store,policy}-storage/tests/ | grep -c adversarial` equals 20 (4 adversarial files × 5 adapters)
- `crates/octo-policy/` directory exists on disk (verified by `0206-006-cipherocto-policy-rename-alignment`); `crates/cipherocto-policy/` directory absent
- `cargo build --workspace --all-targets` green
- `cargo test --workspace --lib` green
- `cargo clippy --workspace --all-targets --features full -- -D warnings` green
- `cargo fmt --all -- --check` green

## Files / Artifacts

- New: `crates/octo-vault-storage/Cargo.toml` + `src/lib.rs` + `tests/{register_roundtrip,drop_table_rejected,namespace_guard,adversarial_double_register,adversarial_empty_allowlist,adversarial_format_injection,adversarial_escape_hatch_misuse}.rs`
- New: `crates/octo-reputation-storage/Cargo.toml` + `src/lib.rs` + 7 tests (3 standard + 4 adversarial)
- New: `crates/octo-cap-macaroon-vault-storage/Cargo.toml` + `src/lib.rs` + 7 tests
- New: `crates/octo-matrix-session-store-storage/Cargo.toml` + `src/lib.rs` + 7 tests
- New: `crates/octo-policy-storage/Cargo.toml` + `src/lib.rs` + 7 tests (depends on `0206-006-cipherocto-policy-rename-alignment` for `crates/octo-policy/` directory)
- Edit: workspace root `Cargo.toml` (no-op — `members = ["crates/*"]` glob auto-picks up new directories)
- Edit: `crates/octo-vault/src/lib.rs` (declare `VaultStore` trait)
- Edit: `crates/octo-reputation/src/lib.rs` (declare `ReputationStore` trait)
- Edit: `crates/octo-cap-macaroon/src/vault_lookup.rs:62` (move trait to new adapter crate — relocate `pub trait VaultLookup` declaration)
- Edit: `crates/octo-matrix-session-store/src/lib.rs` (declare `SessionStore` trait)
- Edit: `crates/octo-policy/src/lib.rs` (declare `PolicyStore` trait; directory exists per `0206-006-cipherocto-policy-rename-alignment`)

## Cross-references

- RFC-0206 v2.0 §Three-Tier Architecture (Tier 2 = facade, owned by `0206-001` per CRIT 3)
- RFC-0206 v2.0 §Cargo.toml Templates Layer B facade (owned by `0206-001` per CRIT 3)
- RFC-0206 v2.0 §Adapter Crate List
- RFC-0206 v2.0 §Cargo.toml Cross-Cuts lines 333-338 (REQUIRED `octo-storage-core` dep in adapter Cargo.toml per CRIT 1; HIGH 3 adds owner-crate dep)
- RFC-0206 v2.0 §Wiring Pattern (generic register helper form per CRIT 2)
- RFC-0206 v2.0 §Format Bypass Defense (adversarial fixtures)
- RFC-0206 v2.0 §Test Vectors TV-0206-A6, A8 (cross-ref), A9(a/b — A9(b) undercounts per MED 2), A10, A11, A12, A14 (3 cross-ref; facade side gated post-`0206-001`)
- RFC-0206 v2.0 §Adapter Crate List line 181 + 183 (canonical naming for `octo-policy`)
- RFC-0205 v2.0 (coupled pair per BLUEPRINT.md §Dependency Validation Rules → 2-Cycle Atomic Promotion rule 5)
- Mission `0206-001-substrate-newtype` (substrate `Database` type + TypedStatement enum + AdapterAllowlist + facade `crates/octo-storage/` migration owner per CRIT 3)
- Mission `0206-003-trait-moves` (HolderRegistry + StoolapDidRegistry moves — must land before adapter trait declarations per MED 3)
- Mission `0206-005-octoident-storage-crate` (6th adapter crate — out of scope for this mission)
- Mission `0206-006-cipherocto-policy-rename-alignment` (directory rename `cipherocto-policy` → `octo-policy`)

## Out of scope

- Substrate newtype impl + facade `crates/octo-storage/` migration (owned by `0206-001-substrate-newtype` per CRIT 3)
- 29 Layer B TYPE renames in `quota-router-storage` + `octo-vault` (owned by `0206-002-layer-b-type-renames`; dropped from this mission's `depends_on:` per HIGH 2 to resolve DAG cycle)
- HolderRegistry + StoolapDidRegistry moves (owned by `0206-003-trait-moves`)
- `octo-ident-storage/` adapter crate (owned by `0206-005-octoident-storage-crate`)
- `cipherocto-policy → octo-policy` directory rename (owned by `0206-006-cipherocto-policy-rename-alignment`)

## Dependencies

- `0206-001-substrate-newtype` (substrate must exist for `Database` type + TypedStatement enum + AdapterAllowlist; also owns facade `crates/octo-storage/` migration per CRIT 3)
- `0206-003-trait-moves` (HolderRegistry + StoolapDidRegistry moves must land before adapter trait declarations; ordering constraint per MED 3)
- `0206-005-octoident-storage-crate` (sibling mission — 6th adapter crate must land in same atomic batch)
- `0206-006-cipherocto-policy-rename-alignment` (directory rename must land before `octo-policy-storage/` adapter crate can be created)
- RFC-0205 v2.0 (coupled pair per BLUEPRINT.md §Dependency Validation Rules → 2-Cycle Atomic Promotion rule 5)
- RFC-0206 v2.0 (acceptance precondition per BLUEPRINT.md rule 5)

**Known DAG cycle (MED 1)**: `0206-002 ↔ 0206-003` is a cross-cutting cycle. This mission's AC is narrowed to omit cross-cutting crates (octo-vault-storage touches `octo-vault` which 0206-002 renames; track in RFC-0206 v2.1 amendment).

## Version History

| Version | Date       | Change                                                             |
| ------- | ---------- | ------------------------------------------------------------------ |
| v2.1    | 2026-08-20 | R2 findings applied; 12 findings (3 CRIT + 4 HIGH + 3 MED + 2 LOW) |
| v2.0    | 2026-08-20 | R1 findings applied; 19 findings (4 CRIT + 4 HIGH + 8 MED + 3 LOW) |
| v1.0    | 2026-08-20 | Initial filing                                                     |
