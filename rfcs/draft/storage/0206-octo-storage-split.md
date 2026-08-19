# RFC-0206 (Storage): octo-storage Substrate Split

## Status

**Version:** 1.0 (2026-08-19)
**Status:** Draft

## Authors

- Author: @mmacedoeu

## Maintainers

- Maintainer: octo-storage owner
- Co-maintainer: per-owner adapter crate owners (octo-cap-macaroon-storage, octo-ident-storage, octo-policy-storage, octo-vault-storage, octo-market-storage)

## Summary

Splits `crates/octo-storage` into a Layer A frozen substrate (`octo-storage-core`, RFC-frozen, years-stable) holding the Stoolap fork handle + typed migration runner, and a Layer B re-export facade (`octo-storage`, RFC-driven, additive) aggregating per-owner adapter crates. Closes the review §4.6.1 MED blocker; resolves the §4.4 / §4.6 / §4.6.1 owner-crate cycle risk by enforcing per-owner adapter placement.

## Dependencies

**Requires:**

- RFC-0914-a (Storage): Stoolap-only persistence convention
- RFC-0205 (Storage): Stoolap fork stability certification — defines `octo-stoolap-frozen` Layer A substrate consumed by `octo-storage-core`
- RFC-0105 (Numeric): Deterministic Quant Arithmetic — DQA wire form consumed by core
- RFC-0010 (Process): Canonical DID Codec + 32-byte chain_id addendum — `octo-ident-storage` adapter depends on typed `ChainId`

**Optional:**

- RFC-0960 (Storage): Vault substrate — `octo-vault-storage` adapter implements `VaultStore`
- RFC-0957 (Storage): Capability verify-time invariant — `octo-cap-macaroon-storage` adapter implements `HolderRegistry`

> **Dependency Validation Rules:** All upstream RFCs Accepted (RFC-0914-a, RFC-0205 Draft, RFC-0105, RFC-0010). This RFC introduces a new Layer A substrate crate (`octo-storage-core`); all consumer crates depend on it through the Layer B facade `octo-storage`.

## Design Goals

| Goal | Target                | Metric                                                                       |
| ---- | --------------------- | ---------------------------------------------------------------------------- |
| G1   | Zero cycle risk       | `cargo metadata` audit: no owner-trait crate in `octo-storage-core` dep tree |
| G2   | ≤ 1 migration surface | All migrations routed via `octo_storage_core::MigrationsHandle`              |
| G3   | Single import path    | Downstream uses `octo_storage::StoolapHolderRegistry` (facade)               |
| G4   | Per-owner isolation   | `octo-cap-macaroon-storage` does NOT depend on `octo-ident-storage`          |

## Motivation

`docs/reviews/2026-08-15-storage-layer-restructuring-analysis.md` §4.6.1 audit identified that `crates/octo-storage = B/A?` was undecided: the migrator is non-stable, but Layer A requires years-stable primitives. Owner crates (octo-ident, octo-cap-macaroon, octo-market, octo-policy) each constructed `stoolap::Database` directly, duplicating migration-runner logic and creating a cycle risk: owner-trait crate → storage-core → owner-trait crate.

**Solution:** Adopt the §4.6.1 resolution — split into Layer A frozen core + Layer B re-export facade + per-owner adapter crates.

## Roles and Authorities

1. **octo-storage-core owner** — owns the Layer A substrate crate; gates schema changes via RFC.
2. **octo-storage facade owner** — owns the Layer B re-export crate; depends on `octo-storage-core` + each adapter crate.
3. **Adapter crate owners** — one per owner-trait (HolderRegistry, DidRegistry, PolicyStore, VaultStore, OrderBookStore, EscrowStore); file RFCs to add new adapters.
4. **RFC reviewer** — signs off on new adapter crates and migration additions to `octo-storage-core`.

