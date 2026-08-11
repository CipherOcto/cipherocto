# 0871e-f7-cross-instance-did-coordination — Cross-instance DID write coordination

**Status:** claimed (2026-08-11); substrate RFC-0862 v1.3 Accepted (commit `62ed3af1`) + DAG predecessor `0871b-storage-backend` LANDED (commit `71f8d745`). Approach picked 2026-08-10 → Option B per [[drain-coordinator-approach-2026-08-10]].
**Substrate:** RFC-0862 v1.3 (Accepted 2026-08-11, commit `62ed3af1`) §DidWriteCoordinator + RFC-0010 v1.3 `StoolapDidRegistry` (LANDED 2026-08-11, commit `71f8d745`)
**Parent:** RFC-0862 §Future Work F8 (writer election / auto-failover); DAG predecessor `0871b-storage-backend`

## Scope

Cross-instance DID write coordination: production HA / sharded
deployments of `StoolapDidRegistry` need atomic write coordination
across instances. v1.3 ships single-instance coordination
(per-instance `drain_lock`-equivalent mutex); F7 adds the
multi-instance contract analogous to `DrainCoordinator` for
`SpendLedger` (mission `0871e-phase5c-1-cross-instance-drain`).

### Approach pick (2026-08-10)

**Option B (centralized aggregator)** — user direction 2026-08-10.
Same pick as `0871e-phase5c-1-cross-instance-drain` (substrate
sharing per RFC-0862 v1.3 amendment `rfcs/draft/networking/0862-writer-election-bootstrap-v120.md`).

**Forward-thinking room for Option C (CRDT LWW)** — RFC-0862 v1.3
NEW §Future Work F12 + F13 (HLC + LWW counter + reconciliation
during failover window). The `DidWriteCoordinator` trait exposes
`submit_register_local_fallback` as the extension point; default
impl = fail-closed (current behavior); future amendment swaps the
impl to LWW without changing the trait surface.

### Mission scope (Option B implementation)

> **Status (2026-08-11):** Step 1 + 2 + 4 LANDED via this commit
> (substrate trait + ChainId + canonical_hash function + 9 unit tests
> covering default impls + sealed pattern + dyn-compat). Step 3
> re-scoped (see "Layer discipline correction" below). Steps 5-6
> deferred to follow-on missions (concrete coordinator impl + resolver-
> node mediation + cross-instance TV).

1. RFC-0862 v1.3 amendment lands first. **DONE** (Accepted 2026-08-11, commit `62ed3af1`); reference path updated to `rfcs/accepted/networking/0862-writer-election-bootstrap-v130.md` per AC#13.
2. New trait `DidWriteCoordinator` in
   `crates/octo-ident/src/write_coordinator.rs` per RFC-0862 v1.3
   §DidWriteCoordinator. Mirrors `DrainCoordinator` shape; same
   `WriterElection` substrate. **LANDED 2026-08-11** — trait + sealed
   pattern + default impls + `ChainId` newtype + `canonical_hash` free
   function. 9 unit tests cover default-impl semantics:
   canonical-hash validation, hash/document mismatch, coordinator-
   error propagation, LWW fail-closed default, dyn-compat via
   `Arc<dyn DidWriteCoordinator>`.
3. **`StoolapDidRegistry` integration — RE-SCOPED.** Originally
   planned to inject `Arc<dyn DidWriteCoordinator>` directly into
   `StoolapDidRegistry`. **Deferred to follow-on mission
   `0871e-f7-impl-resolver-mediation`** for layer-discipline reasons:
   `StoolapDidRegistry::register` is sync (`DidRegistry` trait);
   coordinator is async. Bridging sync→async via `block_on` would
   couple the storage layer to a tokio runtime. The clean
   Layer C split: `octo-identity-resolver-node` mediates coordinator
   calls around `StoolapDidRegistry::register` (consult coordinator
   → on success, delegate to local `StoolapDidRegistry::register`).
   Per-instance mutex defense-in-depth RETAINED as the current
   stoolap FOR UPDATE row lock (already in place).
