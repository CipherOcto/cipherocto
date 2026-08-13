# Mission: marketplace-generic-store

## Status

Closed 2026-08-13 (@claude). LANDED.

## RFC

RFC-0968 retirement gate. Mission
`marketplace-facade-reputation-async-migration` v0.2 made
`Marketplace::reputation_compat` async-friendly but kept the
canonical store hard-coded as
`octo_reputation::store::InMemoryReputationStore`. Production
wiring needs the struct generic over the canonical store so a
`StoolapReputationStore` (or any other `ReputationStore` impl) can
be plugged in for restart durability.

## Dependencies

- Mission `marketplace-facade-reputation-async-migration` v0.2
  (compat field in place)
- Mission `marketplace-cheapest-with-ranking-async` v0.2 (async
  read path through the compat)
- `octo_reputation::store::ReputationStore: Send + Sync` trait
  already in place; `InMemoryReputationStore` and
  `StoolapReputationStore` both implement it

## Acceptance Criteria

- [x] `Marketplace<S = InMemoryReputationStore>` — struct
      parameterised over any `ReputationStore` impl, with
      `InMemoryReputationStore` as the default generic parameter so
      existing call sites keep working unchanged
- [x] New constructors:
      - `Marketplace::open_in_memory_with_store<S: ReputationStore>(store: S)`
      - `Marketplace::open_path_with_store<S: ReputationStore>(path: &str, store: S)`
      - `Marketplace::from_repo_with_store<S: ReputationStore>(repo: Arc<DynAskRepository>, store: S)`
- [x] Convenience wrappers retained on
      `Marketplace<InMemoryReputationStore>`:
      - `Marketplace::open_in_memory()`
      - `Marketplace::open_path(path)`
      - `Marketplace::from_repo(repo)`
- [x] Round-trip test pins the new constructor:
      `open_in_memory_with_store_wires_canonical_reputation_store`
      (async record + read through the custom store path)
- [x] Clippy passes with zero warnings
- [x] All existing tests pass + 1 new test (8 marketplace_reputation_async
      tests, 24 marketplace_e2e tests, 32 task_market tests, 23
      eleven_step tests — 87 total)

## Claimant

(@claude)

## Pull Request

(in progress)

## Notes

**Layer discipline:** the production wiring now lives at the
construction site. The struct is generic; production code calls
`Marketplace::open_path_with_store("/path/to/reputation.db",
StoolapReputationStore::open(...))`. Test/dev code keeps using the
default `InMemoryReputationStore` via `Marketplace::open_in_memory()`
— no type annotations needed.

**Layer direction check:** `StoolapReputationStore` is in the
`octo-reputation` crate (Layer B); `Marketplace` is in
`quota-router-core` (Layer C). The store is passed by value into
the constructor — no `dyn ReputationStore` indirection, no Arc
wrapping beyond what `Marketplace` already does internally. The
type-parameterised struct preserves Layer-B → Layer-C direction.

**Default generic parameter (`S = InMemoryReputationStore`)**
picks up correctly for both:
- `let m = Marketplace::open_in_memory().expect(...);` — type
  inference flows from the constructor's return type.
- `impl Marketplace { ... }` inside `mod.rs::tests` — the impl
  resolves to `Marketplace<InMemoryReputationStore>` via the
  default.

**Files touched:**
- `crates/quota-router-core/src/marketplace/mod.rs` — generic
  struct, `_with_store` constructors, convenience impl on
  `Marketplace<InMemoryReputationStore>`
- `crates/quota-router-core/tests/marketplace_reputation_async.rs`
  — 1 new constructor round-trip test

**Out of scope (NOT this mission):**
- Actual production wiring in `octo-quota-router-node` /
  `quota-router-cli` to thread a `StoolapReputationStore` through
  the runtime. That's a deployment concern that lands when the
  retirement gate flips (mission
  `marketplace-retirement-gate-flip`).
- Removing the `reputation: scoring::ProviderReputationRegistry`
  legacy shadow. That stays for the dual-read parity gate.

## Cross-references

- Mission `marketplace-facade-reputation-async-migration` v0.2
- Mission `marketplace-cheapest-with-ranking-async` v0.2
- RFC-0968 §retirement gate
- `crates/octo-reputation/src/store/mod.rs` (`ReputationStore`
  trait, `Send + Sync` bound)

## Version History

| Version | Date       | Status  | Change                                                                                                                                                                                                                                                                                       |
| ------- | ---------- | ------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| v0.1    | 2026-08-13 | closed  | `Marketplace<S = InMemoryReputationStore>` generic struct landed. `open_in_memory_with_store` / `open_path_with_store` / `from_repo_with_store` constructors added. Convenience impl on `Marketplace<InMemoryReputationStore>` retains `open_in_memory` / `open_path` / `from_repo`. 1 new constructor round-trip test. 87 tests pass. |
