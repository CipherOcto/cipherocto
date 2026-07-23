# Event-Sourced Ledger Precedents — Synthesis

**Date:** 2026-07-22
**Status:** Research Phase 4 of N — event-sourced ledger precedents (per user direction 2026-07-22)
**Builds on:**
- `docs/research/2026-07-22-value-transfer-model-internal-landscape.md` (Phase 1)
- `docs/research/2026-07-22-grand-design-vaults-capabilities-reservations.md` (Phase 2)
- `docs/research/2026-07-22-external-capability-based-spend-systems.md` (Phase 3)
**Scope:** four production-tested systems with lessons for cipherocto's grand design §9 (Event-Sourced Ledger):

| System | What it ships | What cipherocto learns |
|---|---|---|
| Datomic | ARAR (Assert/Read/Accumulate/Retract) primitive; immutable facts; time model; corrections via retraction | Append-only event log is a 30-year-old solved problem in databases |
| EventStoreDB | Persistent subscriptions; stream projections; categories; $by_category projection | Stream-of-events is the natural fit; projections are first-class |
| Apache Kafka + projections | Log + consumer-side projection; offsets; compaction; exactly-once semantics | Partitioning + log-segment checkpoints are the horizontal-scalability trick |
| Cosmos SDK events | Events emitted from tx handlers; secondary index, NOT source of truth | **Events as observability layer is the wrong model for cipherocto — confirms grand design §9 direction** |

---

## 0. The fundamental question

Grand design §9 says: **balances are projections, not state.**

```text
reject mutable balance rows as canonical state
use append-only event log
```

Phase 4 surveys prior art. Two questions:

1. **Has anyone shipped event-as-source-of-truth at scale?** (Not just observability)
2. **What pitfalls does production experience expose?**

---

## 1. Datomic — the gold standard

### 1.1 What it ships

Datomic is a 10+ year old database from Rich Hickey / Cognitect (now Nubank). Core model:

- **Atomic facts called datoms**: `[entity-id, attribute, value, transaction-id, op]`
- **Indelible and chronological** — change is represented by adding new datoms, never modifying existing ones
- **Time model** — every read is "as of" a specific point in transaction history
- **Corrections via retraction** — "removing" is `op=false`, original datom unchanged
- **Universal schema** — no tables, only user-defined attributes

### 1.2 The ARAR primitive

```text
CRUD → ARAR
Create → Assert
Read   → Read
Update → Accumulate
Delete → Retract
```

The four ops are sufficient for every state transition. "Update" is "Assert a new datom with the same `[E,A]` (entity, attribute) but new `V`". "Delete" is "Retract" — original datom stays in storage but the read filter excludes it.

### 1.3 What cipherocto should adopt

| Datomic concept | Cipherocto current | Recommendation |
|---|---|---|
| Atomic fact with Tx ID | Event row with `tx_id`, `event_id` | Already aligned — make Tx ID mandatory, not optional |
| Indelible | Grand design §9 says append-only | Adopt verbatim |
| Time model | Grand design §7 (event log) | Add `time_query(event_seq <= N)` primitive |
| Corrections via retraction | not explicit | Add `DisputeResolved` event type with `:correction/for` semantics (already in grand design §9 list) |
| Universal schema | Vault-specific fields | Per-event-type attribute schemas — different per event type, but the *event* itself is uniform |

### 1.4 The lesson Datomic teaches

> "Datomic is indelible and chronological. Information accumulates over time, and change is represented by accumulating the new, not by modifying or removing the old."

This is **exactly** the grand design §9 thesis. Datomic proves it's tractable in production. Cipherocto can copy this model directly.

### 1.5 Datomic's hidden cost — separation of storage from query

Datomic stores datoms in a **log-structured** storage layer (similar to Kafka segments), but **queries are computed on-demand** by reading the log into memory and filtering. This is slow without indexes.

Datomic ships **multiple indexes**: EAVT, AEVT, AVET, VAET. Each is a sorted projection of the datom set. Reads pick the right index.

**Cipherocto implication:** grand design §9 needs an **index strategy**, not just a log. Without indexes, every balance read scans the entire event history. Acceptable for ZK proofs (which need the full history anyway), terrible for production reads.

