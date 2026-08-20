# RFC-0206 (Storage): octo-storage Substrate Split

## Status

**Version:** 1.0 (2026-08-19)
**Status:** Draft
**Layer:** B (introduces a new Layer A substrate crate `octo-storage-core`; the substrate itself is the Layer A boundary change; the rest of the RFC is Layer B facade + adapter wiring)

## Authors

- Author: @mmacedoeu

## Maintainers

- Maintainer: octo-storage owner
- Co-maintainer: per-owner adapter crate owners (octo-cap-macaroon-storage, octo-ident-storage, octo-policy-storage, octo-vault-storage; `octo-market-storage/` deferred per plan §4.2 B.4)

## Summary

Splits `crates/octo-storage` into a Layer A frozen substrate (`octo-storage-core`, RFC-frozen, years-stable) holding the Stoolap fork handle + typed migration runner, and a Layer B re-export facade (`octo-storage`, RFC-driven, additive) aggregating per-owner adapter crates. Closes the review §4.6.1 MED blocker; resolves the §4.4 / §4.6 / §4.6.1 owner-crate cycle risk by enforcing per-owner adapter placement.

## Dependencies

**Requires:**

- RFC-0914 (Economics): Stoolap-only quota-router persistence convention — establishes the Stoolap-only invariant this RFC extends with a per-owner adapter surface
- RFC-0205 (Storage): Stoolap fork stability certification — defines `octo-stoolap-frozen` Layer A substrate consumed by `octo-storage-core` (Draft; sibling RFC; must reach Accepted before RFC-0206 reaches Accepted per §Promotion Path)
- RFC-0105 (Numeric): Deterministic Quant Arithmetic — DQA wire form consumed by core
- RFC-0010 (Process): Canonical DID Codec + 32-byte chain_id addendum — `octo-ident-storage` adapter depends on typed `ChainId`

**Optional:**

- RFC-0960 (Storage): Vault substrate — `octo-vault-storage` adapter implements `VaultStore`
- RFC-0957 (Storage): Capability verify-time invariant — `octo-cap-macaroon-storage` adapter implements `HolderRegistry`

> **Dependency Validation Rules:** Required RFCs at minimum Draft status before RFC-0206 reaches Accepted. Currently: RFC-0205 is Draft (sibling); RFC-0105, RFC-0010, RFC-0914 are Accepted. This RFC introduces a new Layer A substrate crate (`octo-storage-core`); all consumer crates depend on it through the Layer B facade `octo-storage`.

## Design Goals

| Goal | Target                | Metric                                                                       |
| ---- | --------------------- | ---------------------------------------------------------------------------- |
| G1   | Zero cycle risk       | `cargo metadata` audit: no owner-trait crate in `octo-storage-core` dep tree |
| G2   | ≤ 1 migration surface | All migrations routed via `octo_storage_core::apply_pending` (free fn)       |
| G3   | Single import path    | Downstream uses `octo_storage::StoolapHolderRegistry` (facade)               |
| G4   | Per-owner isolation   | `octo-cap-macaroon-storage` does NOT depend on `octo-ident-storage`          |

## Motivation

`docs/reviews/2026-08-15-storage-layer-restructuring-analysis.md` §4.6.1 audit identified that `crates/octo-storage = B/A?` was undecided: the migrator is non-stable, but Layer A requires years-stable primitives. Owner crates (octo-ident, octo-cap-macaroon, octo-policy) each constructed `stoolap::Database` directly, duplicating migration-runner logic and creating a cycle risk: owner-trait crate → storage-core → owner-trait crate.

**Solution:** Adopt the §4.6.1 resolution — split into Layer A frozen core + Layer B re-export facade + per-owner adapter crates.

## Roles and Authorities

