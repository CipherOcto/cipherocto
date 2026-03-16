# Use Case: Stoolap-Only Persistence for Quota Router

## Problem

The current quota router design (RFC-0903) requires two persistence systems:

1. **Stoolap** (embedded SQL) - for key storage, ledger, metadata
2. **Redis** - for L1 cache and distributed pub/sub

This dual-system architecture introduces:
- Operational complexity (manage two systems)
- Deployment overhead (Redis server/process)
- Network latency (cache misses require Redis round-trip)
- Cost (Redis memory + compute)

## Stakeholders

- **Primary:** Platform operators deploying quota router
- **Secondary:** DevOps teams managing infrastructure
- **Affected:** End users (indirectly, through deployment reliability)

## Motivation

CipherOcto/stoolap already provides:
- Embedded deployment (no separate server)
- MVCC transactions (ACID compliance)
- Semantic query caching (predicate-based cache)
- WAL persistence (crash recovery)

The research `docs/research/stoolap-rfc0903-sql-feature-gap-analysis.md` confirms:
- CHECK constraints: ✅ Implemented
- FOR UPDATE: ⚠️ Needs extension (~2-3 days)
- Pub/Sub: ❌ Not implemented (needs new feature)
- Triggers: ❌ Not supported (use application layer instead)

**Goal:** Single persistence layer (Stoolap) for all quota router data.

## Success Metrics

| Metric | Target | Measurement |
|--------|--------|-------------|
| Eliminate Redis dependency | 100% | No Redis in deployment config |
| Cache hit rate | ≥80% | Semantic cache + L1 cache |
| Key lookup latency | <1ms | P50 measured in production |
| Data consistency | 100% | MVCC + FOR UPDATE |
| Deployment complexity | Reduce 50% | One process vs two |

## Constraints

- **Must not:** Break existing RFC-0903 API contracts
- **Must not:** Reduce data consistency guarantees
- **Must not:** Increase key validation latency beyond 1ms P50
- **Limited to:** Single-node and multi-node deployments (horizontal scaling via replication)

## Non-Goals

- Replace all Redis use cases in CipherOcto (focus on quota router only)
- Implement full pub/sub protocol (only cache invalidation use case)
- Add triggers to Stoolap (application-layer enforcement is sufficient)

## Impact

### Architecture Change

```
Current (Redis + Stoolap):
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│  Quota      │────▶│   Stoolap   │────▶│    Redis    │
│  Router     │     │ (SQL/Keys)  │     │ (Cache/Pubsub)│
└─────────────┘     └─────────────┘     └─────────────┘

Proposed (Stoolap-only):
┌─────────────┐     ┌─────────────┐
│  Quota      │────▶│   Stoolap   │
│  Router     │     │ (SQL+Cache) │
└─────────────┘     └─────────────┘
```

### Required Stoolap Extensions

| Feature | Status | Implementation Effort |
|---------|--------|----------------------|
| FOR UPDATE | Not implemented | ~2-3 days |
| Pub/Sub (in-process) | Not implemented | ~2-3 days |
| Semantic cache tuning | Implemented | Already works |
| Application-layer checks | Available | Already works |

### Deployment Simplification

| Aspect | Current | Proposed |
|--------|---------|----------|
| Processes | 2 (router + Redis) | 1 (router only) |
| Memory | Router + Redis | Router only |
| Network | Localhost Redis | None (embedded) |
| Config | Complex (Redis URL, pool) | Simple (file path) |

## Implementation Phases

### Phase 1: Foundation (MVE)
- Use Stoolap as primary DB (already working)
- Keep Redis for cache/pubsub (dual-write)
- Measure performance delta

### Phase 2: Cache Migration
- Replace Redis L1 cache with Stoolap semantic cache
- Implement in-process broadcast for cache invalidation
- Validate cache hit rate

### Phase 3: Locking
- Add FOR UPDATE syntax to Stoolap fork
- Implement multi-router atomic budget updates
- Test concurrent access patterns

### Phase 4: Full Replacement
- Remove Redis from deployment
- Single-process deployment
- Full integration testing

## Technical Details

### Cache Strategy

Stoolap's semantic caching already provides predicate-based cache hits:

```sql
-- Cached: amount > 100
-- Query: amount > 150
-- Result: Filter cached results, return subset
```

For L1 cache replacement:
- Use application-level cache with TTL
- Store `key_hash -> serialized(ApiKey)` in dedicated table
- Invalidate on mutation (UPDATE/DELETE)

### Pub/Sub Alternative

For multi-node cache invalidation:

```rust
// In-process broadcast (single process, multiple threads)
use tokio::sync::broadcast;

// On key mutation
let _ = INVALIDATION_TX.send(InvalidationEvent { key_hash, reason });

// On each router thread
let mut rx = INVALIDATION_TX.subscribe();
async {
    while let Ok(event) = rx.recv().await {
        local_cache.invalidate(&event.key_hash);
    }
}
```

### Application-Layer Enforcement

MAX_KEYS_PER_TEAM already implemented at application layer (RFC-0903 v29):

```rust
pub fn check_team_key_limit(db: &Database, team_id: &Uuid) -> Result<(), KeyError> {
    let count: i64 = db.query(
        "SELECT COUNT(*) as cnt FROM api_keys WHERE team_id = $1",
        params![team_id.to_string()],
    )?.next()?.get("cnt")?;

    if count >= MAX_KEYS_PER_TEAM as i64 {
        return Err(KeyError::TeamKeyLimitExceeded { ... });
    }
    Ok(())
}
```

## Risks

| Risk | Severity | Mitigation |
|------|----------|------------|
| FOR UPDATE not performant | Low | Benchmark, optimize if needed |
| Multi-node invalidation | Medium | In-process first, WAL polling later |
| Cache hit rate drop | Medium | Tune semantic cache parameters |
| Schema migration | Low | Version table, migration scripts |

## Related RFCs

- RFC-0903 (Economics): Virtual API Key System (Final v29)
- RFC-0904 (Economics): Real-Time Cost Tracking (Planned)
- RFC-0909 (Economics): Deterministic Quota Accounting (Optional)
- Research: `docs/research/stoolap-rfc0903-sql-feature-gap-analysis.md`