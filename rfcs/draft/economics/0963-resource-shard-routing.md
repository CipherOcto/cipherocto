# RFC-0963 (Economics): Resource Shard Routing

## Status

Draft

> **Note:** Companion RFC to RFC-0960 §10 (Horizontal Scalability — Resource Sharding). Defines the shard routing algorithm, cross-shard transaction protocol, per-shard Merkle commitment, and horizontal scaling invariants. Builds on RFC-0862 (sync as propagation), RFC-0959 (SettlementReceipt), RFC-0960 §7 (MultiSettlement / atomic swaps), and Phase 4 finding (event-sourced ledger precedents) which corrected the draft routing key from event-type to vault-id.

## Version History

| Version | Date | Author | Note |
|---------|------|--------|------|
| v1.0 | 2026-07-23 | @cipherocto + @mmacedoeu | Initial draft. |

## Authors

- Author: @cipherocto (grand-design resource shard work)
- Contributor: @mmacedoeu (RFC-0963 protocol extraction)

## Maintainers

- Maintainer: @mmacedoeu
- Maintainer: @cipherocto

## Summary

CipherOcto shards by **resource**, not by transaction count. Each shard owns the event log + projection for a partition of vaults. Cross-shard mutations use `MultiSession` (RFC-0962 §7) for atomicity. Each shard publishes its own Merkle root; the global chain head is the Merkle mountain range (MMR) over per-shard roots.

Three artifacts:

1. **Shard ID derivation** — `shard_id(vault_id) = u32::from_be_bytes(BLAKE3(vault_id)[0..4]) % num_shards`. Stable across all nodes; deterministic; no central registry.
2. **Per-shard event log** — `events_shard_{N}` table per shard N. Append-only. Sync via RFC-0862.
3. **Cross-shard protocol** — `MultiSession` with `AllRequired` completion rule (default) or `Quorum(n)` for sharded reads.

### Routing key selection — `vault_id`, not event type

Phase 4 finding: routing by event type (e.g., all `TransferApplied` events to one shard) breaks balance projections. A vault's balance is `+ins - outs` over events touching it; if those events are scattered across N shards, every balance read requires N-shard fan-out. Routing by `vault_id` keeps all events for a vault co-located, making balance reads single-shard.

Event-type routing would also cause divergent balance projections during partial sync: a node that has synced shard A but not shard B would compute a different balance than a node that has synced both. Vault-id routing makes single-vault balance computable from one shard's log only.

## Dependencies

### Required RFCs

| RFC | Status | Reason |
|-----|--------|--------|
| RFC-0960 | Draft (companion) | Defines §10 resource sharding architecture |
| RFC-0962 | Draft (companion) | MultiSession cross-shard coordination |
| RFC-0862 | Accepted (v1.2.0) | Sync as propagation; per-shard event batch shipping |
| RFC-0959 | Accepted (v1.0) | SettlementReceipt envelope for cross-shard value transfer |
| RFC-0126 | Accepted (v2.5.1) | Canonical serialization for shard commitment |
| RFC-0853 | Draft | BLAKE3 primitive source for shard ID derivation + Merkle commitments |

### Companion RFCs (Planned)

| RFC | Relationship | Reason |
|-----|--------------|--------|
| RFC-0964 | Builds on | Constraint encoding for cross-shard constraint verification |
| RFC-0965 | Builds on | Capability extension format for shard-scoped capabilities |

### Dependency Validation

| Dependency | Type | Current Status (2026-07-23) | Hard-block? |
|------------|------|------------------------------|-------------|
| RFC-0960 | Requires | Draft (companion) | YES |
| RFC-0962 | Requires | Draft (companion) | YES |
| RFC-0862 | Requires | Accepted | No |
| RFC-0959 | Requires | Accepted | No |
| RFC-0126 | Requires | Accepted | No |
| RFC-0853 | Requires | Draft | YES |

**DAG check:** `0963 ← {0960, 0962, 0862, 0959, 0126, 0853}` — acyclic.

## Design Goals

| Goal | Target | Metric |
|------|--------|--------|
| G1 | Balance reads are single-shard | Every vault's event log lives on exactly one shard; reads need no fan-out |
| G2 | Shard ID derivation deterministic | Two nodes computing `shard_id(vault_id)` produce identical u32 |
| G3 | Shard count dynamic | Adding/removing shards triggers re-shard with bounded data migration |
| G4 | Cross-shard atomicity | MultiSession with AllRequired completes or aborts within `timeout_unix_ms` |
| G5 | Sync-friendly | Per-shard event log is independent; nodes can sync shards out of order |
| G6 | ZK-friendly | Per-shard Merkle root commits to entire shard state without revealing events |