4. CRDT-extension hook shipped (Option C substrate, fail-closed
   default): `submit_register_local_fallback` returns
   `DidWriteCoordinatorError::WriterUnavailable`. Future amendment
   (F12 + F13) replaces the default with LWW counter + reconciliation.
   **LANDED 2026-08-11** — `#[deprecated]` trait method with
   `WriterUnavailable` default impl per RFC-0862 v1.3 R12.
5. Cross-instance integration TV: spawn N `StoolapDidRegistry`
   instances + a coordinator; concurrent register / revoke total =
   expected (exactly), no torn writes. **DEFERRED** to follow-on
   mission — needs concrete `WriterElection`-backed coordinator impl
   (future RFC-0862 v1.4 amendment).
6. Performance benchmark: write latency within RFC-0871 §Adversary
   A7 bounds (target p99 ≤ 30ms per Option B analysis).
   **DEFERRED** to follow-on mission.

### Layer discipline correction

Per [[cipherocto-design-principles]] §Layer discipline:
- `octo-ident` (Layer B) — substrate trait + `ChainId` + canonical_hash
- `quota-router-storage` (Layer B-adjacent) — `StoolapDidRegistry`
  stays pure local persistence (no coordinator dependency)
- `octo-identity-resolver-node` (Layer C) — coordinator mediator
- `octo-sync` (Layer B-substrate, future) — concrete `WriterElection`-
  backed coordinator impl

This split keeps `StoolapDidRegistry` free of async-runtime coupling
and preserves the `DidRegistry` trait as a sync substrate that any
storage backend (Stoolap, RocksDB, in-memory) can implement without
needing a tokio runtime.

## Test Vectors (preview)

- 4 new TV: cross-instance atomic register (N=10 instances,
  M=100 concurrent registers, exactly 1 succeeds per canonical_did);
  leader failover (coordinator switches mid-register, no register
  lost); WAL replay (replay from `DatabaseSyncAdapter` snapshot,
  DID state consistent); storage-error fail-closed (coordinator
  unreachable → register refused, no local fallback).

### TV status (2026-08-11)

9 substrate TV landed in `octo-ident` (covers trait default impls +
canonical-hash validation + LWW fail-closed + dyn-compat):

- `canonical_hash_is_deterministic`
- `canonical_hash_distinguishes_public_keys`
- `canonical_hash_matches_mint_hash` (key invariant: canonical_hash
  of a document == `CanonicalCodec::mint(pubkey).hash`)
- `chain_id_round_trips_via_display`
- `error_display_messages_are_nonempty`
- `submit_register_validates_canonical_hash_and_delegates`
- `submit_register_rejects_hash_document_mismatch`
- `submit_register_propagates_coordinator_error`
- `submit_revoke_records_call`
- `submit_register_local_fallback_returns_writer_unavailable_by_default`
- `dyn_compatible_via_arc`

4 cross-instance TV DEFERRED to follow-on mission
`0871e-f7-impl-resolver-mediation` (needs concrete `WriterElection`-
backed coordinator impl + multi-instance test harness).

## Layer direction

- `octo-ident` (Layer B) — `DidWriteCoordinator` trait
- `quota-router-storage` (Layer B-adjacent) — `StoolapDidRegistry`
  consumes the coordinator
- `octo-sync` (Layer B-substrate) — `WriterElection` impl +
  `DatabaseSyncAdapter` for state replication between coordinator
  replicas
- `octo-paid-query` (Layer E) — `DrainCoordinator` (sister substrate)

## Validation

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --lib
```

## Cross-references

- [[rfc-0010-v13-storage-extension]] — v1.3 `DidRegistry` substrate
- [[mission-0871b-storage-backend]] — substrate mission
- [[mission-0871e-phase5c-1-cross-instance-drain]] — sister mission
  (same approach pick)
- [[cipherocto-design-principles]] — Layer A additive-only rule
- `rfcs/accepted/networking/0862-writer-election-bootstrap-v130.md` —
  RFC-0862 v1.3 (Accepted 2026-08-11, commit `62ed3af1`)

## Claimant

@mmacedoeu

## Pull Request

#
