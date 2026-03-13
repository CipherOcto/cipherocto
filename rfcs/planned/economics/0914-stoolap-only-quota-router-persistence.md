# RFC-0914 (Economics): Stoolap-Only Quota Router Persistence

## Status

Planned (v1)

## Authors

- Author: @cipherocto

## Summary

Define the architecture for eliminating Redis dependency from the quota router by using Stoolap as the sole persistence layer for keys, cache, and distributed state.

## Dependencies

**Requires:**

- RFC-0903: Virtual API Key System (Final)
- RFC-0912: Stoolap FOR UPDATE Row Locking
- RFC-0913: Stoolap Pub/Sub for Cache Invalidation

**Optional:**

- RFC-0904: Real-Time Cost Tracking

## Motivation

Current quota router architecture:
- Stoolap: Key storage, ledger, metadata
- Redis: L1 cache, pub/sub for invalidation

Dual-system issues:
- Operational complexity (two systems to manage)
- Deployment overhead (Redis server/process)
- Network latency (cache misses)
- Cost (Redis memory + compute)

Goal: Single persistence layer (Stoolap) for all quota router data.

## Scope

### In Scope

- Key storage and validation
- L1 cache (replacing Redis)
- Budget ledger (already in Stoolap)
- Rate limiting state
- Distributed cache invalidation (pub/sub)
- Row locking for atomic updates (FOR UPDATE)

### Out of Scope

- Other CipherOcto components using Redis
- Full pub/sub protocol (only cache invalidation)
- Trigger support (application-layer enforcement)

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                   Quota Router                              │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐        │
│  │   API Key   │  │    L1       │  │   Rate      │        │
│  │  Validation │  │   Cache     │  │  Limiter    │        │
│  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘        │
│         │                │                │                │
│         ▼                ▼                ▼                │
│  ┌─────────────────────────────────────────────────────┐   │
│  │                   Stoolap                           │   │
│  │  ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌─────────┐    │   │
│  │  │ api_keys│ │spend_   │ │ cache   │ │ rate_   │    │   │
│  │  │         │ │ ledger  │ │ table   │ │ limit   │    │   │
│  │  └─────────┘ └─────────┘ └─────────┘ └─────────┘    │   │
│  └─────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────┘
```

### Data Flow

1. **Key Lookup**: Check L1 cache table → fallback to api_keys table
2. **Budget Update**: SELECT ... FOR UPDATE → UPDATE spend_ledger
3. **Cache Invalidation**: On mutation → broadcast to all router threads

## Implementation Phases

### Phase 1: Foundation (MVE)
- Use Stoolap as primary DB (already working)
- Keep Redis for cache/pubsub (dual-write)
- Measure performance delta

### Phase 2: Cache Migration
- Replace Redis L1 cache with Stoolap table
- Implement in-process broadcast for invalidation
- Validate cache hit rate

### Phase 3: Locking
- Add FOR UPDATE syntax to Stoolap (RFC-0912)
- Implement multi-router atomic budget updates
- Test concurrent access patterns

### Phase 4: Full Replacement
- Remove Redis from deployment
- Single-process deployment
- Full integration testing

## Data Model

### Cache Table

```sql
CREATE TABLE key_cache (
    key_hash BYTEA PRIMARY KEY,
    serialized_key TEXT NOT NULL,
    cached_at INTEGER NOT NULL,
    expires_at INTEGER
);

CREATE INDEX idx_key_cache_expires ON key_cache(expires_at);
```

### Rate Limit State Table

```sql
CREATE TABLE rate_limit_state (
    key_id TEXT PRIMARY KEY,
    rpm_tokens BIGINT NOT NULL,
    tpm_tokens BIGINT NOT NULL,
    last_refill INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
```

## Why Needed

- Eliminates Redis dependency
- Simplifies deployment
- Single source of truth (Stoolap)
- Enables horizontal scaling

## Constraints

- Must not break RFC-0903 API contracts
- Must maintain <1ms P50 key lookup latency
- Must preserve MVCC consistency guarantees

## Approval Criteria

- [ ] RFC-0912 (FOR UPDATE) implemented
- [ ] RFC-0913 (Pub/Sub) implemented
- [ ] L1 cache hit rate ≥80%
- [ ] Key lookup latency <1ms P50
- [ ] Multi-router deployment tested
- [ ] Redis removed from deployment config

## Related Use Case

- `docs/use-cases/stoolap-only-persistence.md`

## Related RFCs

- RFC-0903: Virtual API Key System (Final)
- RFC-0912: Stoolap FOR UPDATE Row Locking
- RFC-0913: Stoolap Pub/Sub for Cache Invalidation