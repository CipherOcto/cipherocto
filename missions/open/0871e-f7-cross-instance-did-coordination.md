# 0871e-f7-cross-instance-did-coordination — Cross-instance DID write coordination

**Status:** unassigned (wave 4; approach picked 2026-08-10 → Option B; RFC draft `0862-writer-election-bootstrap-v120.md` filed)
**Substrate:** RFC-0862 v1.3 (Draft 2026-08-10) + RFC-0010 v1.3 `StoolapDidRegistry`
**Parent:** 0871b-storage-backend + RFC-0862 §Future Work F8 (writer election / auto-failover)

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

1. RFC-0862 v1.3 amendment lands first (in-flight; this mission
   references `rfcs/draft/networking/0862-writer-election-bootstrap-v120.md`).
2. New trait `DidWriteCoordinator` in
   `crates/octo-ident/src/write_coordinator.rs` per RFC-0862 v1.3
   §DidWriteCoordinator. Mirrors `DrainCoordinator` shape; same
   `WriterElection` substrate.
3. `StoolapDidRegistry` integrates with the coordinator — replaces
   per-instance mutex with coordinator-mediated atomicity. Per-instance
   mutex RETAINED as defense-in-depth.
4. CRDT-extension hook shipped (Option C substrate, fail-closed
   default): `submit_register_local_fallback` returns
   `DidWriteCoordinatorError::WriterUnavailable`. Future amendment
   (F12 + F13) replaces the default with LWW counter + reconciliation.
5. Cross-instance integration TV: spawn N `StoolapDidRegistry`
   instances + a coordinator; concurrent register / revoke total =
   expected (exactly), no torn writes.
6. Performance benchmark: write latency within RFC-0871 §Adversary
   A7 bounds (target p99 ≤ 30ms per Option B analysis).

## Test Vectors (preview)

- 4 new TV: cross-instance atomic register (N=10 instances,
  M=100 concurrent registers, exactly 1 succeeds per canonical_did);
  leader failover (coordinator switches mid-register, no register
  lost); WAL replay (replay from `DatabaseSyncAdapter` snapshot,
  DID state consistent); storage-error fail-closed (coordinator
  unreachable → register refused, no local fallback).

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
- `rfcs/draft/networking/0862-writer-election-bootstrap-v120.md` —
  RFC-0862 v1.3 amendment (substrate)

## Claimant

@unassigned

## Pull Request

#