## Motivation

### Why shard at all?

Single-node ledgers don't scale beyond one machine's write throughput. Sharding is the standard answer, but the design space is wide. Most blockchains shard by **transaction count** (e.g., Ethereum 2.0: 64 shards, each processing 1/64 of transactions). CipherOcto shards by **resource** because:

1. **Resource consumption is naturally bounded.** GPU, bandwidth, storage, inference — each has independent throughput ceilings. Sharding by resource type maps to physical capacity.
2. **Value transfers touch few resources.** A payment from one vault to another only needs to touch the two involved vaults (and their shards). Cross-shard coordination is rare.
3. **Capability constraints are resource-scoped.** A `RateLimit(max_per_window, "GPU seconds")` constraint only needs to evaluate against the GPU shard's event log.
4. **ZK proofs compose per shard.** A balance proof for vault V only needs the Merkle root of V's shard, not the global state.

### Why vault_id, not event_type?

Phase 4 research (`docs/research/2026-07-22-event-sourced-ledger-precedents.md`) surfaced this directly:

> Shard routing must be by `vault_id` (not by event type). Routing by event type would cause divergent balance projections; must shard by `vault_id`.

The mechanism: a vault's balance projection is `+ins - outs` over events touching that vault. If those events are scattered across N shards by type, every balance read requires N-shard fan-out. Worse, during partial sync a node that has synced shard A but not shard B computes a different balance than a node that has synced both. The result: balance non-determinism, broken consensus.

Vault-id routing co-locates all events for one vault on one shard, making:
- Balance reads single-shard (one log read, one projection).
- Balance proofs ZK-friendly (one Merkle proof, one shard root).
- Sync deterministic (one shard's log is sufficient for one vault's projection).

## Specification

### 1. Shard ID derivation

```text
shard_id(vault_id: VaultID, num_shards: u32) -> ShardID:
    let hash = BLAKE3(vault_id)
    let prefix = u32::from_be_bytes(hash[0..4])
    return prefix % num_shards
```

Properties:
- **Deterministic** — same inputs produce same output on every node.
- **Uniform distribution** — BLAKE3 output is uniformly random; modulo gives roughly equal vault count per shard.
- **No central registry** — every node computes shard_id independently.
- **Forward-compatible** — increasing `num_shards` triggers re-sharding (§4).

`num_shards` is a network parameter. Default = `ceil(sqrt(network_size))`. For 100 nodes, default = 10 shards. For 10,000 nodes, default = 100 shards.

### 2. Per-shard event log

Each shard N owns one table:

```sql
CREATE TABLE events_shard_{N} (
    event_id         BIGINT       NOT NULL,
    event_type       TEXT         NOT NULL,
    tx_id            BYTES        NOT NULL,
    schema_version   INT          NOT NULL,
    visibility       TEXT         NOT NULL,
    timestamp_unix   BIGINT       NOT NULL,
    attributes       BYTES        NOT NULL,
    corrections      BYTES        NULL,
    signature        BYTES        NOT NULL,
    zk_proof         BYTES        NULL,
    vault_id         BYTES        NOT NULL,        -- denormalised for shard filtering
    PRIMARY KEY (event_id)
);

CREATE INDEX ix_events_shard_{N}_vault ON events_shard_{N} (vault_id, event_id);
CREATE INDEX ix_events_shard_{N}_type ON events_shard_{N} (event_type, event_id);
```

Every row carries `vault_id` (denormalised from `attributes`). Insert path:

1. Compute `shard_id(vault_id)`.
2. Route the write to `events_shard_{N}` where N = shard_id.
3. Append-only; no UPDATE.

Why denormalize `vault_id`? So `SELECT ... WHERE vault_id = ?` is single-shard + index-scan, not a full-table scan over `attributes`. Trade-off: storage cost (32 bytes per row) for query speed.

### 3. Per-shard Merkle commitment

Each shard publishes a Merkle root every K events (default K = 1024):

```text
shard_root(N, k) -> Hash:
    let events = events_shard_{N} WHERE event_id >= (k-1)*K AND event_id < k*K
    return BLAKE3(MerkleTree(events).root())
```

The global chain head is a Merkle mountain range (MMR) over per-shard roots:

```text
global_head -> MMRNode {
    children: [shard_root(0, k), shard_root(1, k), ..., shard_root(N-1, k)],
    hash: BLAKE3(concat(children))
}
```

K-deep shards mean a ZK proof of "vault V has balance B at event_id E" requires:
- Merkle proof of the shard root at the relevant k-bucket.
- Merkle proof of the event within the shard's K-event batch.
- MMR proof binding the shard root to the global head.

This is the same shape as RFC-0959's `settlement_hash` commitment, applied per-shard.

### 4. Re-sharding

Increasing `num_shards` triggers re-sharding. Two strategies:

#### 4.1 Drain + refill (default)

1. Mark shard N as `Draining`. No new writes to N.
2. Route all new writes for vaults previously on N to new shards per new `num_shards`.
3. Sync the drain phase: every node must process every event on N before N is retired.
4. After drain completes (event_id reaches "shard retired" mark), N is read-only archive.

Total downtime per vault: zero (vaults are routed to new shards immediately; their pending writes land on new shards).

#### 4.2 Live migration (advanced)

For large vaults with billions of events, drain+refill is slow. Live migration moves events in batches while writes continue:

1. Begin migration: vaults in source shard start dual-writing to source + destination shards.
2. Sync historical events from source to destination in batches.
3. After destination has all historical events, switch reads to destination.
4. Switch writes to destination only; stop dual-writing to source.
5. Source shard keeps historical archive for the migration window (default 30 days), then retires.

Live migration is bounded by network bandwidth + per-shard throughput; default budget: 10% of shard throughput dedicated to migration.

### 5. Cross-shard mutations

A mutation that touches vaults on shards A and B (e.g., transfer from vault V_a on shard A to vault V_b on shard B) uses `MultiSession` (RFC-0962 §7):

```text
MultiSession {
    multi_session_id: MultiSessionID,
    sub_sessions: [
        ConsensusSession { shard_id: A, ... },   // debit V_a
        ConsensusSession { shard_id: B, ... },   // credit V_b
    ],
    completion: AllRequired,
    timeout_unix_ms: 5000,                       // 5s default
    fallback_action: Abort,
}
```

Each sub-session is signed by its shard's block producer. Cross-shard coordination:

1. Initiator constructs `MultiSession` envelope.
2. Initiator broadcasts to all shard block producers.
3. Each block producer validates the relevant sub-session against its shard state.
4. Each block producer signs and broadcasts the sub-session to the global MMR.
5. If all sub-sessions reach `Replayed` within `timeout_unix_ms`, MultiSession commits.
6. If timeout expires, `fallback_action` runs (default `Abort` — no partial commit).

### 6. Shard-scoped capabilities

A capability may carry a `Sharded` constraint (RFC-0965 enumeration) that pins it to a specific shard:

```text
Capability {
    caveats: [
        Vault(vault_id),          // vault lives on shard S
        Sharded(S),               // capability only valid on shard S
        Permission(NativeTokenTransfer),
        ValidRange(...),
        MaxPerTx(...),
    ]
}
```

Verification checks `shard_id(capability.vault_id) == S`. If mismatch, capability is invalid on this shard.

This enables:

- **Regional compliance.** A EU-only capability is pinned to the EU shard.
- **Provider locality.** A provider vault is pinned to the provider's nearest shard.
- **Regulatory data residency.** Privacy-sensitive vaults are pinned to in-jurisdiction shards.

### 7. Catalog schema

```sql
CREATE TABLE shard_registry (
    shard_id              INT PRIMARY KEY,
    state                 TEXT NOT NULL,        -- Active | Draining | Retired
    num_shards_at_creation INT NOT NULL,         -- network size when this shard was born
    current_num_shards    INT NOT NULL,          -- current network size; updated on every re-shard
    created_at_unix       BIGINT NOT NULL,
    retired_at_unix       BIGINT NULL,
    event_count           BIGINT NOT NULL DEFAULT 0,
    last_root_k           BIGINT NOT NULL DEFAULT 0,
    last_root_hash        BYTES NOT NULL DEFAULT zeroblob(32)
);

CREATE TABLE shard_migration_log (
    migration_id          BLOB PRIMARY KEY,     -- BLAKE3(canonical_ser(migration))
    source_shard_id       INT NOT NULL,
    destination_shard_id  INT NOT NULL,
    num_shards_before     INT NOT NULL,
    num_shards_after      INT NOT NULL,
    started_at_unix       BIGINT NOT NULL,
    completed_at_unix     BIGINT NULL,
    state                 TEXT NOT NULL,        -- Pending | DualWriting | Reading | Finalized | Aborted
    events_migrated       BIGINT NOT NULL DEFAULT 0
);
```