1. **octo-storage-core owner** — owns the Layer A substrate crate; gates schema changes via RFC.
2. **octo-storage facade owner** — owns the Layer B re-export crate; depends on `octo-storage-core` + each adapter crate.
3. **Adapter crate owners** — one per owner-trait (HolderRegistry, DidRegistry, PolicyStore, VaultStore, OrderBookStore, EscrowStore); file RFCs to add new adapters.
4. **RFC reviewer** — signs off on new adapter crates and migration additions to `octo-storage-core`.

| Role                      | Identifier                                | Authority Scope                                       | Lifecycle                 | Source/Ref                             |
| ------------------------- | ----------------------------------------- | ----------------------------------------------------- | ------------------------- | -------------------------------------- |
| octo-storage-core owner   | GitHub team `@octo-storage-core-owners`   | Layer A substrate; migration runner                   | Active until role revoked | RFC-0206 §Three-Tier Architecture      |
| octo-storage facade owner | GitHub team `@octo-storage-facade-owners` | Re-export glue; adapter registry                      | Active until role revoked | RFC-0206 §Three-Tier Architecture      |
| Adapter crate owner       | Per-adapter GitHub team                   | Owner-trait impl; register(Arc<Database>) constructor | Per-adapter               | RFC-0206 §Adapter Crate List (Initial) |
| RFC reviewer              | RFC process role                          | New adapter + migration approval                      | Per-RFC                   | RFC-0206 §Promotion Path               |

## Specification

### Three-Tier Architecture

```text
crates/octo-storage-core/  (Layer A, years-stable, RFC-frozen)
  - Stoolap fork handle wrapper (Database lifecycle)
  - Typed migration API (no upstream dep on owner crates)
  - Migration runner (infrastructure: lock, version, applied_at)
  - ZERO domain knowledge (no HolderRegistry, no DidRegistry, etc.)
  - Cargo.toml: depends only on Layer A
    (octo-determin, octo-stoolap-frozen per RFC-0205, blake3, borsh)

crates/octo-storage/  (Layer B, RFC-driven, additive) — RE-EXPORT FACADE ONLY
  - Re-exports the unified typed surface
  - NO domain impls inside this crate (cycle prevention)
  - Owner crates depend on octo-storage (facade) for re-exports
  - Adapter implementations live in per-owner crates
  - Cargo.toml: depends on octo-storage-core + per-owner adapter crates
```

### Adapter Crate List (Initial)

```text
octo-cap-macaroon-storage/   → impl HolderRegistry for StoolapHolderRegistry
octo-ident-storage/          → impl DidRegistry for StoolapDidRegistry (verify placement)
octo-policy-storage/         → impl PolicyStore for StoolapPolicyStore
octo-vault-storage/          → impl VaultStore for StoolapVaultStore (NEW per §20.3)
```

> **Note:** `octo-market-storage/` is **deferred** per plan §4.2 B.4 (octo-market primitive extraction is out of scope for this restructure cycle). When/if the marketplace primitive crate ships, a follow-on RFC adds the corresponding adapter. This RFC explicitly does NOT name it in §Key Files Modified or §TV.

### Cargo.toml Templates

**Layer A — `octo-storage-core/Cargo.toml`** (current on-disk state at `crates/octo-storage-core/Cargo.toml`; post-Phase-1 Task 1 target):

```toml
[package]
name = "octo-storage-core"

[dependencies]
# Layer A → substrate SQL engine. Until RFC-0205 freeze ships
# `octo-stoolap-frozen` as a workspace pin / `[patch.crates-io]` entry,
# octo-storage-core pins the active fork by branch (matches the
# workspace root `[patch.crates-io]` block in the audit). When
# RFC-0205 Phase 1 Task 1 lands, replace `branch = "feat/blockchain-sql"`
# with `rev = "<sha>"` and migrate the dep name to `octo-stoolap-frozen`.
stoolap = { git = "https://github.com/CipherOcto/stoolap", branch = "feat/blockchain-sql" }
# Layer A error type per `cipherocto-design-principles` Layer A row.
thiserror = "2.0"
# NOT a dep: octo-transport, quota-router-core, owner-trait crates
```