| Role                      | Identifier                                | Authority Scope                                       | Lifecycle                 | Source/Ref                  |
| ------------------------- | ----------------------------------------- | ----------------------------------------------------- | ------------------------- | --------------------------- |
| octo-storage-core owner   | GitHub team `@octo-storage-core-owners`   | Layer A substrate; migration runner                   | Active until role revoked | RFC-0206 §Specification     |
| octo-storage facade owner | GitHub team `@octo-storage-facade-owners` | Re-export glue; adapter registry                      | Active until role revoked | RFC-0206 §Specification     |
| Adapter crate owner       | Per-adapter GitHub team                   | Owner-trait impl; register(Arc<Database>) constructor | Per-adapter               | RFC-0206 §Adapter Crates    |
| RFC reviewer              | RFC process role                          | New adapter + migration approval                      | Per-RFC                   | RFC-0001 §Mission Lifecycle |

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
octo-market-storage/         → impl OrderBookStore, EscrowStore for Stoolap* (NEW per §4.2)
```

### Cargo.toml Templates

**Layer A — `octo-storage-core/Cargo.toml`:**

```toml
[package]
name = "octo-storage-core"

[dependencies]
# Layer A frozen substrate per RFC-0205
octo-stoolap-frozen = { git = "https://github.com/CipherOcto/stoolap", rev = "<sha>" }
# Layer A primitive substrate
octo-determin = { path = "../../determin" }
# Hash + encoding
blake3 = { version = "1", features = ["serde"] }
borsh = { version = "1", features = ["derive"] }
# NOT a dep: octo-transport, quota-router-core, owner-trait crates
```

**Layer B facade — `octo-storage/Cargo.toml`:**

```toml
[package]
name = "octo-storage"

[dependencies]
# Layer B → Layer A
octo-storage-core = { path = "../octo-storage-core" }
# Layer B → Layer B (re-export only, no domain impls here)
octo-cap-macaroon-storage = { path = "../octo-cap-macaroon-storage" }
octo-ident-storage = { path = "../octo-ident-storage" }
octo-policy-storage = { path = "../octo-policy-storage" }
octo-vault-storage = { path = "../octo-vault-storage" }
octo-market-storage = { path = "../octo-market-storage" }
# NOT a dep: octo-transport, quota-router-core
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

### RFC-0008 Execution Class Mapping

| Operation                         | Class | Rationale                       |
| --------------------------------- | ----- | ------------------------------- |
| `MigrationsHandle::apply_pending` | A     | Layer A substrate; years-stable |
| Adapter `register(Arc<Database>)` | C     | Initialization glue; per-owner  |
| New adapter crate addition        | C     | RFC-driven additive             |
| Migration SQL file addition       | A     | Schema substrate; requires RFC  |

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

- **Backward:** pre-RFC-0206 owner crates directly using `stoolap::*` continue to compile but are flagged by Clippy lint. Migration window: 90 days.
- **Forward:** adding new adapter = additive; facade auto-re-exports. No new RFC required for adding an adapter IF the adapter implements an existing owner-trait.

## Test Vectors

Governance TV — structural verification:

1. **TV-0206-01:** `crates/octo-storage-core/Cargo.toml` depends only on Layer A crates (`octo-determin`, `octo-stoolap-frozen`, `blake3`, `borsh`).
2. **TV-0206-02:** `crates/octo-storage-core/` source contains zero references to `HolderRegistry`, `DidRegistry`, `PolicyStore`, `VaultStore`, `OrderBookStore`, `EscrowStore`.
3. **TV-0206-03:** `crates/octo-storage/` source contains only `pub use` re-exports; no `impl` blocks for owner traits.
4. **TV-0206-04:** Each adapter crate's `Cargo.toml` depends on exactly one owner-trait crate + `octo-storage-core`.
5. **TV-0206-05:** CI graph audit rejects any cycle in adapter → owner-trait → adapter direction.
6. **TV-0206-06:** CI grep rejects any `stoolap::*` direct construction in owner-trait crates (must go through `octo-storage-core`).
7. **TV-0206-07:** Per-adapter test suite covers `register(Arc<Database>) → Arc<dyn OwnerTrait>` round-trip.

