# RFC-0914 (Economics): Stoolap-Only Quota Router Persistence

## Status

Draft (v8) — Updated schema references and scope clarification

## Authors

- Author: @cipherocto

## Summary

Define the architecture for eliminating Redis dependency from the quota router by using Stoolap as the sole persistence layer for keys, cache, and distributed state.

## Dependencies

**Requires:**

- RFC-0903: Virtual API Key System (Final)
- RFC-0903-B1: Schema Amendments to RFC-0903 Final v22 (for BLOB types on key_id, event_id, request_id)
- RFC-0903-C1: Extended Schema Amendments to RFC-0903 Final (for BLOB types on team_id, api_keys.key_id, api_keys.team_id)
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
- Budget ledger (key_spend table) — DEPRECATED ✅ (replaced by spend_ledger; key_spend retained for migration compatibility only, not for budget enforcement)
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
Target Architecture (not all components implemented — see Scope)
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
│       ✅ IMPL      ✅ IMPL      ❌ OUT     ❌ OUT          │
└─────────────────────────────────────────────────────────────┘

Legend: ✅ IMPL = implemented; ❌ OUT = deferred to future phase (see Out of Scope)
```

**Note on diagram:** The `cache table` and `rate_limit table` components are
**deferred to a future phase** (see Out of Scope). The diagram shows the
target architecture as a goal; the currently implemented components are
`api_keys` and `spend_ledger` only. Do not use this diagram as an
implementation guide — use the schema blocks in RFC-0903-B1 and the
in-scope/out-of-scope sections as the authoritative specification.

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
    key_id BLOB(16) NOT NULL,            -- Raw UUID bytes (16 bytes) — per RFC-0903-B1/C1
    key_hash BYTEA(32) NOT NULL,          -- HMAC-SHA256 (unchanged)
    key_prefix TEXT NOT NULL,             -- Unchanged
    team_id BLOB(16),                     -- Raw UUID bytes (16 bytes) — per RFC-0903-C1
    budget_limit BIGINT NOT NULL,           -- Per RFC-0903-B1/C1 (was INTEGER in RFC-0903 Final)
    rpm_limit INTEGER,
    tpm_limit INTEGER,
    created_at INTEGER NOT NULL,
    expires_at INTEGER,
    revoked INTEGER DEFAULT 0,
    ...
);
```

### teams Table

```sql
CREATE TABLE teams (
    team_id BLOB(16) NOT NULL,           -- Raw UUID bytes (16 bytes) — per RFC-0903-C1
    name TEXT NOT NULL,                    -- Unchanged
    budget_limit BIGINT NOT NULL,           -- Per RFC-0903-C1 (was INTEGER in RFC-0903 Final)
    created_at INTEGER NOT NULL,
    PRIMARY KEY (team_id)
);
```
```

## Legacy Data Model

> **NOTE:** The following table is DEPRECATED and NOT used for budget enforcement. It is retained for migration compatibility only. Do not use in new implementations.

### key_spend Table (DEPRECATED)

Budget enforcement now uses ledger-based `spend_ledger`. The `key_spend` table is retained for legacy rate limiter state. See RFC-0903 Final §Ledger-Based Architecture.

```sql
CREATE TABLE key_spend (
    key_id BLOB(16) NOT NULL,            -- Raw UUID bytes (16 bytes) — per RFC-0903-B1/C1 (FK to api_keys.key_id)
    total_spend INTEGER NOT NULL DEFAULT 0,
    window_start INTEGER NOT NULL,
    last_updated INTEGER NOT NULL,
    UNIQUE(key_id)
);
```

## Why Needed

- Eliminates Redis dependency
- Simplifies deployment
- Single source of truth (Stoolap)
- Enables horizontal scaling

## Constraints

- Must not break RFC-0903 API contracts
- Must maintain <1ms P50 key lookup latency (cache hit path; cache miss requires DB lookup)
- Must preserve MVCC consistency guarantees
- Cache invalidation via WAL pub/sub must propagate within **<100ms** P99 under normal load
  (P50 is typically <5ms; the 100ms P99 bound covers high-throughput key mutation bursts)
  — stale cache entries during this window may allow revoked keys to be used;
  the revocation window is bounded by this staleness limit

## Approval Criteria

- [x] RFC-0903 (Virtual API Key System) - Final
- [x] RFC-0903-B1 (Schema Amendments) v22 - Draft, required for BLOB(16) key_id, event_id, request_id
- [x] RFC-0903-C1 (Extended Schema Amendments) - Draft, required for BLOB(16) team_id, api_keys.key_id, api_keys.team_id
- [x] RFC-0912 (FOR UPDATE) - Accepted, 3/3 missions complete
- [x] RFC-0913 (Pub/Sub) - Accepted, 4/4 missions complete
- [x] Key storage and validation working
- [x] Budget ledger (spend_ledger, ledger-based) implemented — key_spend DEPRECATED and NOT used for budget enforcement
- [x] Rate limiting (in-memory KeyRateLimiter) implemented — state is lost on restart; acceptable for single-node current scale; distributed rate limiting requires future rate_limit state table
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

## Changelog

| Version | Date       | Changes |
|---------|------------|---------|
| v8      | 2026-04-18 | Round 33: fix R33C1 (v6 changelog row restored; was deleted instead of completed); fix R32L1 (Approval Criteria pinned to RFC-0903-B1 v22) |
| v7      | 2026-04-18 | Round 31: version-pin RFC-0903-B1 dependency to v22; update RFC-0914 footer; update Related RFCs section |
| v6      | 2026-04-17 | Round 28 fixes: remove duplicate "## Related RFCs" header causing formatting error |
| v5      | 2026-04-17 | Round 27 fixes: move key_spend to Legacy Data Model section (not in-scope/Implemented); updated version footer |
| v4      | 2026-04-16 | Round 26 fixes: align budget_limit to BIGINT per RFC-0903-B1/C1 (was INTEGER); fix api_keys and teams tables |
| v3      | 2026-04-15 | Add RFC-0903-B1 and RFC-0903-C1 to Dependencies; clarify key_spend DEPRECATED; add diagram legend; update Constraints and Approval Criteria |

## Related RFCs

- RFC-0903: Virtual API Key System (Final)
- RFC-0903-B1: Schema Amendments to RFC-0903 Final v22 (storage types)
- RFC-0903-C1: Extended Schema Amendments to RFC-0903 Final (storage types)
- RFC-0912: Stoolap FOR UPDATE Row Locking (Final)
- RFC-0913: Stoolap Pub/Sub for Cache Invalidation (Accepted)