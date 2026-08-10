# 0871e-f7-cross-instance-did-coordination — Cross-instance DID write coordination

**Status:** unassigned (wave 5; absorbed from RFC-0010 §Future Work F7 + RFC-0862 §Future Work F8 on 2026-08-10)
**Substrate:** RFC-0010 v1.3 `StoolapDidRegistry` + RFC-0862 `DatabaseSyncAdapter`
**Parent:** 0871b-storage-backend + RFC-0862 §Future Work F8 (writer election / auto-failover)

## Scope

Cross-instance DID write coordination: production HA / sharded
deployments of `StoolapDidRegistry` need atomic write coordination
across instances. v1.3 ships single-instance coordination
(per-instance `drain_lock`-equivalent mutex); F7 adds the
multi-instance contract analogous to `DrainCoordinator` for
`SpendLedger` (mission `0871e-phase5c-1-cross-instance-drain`).

### Candidate approaches (decision pending — see §Implementation Guide §Step 1)

1. **2PC coordinator** — server-elected write authority per
   `(canonical_did, chain_id)` shard. Heavy (consensus per write);
   simplest correctness model.
2. **Centralized aggregator** — writes funnel through a single
   elected writer node; replication via existing
   `DatabaseSyncAdapter` (RFC-0862). Light write (single round-trip);
   availability = writer availability.
3. **CRDT-style LWW counter** — each instance writes its own
   `DidDocument` revision; net state reconciled via eventual merge
   keyed by HLC + canonical DID. Highest availability + parallelism;
   hardest to reason about (must reject conflicting revocations).

The three approaches mirror the `DrainCoordinator` candidates
(mission `0871e-phase5c-1` §Candidate approaches). The pick MUST
match the `DrainCoordinator` pick — same substrate, same tradeoff
analysis. Recommendation: **Option B (centralized aggregator)** —
production HA deployments already elect a writer node for
`DatabaseSyncAdapter` (RFC-0862 §Roles); piggybacking DID writes
on the same election avoids a separate consensus layer.

### Mission scope (after approach pick + RFC-0862 amendment)

1. RFC-0862 amendment (or new RFC) for the chosen approach —
   likely RFC-0862 v1.2 with F8 (writer election / auto-failover)
   promoted from §Future Work to §Specification.
2. New trait `DidWriteCoordinator` in `crates/octo-ident/src/` —
   mirrors `DrainCoordinator` shape.
3. `StoolapDidRegistry` integrates with the coordinator — replaces
   per-instance mutex with coordinator-mediated atomicity.
4. Cross-instance integration TV: spawn N `StoolapDidRegistry`
   instances + a coordinator; concurrent register / revoke total =
   expected (exactly), no torn writes.
5. Performance benchmark: write latency within RFC-0871 §Adversary
   A7 bounds (no perceptible degradation for the resolver).

### RFC-0862 amendment scope (F8 promotion)

RFC-0862 §Future Work F8 (writer election / auto-failover) is the
substrate enabler. Promote F8 from §Future Work to §Specification;
add `WriterElection` protocol using `DomainCoordinator` handover
(RFC-0855p-c). The new protocol is the foundation for both
`DrainCoordinator` (spend) + `DidWriteCoordinator` (DID).

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
- `octo-sync` (Layer B-substrate) — `DatabaseSyncAdapter` for state
  replication between coordinator replicas
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

## Claimant

@unassigned

## Pull Request

#