**Layer B facade — `octo-storage/Cargo.toml`** (current on-disk state; thin re-export of `octo-storage-core` only — adapter wiring is Phase 3 future work):

```toml
[package]
name = "octo-storage"

[dependencies]
# Layer B → Layer A substrate
octo-storage-core = { path = "../octo-storage-core" }
# Phase 3 will add per-owner adapter crate deps here as they land.
# Until each adapter crate ships, the facade remains a pure re-export.
# NOT a dep (today): octo-cap-macaroon-storage, octo-ident-storage,
# octo-policy-storage, octo-vault-storage
# NOT a dep (ever): octo-transport, quota-router-core
```

**Per-owner adapter — `octo-cap-macaroon-storage/Cargo.toml`:**

```toml
[package]
name = "octo-cap-macaroon-storage"

[dependencies]
# Layer B → Layer B (this crate → owner trait)
octo-cap-macaroon = { path = "../octo-cap-macaroon" }
# Layer B → Layer A
octo-storage-core = { path = "../octo-storage-core" }
# Layer B → Layer A (frozen substrate per RFC-0205)
octo-determin = { path = "../../determin" }
# Async runtime needed for register(Arc<Database>) + Stoolap async
tokio = { version = "1", features = ["sync"] }
# NOT a dep: octo-transport, quota-router-core
```

### Wiring Pattern

Each adapter crate exposes a `register(Arc<Database>) -> Arc<dyn OwnerTrait>` constructor. The application layer (Layer C node) collects adapters and injects them into domain crates via constructor injection. Per §4.4 per-owner placement: owner crates contain migrations (SQL files) in `crates/<owner>/migrations/`, but owner crates' `Cargo.toml` depends on `octo-storage-core` (NOT `stoolap` directly). Owner crates do NOT construct `stoolap::*` types directly — they go through the registered trait surface.

### Determinism Requirements

- Migration runner MUST produce identical `applied_at` timestamps for byte-identical migration input (no clock drift).
- `octo-stoolap-frozen` rev pin per RFC-0205 §Release-Tag Pin Policy.
- DQA wire form unchanged across re-cert.

### Operation Class Mapping

| Operation                          | Class | Rationale                       |
| ---------------------------------- | ----- | ------------------------------- |
| `octo_storage_core::apply_pending` | A     | Layer A substrate; years-stable |
| Adapter `register(Arc<Database>)`  | C     | Initialization glue; per-owner  |
| New adapter crate addition         | C     | RFC-driven additive             |
| Migration SQL file addition        | A     | Schema substrate; requires RFC  |

> **Note:** Operation Class A/B/C taxonomy per `docs/BLUEPRINT.md` §RFC Process. No separate RFC-NNNN anchors this taxonomy; it is defined inline in the process doc.

### Error Handling

| Error                                                         | Detection                 | Recovery                                      |
| ------------------------------------------------------------- | ------------------------- | --------------------------------------------- |
| `octo-storage-core` accidentally depends on owner-trait crate | CI `cargo metadata` audit | Reject merge; route to RFC reviewer           |
| Adapter cycle (A → B → A)                                     | CI graph audit            | Reject merge                                  |
| Migration drift across re-cert                                | CI byte-verify            | File RFC; reset pin per RFC-0205 policy       |
| Owner crate constructs `stoolap::*` directly                  | Clippy lint + CI grep     | Refactor to use `octo-storage-core::Database` |

## Performance Targets

| Metric                     | Target                | Notes                             |
| -------------------------- | --------------------- | --------------------------------- |
| Migration runner latency   | < 10 ms per migration | Lock + version + applied_at       |
| Adapter `register` latency | < 5 ms                | Arc wrapping + type-id resolution |
| `cargo metadata` audit     | < 2 s                 | Graph scan                        |