### 8. Sync interaction (RFC-0862)

Per-shard sync via RFC-0862:

```text
sync_batch := {
    shard_id: N,
    from_event_id: E_start,
    to_event_id: E_end,
    events: Vec<EventEnvelope>,
    shard_root_at_E_end: Hash,
}
```

A node syncing shard N needs:
1. The current `shard_id` (computed deterministically from any vault_id).
2. The event log from genesis to head for shard N (or from last seen event_id).
3. The shard root at the batch boundary (for MMR consistency).

Sync is per-shard independent: a node can sync shards 0, 5, 7 in parallel and shard 3 later. Out-of-order sync is safe because shard_id derivation is deterministic.

### 9. Worked example: cross-shard transfer

**Setup:** Alice's vault on shard 0, Bob's vault on shard 3, 5 shards total.

**Step 1: Alice signs the transfer.**

```text
Capability {
    caveats: [Vault(alice_vault), Permission(NativeTokenTransfer), MaxPerTx(100 OCTO_W)],
    holder_signature: alice_sig,
}

MultiSession {
    sub_sessions: [
        ConsensusSession { shard_id: 0, sql: [UPDATE vaults SET balance = balance - 100 WHERE vault_id = alice_vault] },
        ConsensusSession { shard_id: 3, sql: [UPDATE vaults SET balance = balance + 100 WHERE vault_id = bob_vault] },
    ],
    completion: AllRequired,
    timeout_unix_ms: 5000,
}
```

**Step 2: Broadcast.** Alice's router sends the MultiSession to block producers for shards 0 and 3.

**Step 3: Validate + sign.**

- Shard 0 producer: validates `alice_vault` lives on shard 0 (yes). Validates Alice's capability covers the transfer (yes). Signs sub-session.
- Shard 3 producer: validates `bob_vault` lives on shard 3 (yes). Validates the credit is paired with shard 0's debit (yes, via tx_id linkage). Signs sub-session.

**Step 4: Commit.** Both sub-sessions included in their respective shards' blocks. Shard 0 root updated. Shard 3 root updated. Global MMR updated.

**Step 5: Replay.** Every node receives both sub-sessions via RFC-0862 sync. Replays both in deterministic order. State converges.

**Timeout case:** If shard 3 producer is offline > 5s, `fallback_action = Abort`. Shard 0's debit is reverted (a `TransferCorrected` event is appended).

## Open Questions

| # | Question | Resolution Target |
|---|----------|-------------------|
| 1 | Optimal `num_shards` for a network of N nodes? | Empirical; default = `ceil(sqrt(N))` |
| 2 | Should historical shards (Retired) be queryable? | Yes — read-only archive; GC after 1 year |
| 3 | Can a single capability span multiple shards? | No — use MultiCapability + MultiSession instead |
| 4 | What's the cross-shard read cost? | Two shard reads + MMR proof; ~10ms p99 for 100 shards |
| 5 | How does ZK proof of "vault V has balance B" compose across shards? | Single shard proof + MMR inclusion proof |
| 6 | Can shard routing change mid-session? | No — `shard_id(vault_id, num_shards)` is fixed at session creation |

## Out of Scope

- **State sharding vs. transaction sharding.** RFC-0963 shards state (events). Some systems shard transactions instead; we don't.
- **Cross-shard contract calls.** Capability constraints + vet (RFC-0965) handle cross-vault composition; no separate cross-shard contract primitive.
- **Dynamic shard count without migration.** All shard-count changes trigger migration; no in-place rebalance.
- **Inter-shard consensus optimization.** Standard MMR + BLAKE3 commitments; no custom aggregation.

## Status

This RFC = Resource shard routing. Status: Draft. Companion RFCs 0960 (architecture), 0962 (MultiSession), 0964 (Constraint encoding), 0965 (capability ext) in flight. Awaiting review and promotion to Accepted.

Once Accepted, the `cipherocto-shard-router` crate implements:
- `shard_id(vault_id, num_shards) -> ShardID`
- Per-shard event log writer
- MultiSession coordinator
- MMR root builder
- Live migration coordinator
- ZK proof aggregator (per-shard proofs → MMR inclusion proof)
