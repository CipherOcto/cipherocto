# RFC-0914 (Economics): Stoolap-Only Quota Router Persistence

## Status

Draft (v2) - Updated scope to match implementation

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

- Key storage and validation ✅ (RFC-0903)
- Budget ledger (key_spend table) ✅
- Rate limiting state (in-memory KeyRateLimiter) ✅
- Distributed cache invalidation via WAL pub/sub ✅ (RFC-0913)
- Row locking via FOR UPDATE ✅ (RFC-0912)
- HTTP API routes for key management ✅ (Mission 0903-f)

### Out of Scope

- L1 cache table (defer to future phase)
- Rate limit state table (in-memory sufficient for current scale)
- Other CipherOcto components using Redis
- Full pub/sub protocol (only cache invalidation)

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

### Phase 1: Foundation (MVE) ✅ DONE
- Use Stoolap as primary DB
- Key storage and validation
- Budget ledger (key_spend table)

### Phase 2: Rate Limiting ✅ DONE
- In-memory KeyRateLimiter
- RPM/TPM enforcement

### Phase 3: Pub/Sub Integration ✅ DONE (RFC-0913)
- WAL pub/sub for cache invalidation
- DatabaseEvent types for key invalidation

### Phase 4: Row Locking ✅ DONE (RFC-0912)
- FOR UPDATE syntax in Stoolap
- Atomic budget updates

### Phase 5: Key Management API ✅ DONE
- HTTP routes for CRUD operations (Mission 0903-f)

## Data Model (Implemented)

### api_keys Table

```sql
CREATE TABLE api_keys (
    key_id TEXT NOT NULL UNIQUE,
    key_hash TEXT NOT NULL UNIQUE,
    key_prefix TEXT NOT NULL,
    team_id TEXT,
    budget_limit INTEGER NOT NULL,
    rpm_limit INTEGER,
    tpm_limit INTEGER,
    created_at INTEGER NOT NULL,
    expires_at INTEGER,
    revoked INTEGER DEFAULT 0,
    ...
);
```

### key_spend Table (Budget Ledger)

```sql
CREATE TABLE key_spend (
    key_id TEXT NOT NULL UNIQUE,
    total_spend INTEGER NOT NULL DEFAULT 0,
    window_start INTEGER NOT NULL,
    last_updated INTEGER NOT NULL
);
```

### teams Table

```sql
CREATE TABLE teams (
    team_id TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    budget_limit INTEGER NOT NULL,
    created_at INTEGER NOT NULL
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

- [x] RFC-0903 (Virtual API Key System) - Final
- [x] RFC-0912 (FOR UPDATE) - Accepted, 3/3 missions complete
- [x] RFC-0913 (Pub/Sub) - Accepted, 4/4 missions complete
- [x] Key storage and validation working
- [x] Budget ledger (key_spend) implemented
- [x] Rate limiting (in-memory) implemented
- [x] Key management HTTP routes (Mission 0903-f)
- [ ] L1 cache table (future phase - optional)
- [ ] Rate limit state table (future phase - optional)

## Related Use Case

- `docs/use-cases/stoolap-only-persistence.md`

## Current Status

This RFC describes the current architecture of quota-router. The core functionality is implemented:
- Stoolap as the single persistence layer
- No Redis dependency
- In-memory rate limiting with pub/sub-based invalidation

Future enhancements (optional):
- L1 cache table for higher cache hit rates
- Rate limit state table for distributed rate limiting

## Related RFCs

- RFC-0903: Virtual API Key System (Final)
- RFC-0912: Stoolap FOR UPDATE Row Locking
- RFC-0913: Stoolap Pub/Sub for Cache Invalidation