## Implicit Assumptions Audit

| Assumption                                               | Where Relied Upon         | Blast Radius if False                        | Mitigation / Status                                |
| -------------------------------------------------------- | ------------------------- | -------------------------------------------- | -------------------------------------------------- |
| Per-owner adapter crates form a DAG                      | §Adapter Crate List       | Cycle breaks facade re-export; compile fails | CI `cargo metadata --format-version 1` graph audit |
| Migrations always placed in `crates/<owner>/migrations/` | §Wiring Pattern           | Cross-crate migration; layering violation    | Lint + CI grep                                     |
| `octo-storage-core` depends only on Layer A              | §Cargo.toml Templates     | Layer A pollution; RFC reviewer can reject   | CI dep-graph audit                                 |
| DQA wire form stable across adapter additions            | §Determinism Requirements | Settlement replay diverges                   | Pinned at RFC-0105; bump = RFC-major               |
| Adapter crates can re-export without conflict            | §Layer B facade           | Type collision; facade fails to compile      | Type-naming convention: `Stoolap<OwnerTrait>`      |

### Categories to Audit

- **Operator trust** — adapter crate owners trusted to maintain API stability; compromise → facade breaks. Mitigation: per-adapter test suite + CI gate.
- **Platform trust** — Stoolap fork availability per RFC-0205.
- **Time source** — migration `applied_at` uses wall-clock; skew across nodes tolerated (advisory only, not consensus).
- **Network partition** — none; substrate is local.
- **Upgrade safety** — adding adapter = additive; removing adapter = breaking change requiring RFC-major.
- **Configuration** — `Cargo.toml` dep graph is source of truth; no env vars.
- **Identity stability** — adapter owner GitHub teams must be stable; quarterly audit.
- **Resource availability** — disk for migration metadata; standard.

## Security Considerations

- **Migration SQL injection** — adapter crates write SQL; poisoned input breaks schema. Mitigation: SQL files are static checked into repo; CI lints for parameterized queries.
- **Adapter supply chain** — attacker compromises adapter crate; facade exposes malicious type. Mitigation: per-adapter code review + signed releases.
- **Cycle exploit** — adapter cycle creates import-time DoS. Mitigation: CI graph audit + cyclic fail-fast.
- **Cross-adapter data leak** — adapter A reads adapter B's table without permission. Mitigation: per-owner table ownership enforced by `octo-storage-core` schema registry.

## Adversary Analysis

| Decision                    | Q1 Beneficiary            | Q2 Cost to Attacker           | Q3 Gain if Successful                 | Q4 Defense (cost to legit op)          | Q5 Residual Risk                    |
| --------------------------- | ------------------------- | ----------------------------- | ------------------------------------- | -------------------------------------- | ----------------------------------- |
| Per-owner adapter placement | Compromised adapter owner | Owner account compromise      | Cycle injection → facade compile fail | CI graph audit (low cost)              | LOW — automatic gate                |
| Layer A frozen core         | None directly             | High                          | Inject domain into Layer A            | CI rejects owner-trait deps (low cost) | LOW — multi-tenant separation       |
| Migration runner            | Compromised SQL file      | Write access to migration dir | Schema corruption                     | Code review + RFC gate (medium cost)   | MED — depends on reviewer vigilance |
| Adapter registry            | Compromised facade owner  | Facade owner account          | Re-export malicious types             | Per-adapter code review (low cost)     | LOW — facade surface narrow         |

### Severity Classification

| Severity     | Definition                  | Action                                |
| ------------ | --------------------------- | ------------------------------------- |
| **CRITICAL** | Cycle injection into facade | MUST mitigate before Accept (CI gate) |
| **HIGH**     | Migration SQL corruption    | SHOULD mitigate; RFC review           |
| **MEDIUM**   | Adapter type collision      | SHOULD mitigate (CI build)            |
| **LOW**      | Audit checklist skipped     | MAY accept; document residual         |