**Recommendation:** add per-resource-type event indexes:

```sql
CREATE INDEX idx_transfer_events_by_vault ON transfer_events(vault_id, event_seq);
CREATE INDEX idx_transfer_events_by_settlement ON transfer_events(settlement_hash);
CREATE INDEX idx_capability_events_by_holder ON capability_events(holder_did, event_seq);
CREATE INDEX idx_settlement_events_by_reservation ON settlement_events(reservation_id, event_seq);
```

These mirror Datomic's EAVT index family but specialized for cipherocto's event types.

---

## 2. EventStoreDB — the streaming variant

### 2.1 What it ships

EventStoreDB (Greg Young's company) is purpose-built for event sourcing. Three primitive concepts:

- **Streams** — ordered sequences of events, named (e.g., `vault-42-USD`)
- **Categories** — secondary index projecting events from multiple streams (e.g., `category:transfer` = all transfer events across all streams)
- **Projections** — server-side functions that consume events and emit new streams

### 2.2 The persistent subscription model

A consumer can subscribe to a stream with:

- **Catch-up subscription** — start from a specific position (event number), read forward until current, then live-tail
- **Persistent subscription** — server tracks consumer group state; competing-consumer semantics; checkpointing built-in

This is **exactly** what cipherocto's resource shards (§10 of grand design) need. Each shard is a stream. Cross-shard queries are categories.

### 2.3 `$by_category` projection — the trick

EventStoreDB's built-in `$by_category` projection automatically links events into category streams. You write to a stream, the projection system makes those events appear in the category stream. Consumers can subscribe to either.

**Cipherocto analogue:** when a `TransferApplied` event is written to the vault's stream, a built-in projection makes it appear in the `category:transfer` and `category:vault:<DID>` streams. No application code needed for the indexing.

**Recommendation:** ship cipherocto with a built-in set of category projections:

```text
category:transfer
category:settlement
category:capability_grant
category:capability_revoke
category:dispute_open
category:dispute_resolve
category:audit_window_open
category:audit_window_close
```

Each projection auto-creates on first event.

### 2.4 What EventStoreDB doesn't do

- No ZK proofs (plaintext events)
- No CRDT semantics for concurrent writers
- No resource partitioning by event type (just stream partitioning)

These are cipherocto's value-adds.

---

## 3. Apache Kafka + projections

### 3.1 What it ships

Kafka's primitive: append-only log of records, partitioned by key. Consumer tracks offset. The whole industry runs on this.

### 3.2 The horizontal-scalability story

```text
log_segment_0: events [0, N0)
log_segment_1: events [N0, N1)
log_segment_2: events [N1, N2)
...
```

Each segment is a file. Old segments can be compacted (delete records with the same key, keep latest) or deleted. Consumer offset is a `(topic, partition, offset)` triple. Producer batches records; broker writes sequentially.

This is **the** production-tested model for high-throughput event logs. Billions of events per day at LinkedIn, Netflix, Uber, etc.

### 3.3 The cipherocto equivalent

Grand design §10 (resource sharding) is **Kafka partitioning by resource type**. The mechanics are:

```text
shard:transfer:resource=OCTO-W      → kafka-style log of transfer events for OCTO-W
shard:settlement:resource=OCTO-W    → settlement events for OCTO-W
shard:capability:holder=<DID>       → capability events for one holder
```

Each shard = one log. Cross-shard settlement = atomic writes across logs (which is exactly what Kafka transactional producers do).

**The cipherocto equivalent of Kafka's log segments is stoolap's `events_<resource>` table partition** — same idea, SQL instead of files.

### 3.4 Log compaction — the underrated primitive

Kafka supports **log compaction**: the broker keeps only the latest record for each key. Old records are deleted. This is **the** production model for maintaining a materialized view alongside the log.

**Cipherocto analogue:** a vault's balance is `SUM(ins) - SUM(outs)` over the event log. Log compaction would store `vault_balance[vault_id] = latest` as a sidecar, with periodic compaction to delete intermediate events. Same as Kafka.

**But cipherocto should NOT compact by default.** ZK proofs require the full event history. Compaction should be opt-in per resource type (e.g., "high-throughput resource shards can compact; cryptographic shards cannot").

### 3.5 What Kafka doesn't solve

- No cryptographic verification of events (signed events are application responsibility)
- No consensus (it's a log, not a state machine)
- No multi-region consistency (cross-region replication is async by default)

Cipherocto adds all three via the sync layer (RFC-0862).

---

## 4. Cosmos SDK events — the wrong model (intentionally)

### 4.1 What it ships

Cosmos SDK emits events from tx handlers as **secondary observability**:

```go
ctx.EventManager().EmitEvent(sdk.NewEvent(
    "transfer",
    sdk.NewAttribute("from", fromAddr),
    sdk.NewAttribute("to", toAddr),
    sdk.NewAttribute("amount", amount.String()),
))
```

Events flow through ABCI to CometBFT (the consensus engine), which includes them in block results. Indexers (e.g., the BigDipper block explorer) consume events.

The **state IS mutated** by the tx handler. Events are **emitted** alongside the mutation.

### 4.2 Why this is wrong for cipherocto

Cosmos's choice optimizes for **two things**:

1. Performance — state mutation is direct, no log replay needed for queries
2. Indexing — events are structured for block explorers

This works because Cosmos's state model is **mutable**: accounts have balances, those balances change. Events are decoupled observability.

**Cipherocto's grand design §9 explicitly rejects this.** Cipherocto's state is the projection; the log is the source. Why?

| Reason | Cosmos | Cipherocto |
|---|---|---|
| ZK proofs of balance | Hard (need to prove state transition) | Easy (recompute from log in ZK circuit) |
| Audit trail | Events (separate) | Log (same thing) |
| Replay determinism | Only for new nodes (state mutation is non-replayable) | Native (genesis → head is the canonical derivation) |
| RFC-0862 sync | Log of state changes (CRDT-like) | Log IS the state |
| Rollback/dispute resolution | Complex (state has history, log is metadata) | Easy (revert last N events) |

### 4.3 The lesson — what to copy from Cosmos

Cosmos's **event schema** (typed events with attribute key-value pairs) is a clean design:

```go
type Event struct {
    Type       string
    Attributes []Attribute  // key-value pairs
}

type Attribute struct {
    Key   []byte
    Value []byte
}
```

**Recommendation:** cipherocto's event schema should mirror this:

```rust
struct Event {
    event_type: String,        // e.g., "TransferApplied"
    attributes: Vec<(Bytes, Bytes)>,
    tx_id: TxId,
    event_seq: u64,            // global monotonic
}
```

Typed events with attribute key-value pairs. Easy to index, easy to filter, easy to ZK-prove (each attribute is a public input).

### 4.4 The lesson — what NOT to copy

Do not let state mutation happen **outside** the event log. In Cosmos, the state changes *and* the events emit. In cipherocto, state changes *only* via event application. No UPDATE statements on `accounts` table; only INSERT into `transfer_events` + a derived projection.

This means cipherocto must forbid direct `UPDATE` on balance columns. Enforce via:

```sql
CREATE TRIGGER no_direct_balance_update
BEFORE UPDATE ON octo_w_balances
WHEN current_query_setting('app.allowed_source') IS NULL OR
     current_query_setting('app.allowed_source') != 'event_projection'
BEGIN
    SELECT raise(ABORT, 'direct UPDATE forbidden; apply via event log');
END;
```

Or simpler: don't have an `accounts` table at all. Only `transfer_events` + a materialized view.

---

## 5. The combined event-sourced ledger pattern

After four systems, the unified pattern:

```text
┌────────────────────────────────────────────────────────────┐
│ Layer 1: Log                                                │
│   - Append-only event table per resource shard              │
│   - Global monotonic event_seq                              │
│   - tx_id for atomicity grouping                            │
│   - Signed events (capability holder signature per event)   │
│   - Datomic: ARAR primitive (Assert/Retract both append)    │
├────────────────────────────────────────────────────────────┤
│ Layer 2: Projections                                         │
│   - Materialized views per vault, per asset, per DID         │
│   - Rebuilt from log on demand                              │
│   - Cached for performance, recomputable from log           │
│   - Kafka: log-compaction equivalent (opt-in)               │
├────────────────────────────────────────────────────────────┤
│ Layer 3: Indexes                                             │
│   - Datomic: EAVT family (Entity/Attribute/Value/Tx)        │
│   - EventStoreDB: $by_category projection                   │
│   - Cipherocto: per-resource indexes (per §1.5)             │
├────────────────────────────────────────────────────────────┤
│ Layer 4: Subscriptions                                       │
│   - EventStoreDB persistent subscription model              │
│   - Consumer groups per resource shard                      │
│   - Catch-up from event_seq + live tail                     │
│   - Cipherocto: each router node is a consumer              │
└────────────────────────────────────────────────────────────┘
```

This is the production-tested shape. Cipherocto can adopt it directly.

---

## 6. Pitfalls the four systems expose

### 6.1 Snapshot drift

**Datomic:** snapshots are stored alongside the log but don't auto-update. Stale snapshots = wrong answers.

**Kafka:** compaction lag = stale materializations.

**Cipherocto implication:** the projection layer needs **invalidation** semantics. When a settlement is disputed, the projection for affected vaults must be marked stale.

**Recommendation:** projections carry a `stale_since: Option<EventSeq>` field. Reads detect staleness; recompute if `stale_since` is non-None.

### 6.2 Replay cost

**EventStoreDB:** projections replay the entire stream on first start. For a multi-year stream, this takes hours.

**Kafka:** consumer lag is bounded by broker capacity, but a new consumer still has to replay.

**Cipherocto implication:** new nodes joining the network must replay the entire event log. For an active AI marketplace, this is millions of events.

**Recommendation:** ship **periodic snapshot bundles** that a new node can fetch + verify + apply incrementally. Datomic has this (`-` segment files); Kafka has this (snapshot + offset commit).

### 6.3 Event schema evolution

**Datomic:** schema is itself represented as datoms. Schema changes are events. Application code must read at the schema version of the data it sees.

**Kafka:** Confluent Schema Registry enforces backward/forward compatibility per topic. Records carry schema ID.

**Cipherocto implication:** event schemas will evolve. Without a schema registry, old readers can't parse new events (or vice versa).

**Recommendation:** each event type carries a `schema_version: u32` field. Readers reject events with unrecognized schema versions (configurable: forward-compatible vs strict).

### 6.4 Concurrent writers

**Datomic:** ACID transactions serialize all writers. No concurrency.

**Kafka:** partitioning serializes writers within a partition. Cross-partition writes are not ordered.

**Cipherocto implication:** if two settlement events for the same vault land on different shards, they may apply in different orders on different nodes. Balance projections diverge.

**Recommendation:** vault-scoped writes must hit the same shard. Shard routing by `vault_id` (not by event type). Grand design §10 needs this update.

### 6.5 Privacy

**Datomic:** plain values. No privacy model.

**Kafka:** plain records. No privacy model.

**EventStoreDB:** plain events. Metadata is plaintext.

**Cosmos:** events are public.

**Cipherocto implication:** none of the four systems solve privacy. cipherocto's whitepaper calls for `PRIVATE` / `CONFIDENTIAL` / `SHARED` / `PUBLIC` data flagging (master plan). For private events, the log entries need ZK proofs of correctness (the projection is correct without revealing inputs).

**Recommendation:** event types have a `visibility: Public | Confidential | Private` flag. `Confidential` events carry an encrypted payload + a commitment. `Private` events carry a ZK proof of correctness. All systems surveyed treat events as public; cipherocto must extend.

---

## 7. Updates to grand design §9 (Event-Sourced Ledger)

Current grand design §9 lists 12 event types:

```
VaultCreated, CapabilityGranted, CapabilityAttenuated, CapabilityExpired,
CapabilityRevoked, ReservationCreated, ReservationUpdated, SettlementCompleted,
TransferApplied, DisputeOpened, DisputeResolved, VaultFrozen, VaultRetired
```

Updates based on Phase 4:

1. **Add `:correction/for: EventId` to `DisputeResolved`** (Datomic pattern). The correction event explicitly references the event it corrects.

2. **Add `schema_version: u32` field to every event.** Required for forward-compat.

3. **Add `visibility: Public | Confidential | Private` field to every event.** Required for whitepaper data flagging.

4. **Add `projection_stale: bool` to the projection layer.** For invalidation on dispute.

5. **Specify shard routing by `vault_id`** (not by event type) for vault-scope consistency.

6. **Add a snapshot bundle format** for new nodes joining.

7. **Replace "balance as projection" with "balance as projection + snapshot + invalidation marker"** — three things, not one.

---

## 8. The cipherocto event schema (concrete)

Based on Cosmos's clean event schema, extended for cipherocto's needs:

```rust
struct Event {
    event_id: EventId,            // global monotonic u64
    event_type: EventType,        // enum (VaultCreated, TransferApplied, ...)
    tx_id: TxId,                  // groups atomic event sets
    schema_version: u32,          // for forward-compat
    visibility: Visibility,       // Public | Confidential | Private
    timestamp: Timestamp,
    attributes: Vec<(Bytes, Bytes)>,   // type-specific payload
    corrections: Vec<EventId>,    // events this one supersedes (Datomic-style)
    signature: Signature,         // who emitted this event
    proof: Option<ZKProof>,       // for Private events
}

enum EventType {
    VaultCreated,
    CapabilityGranted,
    CapabilityAttenuated,
    CapabilityExpired,
    CapabilityRevoked,
    ReservationCreated,
    ReservationUpdated,
    SettlementCompleted,
    TransferApplied,
    DisputeOpened,
    DisputeResolved,
    VaultFrozen,
    VaultRetired,
}

enum Visibility {
    Public,         // plaintext in log
    Confidential,   // encrypted payload + commitment
    Private,        // ZK proof of correctness; payload hidden
}
```

This is closer to Cosmos's design + Datomic's ARAR + ZK for privacy. Practical to ship.

---

## 9. Open questions

| Question | Phase 5 candidate | Phase 7 candidate |
|---|---|---|
| Snapshot bundle format? | Yes (block in delivery) | — |
| Schema registry? | RFC candidate | — |
| Projection invalidation protocol? | RFC candidate | — |
| Cross-shard atomic writes? | — | RFC candidate (Consensus Session §12) |
| ZK proofs of event correctness? | — | RFC candidate (extends RFC-0958) |

---

## 10. References

### External

- Datomic data model: <https://docs.datomic.com/cloud/whatis/data-model.html>
- EventStoreDB docs: <https://developers.eventstore.com> (blocked; model-knowledge supplement)
- Kafka docs: <https://kafka.apache.org/documentation/>
- Fowler Event Sourcing: <https://martinfowler.com/eaaDev/EventSourcing.html>
- Fowler CQRS: <https://martinfowler.com/bliki/CQRS.html>
- Cosmos SDK events: <https://github.com/cosmos/cosmos-sdk-docs/blob/main/docs/learn/advanced/08-events.md>
- Cosmos BaseApp: <https://docs.cosmos.network/sdk/latest/learn/concepts/baseapp>

### Internal

- Grand design doc §9 (Event-Sourced Ledger)
- Grand design doc §10 (Resource Sharding)
- Grand design doc §6 (Audit Window — needs correction event support)
- RFC-0862 (sync as propagation)
- RFC-0958 (ZK capability — extends to events)

---

## 11. Status

This doc = Phase 4 of N research. Four event-sourced precedents surveyed. **Cosmos's model (events as observability) is explicitly rejected for cipherocto; Datomic + EventStoreDB + Kafka patterns are adopted.** Concrete updates proposed to grand design §9: schema_version field, visibility flag, correction references, projection invalidation markers, shard routing fix.

**Next action:** proceed to Phase 5 (enterprise migration playbooks) OR Phase 9 (synthesize Phases 2-4 into RFC-0960 grand-design synthesis). Recommend Phase 9 — Phases 2/3/4 now form a coherent design; the synthesis RFC captures the breakthrough.