## Alternatives Considered

| Approach                                                                         | Pros                                                       | Cons                                                                       |
| -------------------------------------------------------------------------------- | ---------------------------------------------------------- | -------------------------------------------------------------------------- |
| Option A: Single `octo-storage` crate (no split)                                 | Simpler; one crate to maintain                             | Layer A vs B undecided; migrator non-stable; cycle risk                    |
| Option B: Per-owner direct `stoolap` deps                                        | Zero adapter overhead                                      | Duplicated migration logic; cycle risk; layering violation                 |
| Option C: Trait-only abstraction (no Stoolap impl)                               | Maximum portability                                        | Reimplements all Stoolap-specific features (MVCC DQA, lexicographic codec) |
| **Option D: Layer A core + Layer B facade + per-owner adapter crates (adopted)** | Resolves layering; per-owner isolation; single import path | More crates to maintain                                                    |

## Implementation Phases

### Phase 1: Layer A Core

- [ ] Task 1: Create `crates/octo-storage-core/` with migration runner + Database handle
- [ ] Task 2: Add `octo-storage-core` to workspace `Cargo.toml`
- [ ] Task 3: Migrate Stoolap fork dep from owner crates to `octo-storage-core` (one crate at a time)
- [ ] Task 4: Add CI graph audit + Clippy lint for owner-trait cycle

### Phase 2: Adapter Crates

- [ ] Task 5: Create `octo-cap-macaroon-storage/` + `octo-ident-storage/` + `octo-policy-storage/` adapters
- [ ] Task 6: Create `octo-vault-storage/` + `octo-market-storage/` adapters (per §20.3 + §4.2)
- [ ] Task 7: Verify per-adapter test suite passes `register` round-trip

### Phase 3: Layer B Facade

- [ ] Task 8: Create `crates/octo-storage/` re-export facade
- [ ] Task 9: Update downstream consumers to import from `octo-storage::Stoolap*`
- [ ] Task 10: Remove direct `stoolap` deps from owner-trait crates (90-day migration window)

## Key Files to Modify

| File                                          | Change                                                             |
| --------------------------------------------- | ------------------------------------------------------------------ |
| `Cargo.toml` (workspace)                      | Add `octo-storage-core`, `octo-storage`, adapter crates to members |
| `crates/octo-storage-core/Cargo.toml`         | NEW — Layer A substrate deps                                       |
| `crates/octo-storage-core/src/lib.rs`         | NEW — Database handle + migration runner                           |
| `crates/octo-storage/Cargo.toml`              | NEW — facade re-exports                                            |
| `crates/octo-storage/src/lib.rs`              | NEW — `pub use` aggregations                                       |
| `crates/octo-cap-macaroon-storage/Cargo.toml` | NEW — adapter                                                      |
| `crates/octo-ident-storage/Cargo.toml`        | VERIFY — already exists, verify placement                          |
| `crates/octo-policy-storage/Cargo.toml`       | NEW — adapter                                                      |
| `crates/octo-vault-storage/Cargo.toml`        | NEW — adapter                                                      |
| `crates/octo-market-storage/Cargo.toml`       | NEW — adapter                                                      |
| `.github/workflows/ci.yml`                    | Add graph audit + Clippy lint                                      |

## Future Work

- Sub-mission `octo-storage-facade-versioning.md` for facade semver policy.
- Sub-mission `octo-storage-core-deprecation.md` for Layer B → Layer A migration.
- Adapter `Stoolap<OwnerTrait>` naming-convention enforcement (Clippy lint).

## Version History

| Version | Date       | Author     | Changes                                                                                                                                                                                                                  |
| ------- | ---------- | ---------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| 1.0     | 2026-08-19 | @mmacedoeu | Initial draft. Three-tier architecture (Layer A core + Layer B facade + adapter crates) per review §4.6.1; Cargo.toml templates; per-owner migration placement; CI gates for cycle + Layer A pollution; 7 governance TV. |