### Multi-Round Review

This RFC touches the Layer A substrate boundary. Multi-round review with severity classification is REQUIRED per `docs/BLUEPRINT.md` §Adversarial Review Process.

## Economic Analysis

No new tokens or stake implications. Cost: ~0.5 FTE/quarter for adapter maintainers + per-RFC reviewer time. Mitigated by per-adapter ownership + automated CI gates.

## Compatibility

- **Backward:** owner-trait crates that today construct `stoolap::*` directly (none confirmed in the workspace at RFC-0206 drafting time; the workspace audit `docs/audits/stoolap-fork-stability-2026-08-16.md` found all owner crates go through `octo-storage` already) continue to compile, but are flagged by Clippy lint per TV-0206-06 once adapter crates ship. Migration window from any future direct `stoolap::*` use to `octo-storage-core`: 90 days.
- **Forward:** adding new adapter = additive; facade auto-re-exports. No new RFC required for adding an adapter IF the adapter implements an existing owner-trait.

## Test Vectors

Governance TV — structural verification:

1. **TV-0206-01:** `crates/octo-storage-core/Cargo.toml` depends only on Layer A crates. Current on-disk state: `stoolap` (active fork branch) + `thiserror`. Post-RFC-0205 Phase 1 Task 1 target: `octo-stoolap-frozen` (rev-pinned) + `thiserror`.
2. **TV-0206-02:** `crates/octo-storage-core/` source contains zero references to `HolderRegistry`, `DidRegistry`, `PolicyStore`, `VaultStore`, `OrderBookStore`, `EscrowStore`.
3. **TV-0206-03:** `crates/octo-storage/` source contains only `pub use` re-exports; no `impl` blocks for owner traits.
4. **TV-0206-04:** Each adapter crate's `Cargo.toml` depends on exactly one owner-trait crate + `octo-storage-core`. **Forward requirement** — gates each adapter crate landing; none exist on disk yet (see §Implementation Phases).
5. **TV-0206-05:** CI graph audit rejects any cycle in adapter → owner-trait → adapter direction. Implemented as `cargo metadata --format-version 1` + `jq` reverse-DB scan over the adapter subtree (analogous to RFC-0205 TV-0205-04).
6. **TV-0206-06:** CI grep rejects any `stoolap::*` direct construction in owner-trait crates (must go through `octo-storage-core`). Implemented as `! rg 'use stoolap::' crates/octo-{ident,cap-macaroon}/src/ crates/cipherocto-policy/src/ crates/octo-vault/src/` in CI. (Note: `crates/octo-market/` does not exist on disk today; `crates/octo-policy/` was renamed to `crates/cipherocto-policy/` per workspace audit — the pattern above matches on-disk crate names. `octo-market-storage/` is deferred per plan §4.2 B.4; once shipped, extend the pattern.)
7. **TV-0206-07:** Per-adapter test suite covers `register(Arc<Database>) → Arc<dyn OwnerTrait>` round-trip. Forward requirement — one test file per adapter crate landing.

## Alternatives Considered

| Approach                                                                         | Pros                                                       | Cons                                                                       |
| -------------------------------------------------------------------------------- | ---------------------------------------------------------- | -------------------------------------------------------------------------- |
| Option A: Single `octo-storage` crate (no split)                                 | Simpler; one crate to maintain                             | Layer A vs B undecided; migrator non-stable; cycle risk                    |
| Option B: Per-owner direct `stoolap` deps                                        | Zero adapter overhead                                      | Duplicated migration logic; cycle risk; layering violation                 |
| Option C: Trait-only abstraction (no Stoolap impl)                               | Maximum portability                                        | Reimplements all Stoolap-specific features (MVCC DQA, lexicographic codec) |
| **Option D: Layer A core + Layer B facade + per-owner adapter crates (adopted)** | Resolves layering; per-owner isolation; single import path | More crates to maintain                                                    |

