# 0871e-phase5c-1 — Cross-instance SpendLedger drain coordination

**Status:** claimed 2026-08-11 (@claude) — `RaftLikeDrainCoordinator` substrate + 4 cross-instance TV + `StoolapSpendLedger` coordinator injection.
**Substrate:** RFC-0862 v1.3 (Draft 2026-08-10) + RFC-0855p-c (handover)
**Parent:** 0871e-phase5b-stoolap-ledger (landed `2b24796c`) per [[mission-0871e-phase5b-status]]

## Scope

`StoolapSpendLedger::try_deduct` (mission 0871e-phase5b-stoolap-ledger,
commit `2b24796c`) serializes drains within a single ledger instance
via `drain_lock`. Cross-instance coordination requires a new
substrate — `octo-sync`'s `DatabaseSyncAdapter` provides state
replication but not transaction coordination.

### Why this is a follow-on

1. **Single-instance deployment** — `drain_lock` is sufficient.
   Production deployments running ONE wallet-node process against ONE
   `StoolapSpendLedger` instance do not need cross-instance
   coordination. The follow-on only matters for HA / sharded
   deployments.

2. **`DatabaseSyncAdapter` is WAL streaming, not transaction
   coordination** — per RFC-0862 v1.1.0, the adapter ships raw WAL
   entries + snapshot segments. It does NOT provide 2PC,
   distributed locks, or cross-instance transaction ordering.
   Drain coordination requires a new substrate.

### Approach pick (2026-08-10)

**Option B (centralized aggregator)** — user direction 2026-08-10.
Substrate = RFC-0862 v1.3 amendment (Draft `rfcs/draft/networking/0862-writer-election-bootstrap-v120.md`)
which promotes RFC-0862 §Future Work F8 (writer election) + F11
(bootstrap-orchestrated peer discovery) to §Specification.

**Forward-thinking room for Option C (CRDT LWW)** — RFC-0862 v1.3
NEW §Future Work F12 (HLC + LWW per-instance counter) + F13
(reconciliation during failover window). The `DrainCoordinator`
trait exposes `submit_drain_local_fallback` as the extension point;
default impl = fail-closed (current behavior); future amendment
swaps the impl to LWW without changing the trait surface.

### Mission scope (Option B implementation)

1. RFC-0862 v1.3 amendment lands first (in-flight; this mission
   references `rfcs/draft/networking/0862-writer-election-bootstrap-v120.md`).
2. New trait `DrainCoordinator` in
   `crates/octo-paid-query/src/drain_coordinator.rs` per RFC-0862
   v1.3 §DrainCoordinator.
3. New `WriterElection` impl in `crates/octo-sync/src/writer_election.rs`
   using `DomainCoordinator` handover (RFC-0855p-c).
4. `StoolapSpendLedger` integrates with the coordinator — replaces
   per-instance `drain_lock` with coordinator-mediated atomicity.
   The per-instance mutex is RETAINED as a defense-in-depth layer
   (serializes within-instance even if coordinator handshake is
   delayed).
5. CRDT-extension hook shipped (Option C substrate, fail-closed
   default): `submit_drain_local_fallback` returns
   `DrainCoordinatorError::WriterUnavailable`. Future amendment
   (F12 + F13) replaces the default with LWW counter + reconciliation.
6. Cross-instance integration TV: spawn N threads across M
   `StoolapSpendLedger` instances + a coordinator, drain total =
   budget (exactly), no double-spend.
7. Performance benchmark: drain latency must remain within
   RFC-0871 §Adversary A7 bounds (target p99 ≤ 30ms per Option B
   analysis).

## Test vector discipline (preview)

- 4 new TV: cross-instance atomic drain (N=10 instances, M=100
  concurrent drains, exactly budget-many succeed); leader failover
  (coordinator switches mid-drain, no drain lost); WAL replay
  (replay from `DatabaseSyncAdapter` snapshot, drain state
  consistent); storage-error fail-closed (coordinator unreachable
  → drain refused, no local fallback).

## Depends on

- 0871e-phase5b-stoolap-ledger landed (`2b24796c`) — substrate
- `DatabaseSyncAdapter` (RFC-0862) — state replication substrate
- RFC amendment selecting one of the 3 candidate approaches —
  **NOT YET DRAFTED**; user direction pending

## Blocks

- Multi-instance production deployments
- Cross-shard drain (when per-holder sharding lands per RFC-0959-A1)
- 0959-a1 market delivery drain reconciliation (depends on
  cross-instance drain + holder registry gossip)

## Layer direction

- `octo-paid-query` (Layer E) — `DrainCoordinator` trait
- `quota-router-storage` (Layer B-adjacent) — `StoolapSpendLedger`
  consumes the coordinator
- `octo-sync` (Layer B-substrate) — `WriterElection` impl +
  `DatabaseSyncAdapter` for state replication between coordinator
  replicas

## Validation

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace --lib`

## Cross-references

- [[mission-0871e-phase5b-status]] — substrate predecessor
- [[wave-3-plan-correction-2026-08-10]] — drift context
- [[wave-3-gaps-2026-08-10]] — original wave 3 gap surface
- [[cipherocto-design-principles]] — Layer A additive-only rule
- `rfcs/draft/networking/0862-writer-election-bootstrap-v120.md` —
  RFC-0862 v1.3 amendment (substrate)