## Implementation Phases

### Phase 1: Layer A Core

- [x] Task 1: Create `crates/octo-storage-core/` with migration runner + Database handle (LANDED 2026-08-19 per `missions/claimed/octo-storage-split.md` R1-F5; commits `34e6025d` + `003f3a45`)
- [x] Task 2: Add `octo-storage-core` to workspace `Cargo.toml` (LANDED 2026-08-19)
- [ ] Task 3: Migrate Stoolap fork dep from owner crates to `octo-storage-core` (one crate at a time) — gated on RFC-0205 Phase 1 Task 1 freeze
- [ ] Task 4: Add CI graph audit + Clippy lint for owner-trait cycle

### Phase 2: Adapter Crates

- [ ] Task 5: Create `octo-cap-macaroon-storage/` + `octo-ident-storage/` + `octo-policy-storage/` adapters
- [ ] Task 6: Create `octo-vault-storage/` adapter (per §20.3); `octo-market-storage/` is DEFERRED to a follow-on RFC per plan §4.2 B.4
- [ ] Task 7: Verify per-adapter test suite passes `register` round-trip

### Phase 3: Layer B Facade

- [x] Task 8: Create `crates/octo-storage/` re-export facade (LANDED 2026-08-19; thin re-export of `octo-storage-core` per current `src/lib.rs`)
- [ ] Task 9: Update downstream consumers to import from `octo-storage::Stoolap*` (gated on Phase 2 adapter crate landings)
- [ ] Task 10: Remove direct `stoolap` deps from owner-trait crates (90-day migration window; gated on Phase 2)

## Promotion Path

This RFC reaches Accepted after all of the following are satisfied:

1. **Sibling RFC frozen:** RFC-0205 reaches Accepted (defines the Layer A substrate `octo-stoolap-frozen` that RFC-0206 Phase 1 Task 3 depends on).
2. **Adapter crates shipped or explicitly deferred:** at least `octo-cap-macaroon-storage/` + `octo-ident-storage/` + `octo-vault-storage/` land; `octo-policy-storage/` may be deferred if the naming discrepancy (see §Future Work) is unresolved. `octo-market-storage/` is OUT OF SCOPE per plan §4.2 B.4 and is tracked by a separate sub-RFC.
3. **CI gates land:** Phase 1 Task 4 (`cargo metadata` cycle audit + Clippy owner-trait lint) merged to `.github/workflows/ci.yml`.
4. **Multi-round review passes:** per `docs/BLUEPRINT.md` §Adversarial Review Process.

> **Cross-ref:** these four conditions originate from `docs/plans/2026-08-16-storage-layer-restructuring-execution-plan.md` §S7 termination conditions; the plan doc is a tracking artifact, not the source of truth — this section is authoritative for the RFC's promotion.

## Key Files to Modify

| File                                          | Status                                                                                                                                                  |
| --------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `Cargo.toml` (workspace)                      | Add `octo-storage-core`, `octo-storage`, adapter crates to members                                                                                      |
| `crates/octo-storage-core/Cargo.toml`         | LANDED — Layer A substrate deps (`stoolap` branch + `thiserror`)                                                                                        |
| `crates/octo-storage-core/src/lib.rs`         | LANDED — Database handle + migration runner (`apply_pending` etc.)                                                                                      |
| `crates/octo-storage/Cargo.toml`              | LANDED — facade re-exports (`octo-storage-core` only)                                                                                                   |
| `crates/octo-storage/src/lib.rs`              | LANDED — `pub use` aggregations                                                                                                                         |
| `crates/octo-cap-macaroon-storage/Cargo.toml` | NEW — adapter                                                                                                                                           |
| `crates/octo-ident-storage/Cargo.toml`        | NEW — adapter (workspace audit shows `crates/octo-ident` exists but storage adapter is NEW)                                                             |
| `crates/octo-policy-storage/Cargo.toml`       | NEW — adapter (workspace audit shows `crates/cipherocto-policy/` is the actual name; align on `octo-policy-storage` or document the rename in this RFC) |
| `crates/octo-vault-storage/Cargo.toml`        | NEW — adapter                                                                                                                                           |
| `.github/workflows/ci.yml`                    | Add graph audit + Clippy lint                                                                                                                           |

> **Note:** `crates/octo-market-storage/Cargo.toml` is **NOT** in scope for this RFC (deferred per plan §4.2 B.4). When/if the marketplace primitive crate ships, a follow-on RFC adds the corresponding adapter.

## Future Work

- Sub-mission `octo-storage-facade-versioning.md` (`to be filed`) for facade semver policy.
- Sub-mission `octo-storage-core-deprecation.md` (`to be filed`) for Layer B → Layer A migration.
- Adapter `Stoolap<OwnerTrait>` naming-convention enforcement (Clippy lint).
- Resolve naming discrepancy `crates/cipherocto-policy/` vs proposed `crates/octo-policy-storage/` (decide via follow-on RFC; the workspace currently has `cipherocto-policy/`, not `octo-policy/`).

## Version History

| Version | Date       | Author     | Changes                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| ------- | ---------- | ---------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| 1.0     | 2026-08-19 | @mmacedoeu | Initial draft. Three-tier architecture (Layer A core + Layer B facade + adapter crates) per review §4.6; Cargo.toml templates; per-owner migration placement; CI gates for cycle + Layer A pollution; 7 governance TV.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| 1.1     | 2026-08-19 | @mmacedoeu | Round 1 review fixes: phantom `RFC-0914-a` → `RFC-0914 (Economics)`; phantom `RFC-0001`/`RFC-0008` → `BLUEPRINT.md` ref + inline Operation Class Mapping; consolidated §refs to `§4.4` / `§4.6` / `§4.6.1` (review doc carries §4.6.1 at the layer-assignment MED blocker); Cargo.toml templates aligned with current on-disk state (Phase 1 Tasks 1/2/8 LANDED, Phase 1 Tasks 3/4 + Phase 2/3 forward); `MigrationsHandle::apply_pending` → `octo_storage_core::apply_pending` (free fn); adapter crate naming gap `cipherocto-policy/` vs `octo-policy-storage/` flagged in §Future Work; Implementation Phases checkboxes corrected (Phase 1 Tasks 1/2 + Phase 3 Task 8 marked `[x]`); Layer self-declaration added to Status; Roles Source/Ref column updated to precise §names. Doc accuracy only — no spec change.                                                                                                                                                                                                                                                                                                 |
| 1.2     | 2026-08-19 | @mmacedoeu | Round 2 review fixes: reverted §4.6 → §4.6.1 in Summary + Motivation (R1 reviewer claim that §4.6.1 was phantom was incorrect — review doc line 1732 carries `# §4.6.1 octo-storage layer assignment (MED blocker)`); removed `octo-market-storage` from Maintainers co-maintainer list, §Adapter Crate List (Initial), §Cargo.toml Templates Layer B facade "NOT a dep" list, §Implementation Phases Phase 2 Task 6, and §Key Files Modified (workspace has no `crates/octo-market/`; plan §4.2 B.4 explicitly defers octo-market primitive extraction out of scope); fixed TV-0206-06 grep pattern (`market` and `policy` → `cipherocto-policy/` to match on-disk crate names); tightened "as a published crate" phrasing in §Cargo.toml Templates to "as a workspace pin / `[patch.crates-io]` entry" (the frozen fork is a workspace dep until upstream crates-io publish); dropped inline `RFC-0206 v1.0` version pin in §Compatibility Backward (per CLAUDE.md rule, Version History is the only place version pins belong); backticked `to be filed` markers in §Future Work. Doc accuracy only — no spec change. |
