# Research: Stoolap RFC-0903 SQL Feature Gap Analysis

**Project**: CipherOcto Quota Router
**Date**: 2026-03-13

---

## Executive Summary

This research investigates the feature gaps between the CipherOcto Stoolap embedded SQL database and the SQL requirements specified in RFC-0903 (Virtual API Key System). The goal is to determine if Stoolap can serve as the sole persistence layer for the quota router without requiring Redis for caching and pub/sub functionality.

**Key Finding:** Stoolap can replace Redis for L1 key caching and rate limiting state, but requires extensions for:
1. Explicit `FOR UPDATE` row locking (critical for multi-router deployments)
2. Partial/filtered indexes
3. Triggers for constraint enforcement
4. **Pub/Sub for distributed cache invalidation** (proposed new feature)

---

## Problem Statement

RFC-0903 specifies a ledger-based architecture for virtual API key management with the following persistence requirements:

1. **Atomic Transactions**: `FOR UPDATE` row locking for budget consistency
2. **Complex Indexing**: Partial indexes, composite indexes, unique constraints
3. **Constraint Enforcement**: CHECK constraints and triggers
4. **Caching**: L1 in-memory cache with TTL + distributed invalidation
5. **Rate Limiting**: TokenBucket state management

Current implementation assumes:
- PostgreSQL-compatible SQL for schema
- Redis for distributed cache invalidation (pub/sub)

The CipherOcto fork of Stoolap aims to be the sole persistence layer, requiring analysis of feature gaps and extension proposals.

---

## Research Scope

### Included
- RFC-0903 SQL schema requirements (DDL, indexes, constraints)
- RFC-0903 cache and rate limiting patterns
- Stoolap current capabilities (per `docs/research/stoolap-research.md`)
- Pub/Sub implementation feasibility in embedded databases

### Excluded
- Full implementation details (belongs in RFC)
- Performance benchmarking (future work)
- Alternative databases (PostgreSQL, SQLite comparison)

---

## Findings

### 1. RFC-0903 SQL Requirements

RFC-0903 defines the following SQL schema:

#### Main Tables
```sql
CREATE TABLE api_keys (
    key_id TEXT PRIMARY KEY,
    key_hash BYTEA NOT NULL,
    key_prefix TEXT NOT NULL CHECK (length(key_prefix) >= 8),
    team_id TEXT,
    budget_limit BIGINT NOT NULL CHECK (budget_limit >= 0),
    rpm_limit INTEGER CHECK (rpm_limit >= 0),
    tpm_limit INTEGER CHECK (tpm_limit >= 0),
    created_at INTEGER NOT NULL,
    expires_at INTEGER,
    revoked INTEGER DEFAULT 0,
    revoked_at INTEGER,
    revoked_by TEXT,
    revocation_reason TEXT,
    key_type TEXT DEFAULT 'default',
    allowed_routes TEXT,
    auto_rotate INTEGER DEFAULT 0,
    rotation_interval_days INTEGER,
    description TEXT,
    metadata TEXT,
    FOREIGN KEY (team_id) REFERENCES teams(team_id) ON DELETE SET NULL
);

-- Trigger to enforce MAX_KEYS_PER_TEAM (100 keys per team)
CREATE OR REPLACE FUNCTION check_team_key_limit()
RETURNS TRIGGER AS $$
BEGIN
    IF NEW.team_id IS NOT NULL THEN
        IF (SELECT COUNT(*) FROM api_keys WHERE team_id = NEW.team_id) >= 100 THEN
            RAISE EXCEPTION 'Team key limit exceeded (max 100 keys)';
        END IF;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_team_key_limit
    BEFORE INSERT ON api_keys
    FOR EACH ROW
    WHEN (NEW.team_id IS NOT NULL)
    EXECUTE FUNCTION check_team_key_limit();
```

#### Indexes
```sql
-- Partial index (active keys only)
CREATE INDEX idx_api_keys_hash_active ON api_keys(key_hash) WHERE revoked = 0;
CREATE UNIQUE INDEX idx_api_keys_key_hash_unique ON api_keys(key_hash);
CREATE INDEX idx_api_keys_team_id ON api_keys(team_id);
CREATE INDEX idx_api_keys_expires ON api_keys(expires_at);
```

#### Ledger Table
```sql
CREATE TABLE spend_ledger (
    event_id TEXT PRIMARY KEY,
    request_id TEXT NOT NULL,
    key_id TEXT NOT NULL,
    team_id TEXT,
    provider TEXT NOT NULL,
    model TEXT NOT NULL,
    input_tokens INTEGER NOT NULL,
    output_tokens INTEGER NOT NULL,
    cost_amount BIGINT NOT NULL,
    pricing_hash BYTEA NOT NULL,
    token_source TEXT NOT NULL,
    timestamp INTEGER NOT NULL,
    UNIQUE(key_id, request_id)
);

CREATE INDEX idx_spend_ledger_key_id ON spend_ledger(key_id);
CREATE INDEX idx_spend_ledger_team_id ON spend_ledger(team_id);
CREATE INDEX idx_spend_ledger_timestamp ON spend_ledger(timestamp);
CREATE INDEX idx_spend_ledger_key_time ON spend_ledger(key_id, timestamp);
```

#### Critical Transaction Pattern
```sql
-- Row locking for atomic budget updates
SELECT budget_limit FROM api_keys WHERE key_id = $1 FOR UPDATE;
-- Then UPDATE/INSERT spend_ledger
```

### 2. Current Stoolap Capabilities

Based on `docs/research/stoolap-research.md`:

| Capability | Status | Notes |
|------------|--------|-------|
| MVCC Transactions | ✅ Implemented | ReadCommitted, Snapshot isolation |
| BTreeIndex | ✅ Implemented | Standard B-tree |
| HashIndex | ✅ Implemented | O(1) equality lookups |
| BitmapIndex | ✅ Implemented | Low-cardinality |
| HnswIndex | ✅ Implemented | Vector search |
| Unique Indexes | ✅ Implemented | Via UNIQUE constraint |
| Composite Indexes | ✅ Implemented | MultiColumnIndex |
| JSON Type | ✅ Implemented | JSON document storage |
| Vector Type | ✅ Implemented | f32 vectors |
| CHECK Constraints | ❓ Unclear | Not documented |
| Triggers | ❓ Unclear | Not documented |
| Partial Indexes | ❓ Unclear | Not documented |
| FOR UPDATE | ❓ Unclear | MVCC exists, syntax unverified |
| Pub/Sub | ❌ Not implemented | Not available |
| Semantic Caching | ✅ Implemented | Predicate subsumption |

### 3. Feature Gap Matrix

> **Code Analysis Verified** (2026-03-13): Analyzed Stoolap source at `/home/mmacedoeu/_w/databases/stoolap/src/`

| RFC-0903 Requirement | Stoolap Support | Gap Severity | Evidence |
|---------------------|-----------------|--------------|----------|
| `FOR UPDATE` row locking | ❌ NOT IMPLEMENTED | **Critical** | No `for_update` field in SelectStatement AST; no parser for `FOR UPDATE` clause |
| Partial indexes (`WHERE`) | ❌ NOT IMPLEMENTED | High | CreateIndexStatement has no `where_clause` field |
| CHECK constraints | ✅ **IMPLEMENTED** | None | Parser token (token.rs:309), AST (ast.rs:1746), Schema (schema.rs:59), DDL executor (ddl.rs:224-241), DML enforcement (dml.rs:639-2467) |
| Triggers | ❌ NOT IMPLEMENTED | Medium | Token exists in token.rs:322, but no parser/executor implementation |
| Unique constraints | ✅ Yes | None | Supported via UNIQUE in table/column constraints |
| Composite indexes | ✅ Yes | None | MultiColumnIndex implemented |
| Foreign keys | ✅ Yes | None | Supported in DDL |
| WAL persistence | ✅ Yes | None | Fully implemented |
| MVCC | ✅ Yes | None | Core feature |

**Summary from Code Analysis:**

- **CHECK constraints**: Fully implemented ✅ - Parser parses CHECK, schema stores expression, DML validates on INSERT/UPDATE
- **TRIGGERS**: NOT implemented ❌ - Only the token exists, no parser/executor
- **FOR UPDATE**: NOT implemented ❌ - No SQL syntax support, internal MVCC methods exist but no SELECT ... FOR UPDATE
- **Partial indexes**: NOT implemented ❌ - CREATE INDEX has no WHERE clause support

### 4. Cache and Rate Limiting Patterns

RFC-0903 uses:

#### L1 Key Cache (In-Memory)
```rust
// Current implementation uses lru crate
use lru::LruCache;
// DashMap for concurrent access
use dashmap::DashMap;
```

**Stoolap Alternative:** Use Stoolap's Semantic Query Caching with TTL-based eviction:
```sql
-- Cache query with predicate
SELECT * FROM api_keys WHERE key_hash = ? AND revoked = 0;
-- Invalidate on mutation
DELETE FROM api_keys_cache WHERE key_hash = ?;
```

#### TokenBucket Rate Limiting
```rust
// Current: DashMap<Uuid, (TokenBucket, TokenBucket)>
pub struct TokenBucket { ... }
```

**Stoolap Alternative:** Store in table:
```sql
CREATE TABLE rate_limit_state (
    key_id TEXT PRIMARY KEY,
    rpm_tokens BIGINT,
    tpm_tokens BIGINT,
    last_refill INTEGER,
    PRIMARY KEY (key_id)
);
```

#### Distributed Cache Invalidation (Pub/Sub)
```rust
// Current: Redis pub/sub
redis::publish("key-invalidation", key_hash);
```

**Gap:** Stoolap does not have pub/sub. This is the primary reason Redis is currently required.

---

## Extension Proposal: Stoolap Pub/Sub

### Rationale

Multi-node quota router deployments require a mechanism to invalidate cached keys across all instances. Currently this requires Redis pub/sub. Adding pub/sub to Stoolap would eliminate the Redis dependency entirely.

### Design Proposal

#### Core Pub/Sub Model

```rust
/// Pub/Sub channel for cache invalidation
pub struct PubSubManager {
    subscriptions: Arc<RwLock<HashMap<String, Vec<Channel>>>>,
    event_loop: EventLoop,
}

impl PubSubManager {
    /// Subscribe to a channel
    pub fn subscribe(&self, channel: &str) -> Receiver<String>;

    /// Publish to a channel
    pub fn publish(&self, channel: &str, message: &str) -> Result<usize>;
}
```

#### SQL Interface

```sql
-- Subscribe to channel (application-level)
CREATE SUBSCRIPTION key_invalidation ON 'cache:invalidate:*';

-- Publish notification
NOTIFY 'cache:invalidate:abc123', 'revoked';
```

#### Use Cases

1. **Key Revocation**: When key is revoked on one node, all nodes update cache
2. **Key Rotation**: Invalidate old key, propagate new key
3. **Budget Updates**: Notify other nodes of balance changes
4. **Rate Limit Sync**: Share rate limit state across nodes

### Implementation Approach

#### Option A: In-Process Pub/Sub (Recommended for MVE)

For single-process deployments with multiple threads:
```rust
// Simple channel-based pub/sub within same process
use tokio::sync::broadcast;

// Global broadcast channel for invalidation events
static INVALIDATION_TX: OnceLock<broadcast::Sender<InvalidationEvent>> = OnceLock::new();
```

#### Option B: WAL-Based Pub/Sub

Leverage existing WAL for cross-instance communication:
```rust
// Write invalidation to WAL
// Other instances poll/analyze WAL for changes
struct WalPubSub {
    wal_manager: WalManager,
    poll_interval: Duration,
}
```

#### Option C: Database Notifications (PostgreSQL-style)

For multi-process (embedded) deployments:
```rust
// Use file-based notifications via inotify (Linux) or FSEvents (macOS)
struct FileBasedPubSub {
    notification_dir: PathBuf,
}
```

### Integration with RFC-0903

```rust
/// Extended KeyCache with distributed invalidation
pub struct DistributedKeyCache {
    local_cache: KeyCache,
    pubsub: PubSubManager,
}

impl DistributedKeyCache {
    pub fn new() -> Self {
        // Subscribe to invalidation channel
        let mut cache = Self { ... };
        cache.pubsub.subscribe("key-invalidation", |msg| {
            // Parse message and invalidate local cache
            let event: InvalidationEvent = serde_json::from_str(&msg)?;
            cache.local_cache.invalidate(&event.key_hash);
        });
        cache
    }
}

// On key mutation, publish invalidation
pub fn revoke_key_with_invalidation(
    db: &Database,
    cache: &DistributedKeyCache,
    key_id: &Uuid,
) -> Result<()> {
    // ... DB operations ...
    cache.pubsub.publish("key-invalidation", &event_json)?;
}
```

---

## Recommendations

### Phase 1: Verified (Code Analysis Complete)

| Action | Effort | Impact | Status |
|--------|--------|--------|--------|
| Verify CHECK constraint support | Low | Unblock schema migration | ✅ **IMPLEMENTED** |
| Verify TRIGGER support | Low | Unblock constraint enforcement | ❌ NOT IMPLEMENTED |
| Implement partial index alternative | Medium | Enable filtered indexes | ❌ NOT IMPLEMENTED |
| Document FOR UPDATE status | Low | Clarify row locking behavior | ❌ NOT IMPLEMENTED |

**Result:** RFC-0903 schema with CHECK constraints is fully compatible with Stoolap. Trigger-based enforcement (MAX_KEYS_PER_TEAM) requires application-level implementation.

### Phase 2: Core Extensions (RFC Candidate)

| Feature | Priority | Description |
|---------|----------|-------------|
| FOR UPDATE syntax | **P0** | Explicit row locking for multi-router |
| Pub/Sub mechanism | **P1** | Distributed cache invalidation |
| Partial indexes | P2 | WHERE clause in index DDL |
| Triggers | P2 | Database-level constraint enforcement |

### Phase 3: Advanced Features

| Feature | Priority | Description |
|---------|----------|-------------|
| Semantic cache tuning | P3 | Optimize for key lookup patterns |
| HNSW vector indexes | P3 | Future AI query support |

### Recommended Path

1. **Short-term**: Add FOR UPDATE syntax to Stoolap fork
2. **Medium-term**: Implement in-process pub/sub (broadcast channel)
3. **Long-term**: Full distributed pub/sub via WAL notification

### Risk Assessment

| Risk | Severity | Mitigation |
|------|----------|------------|
| Stoolap FOR UPDATE not implemented | High | Use application-level locking with file advisory locks |
| Pub/Sub complexity | Medium | Start with in-process, expand later |
| Performance regression | Low | Benchmark before/after |
| Schema compatibility | Medium | Add translation layer if needed |

---

## Next Steps

- [x] Verify Stoolap CHECK constraint support (code analysis) → ✅ IMPLEMENTED
- [x] Verify Stoolap TRIGGER support (code analysis) → ❌ NOT IMPLEMENTED
- [x] Test FOR UPDATE syntax against Stoolap → ❌ NOT IMPLEMENTED
- [ ] Create Use Case for Stoolap-only persistence (no Redis)
- [ ] Draft RFC for Stoolap extensions (pub/sub, FOR UPDATE, Triggers)

---

## References

- RFC-0903: Virtual API Key System (Final)
- Stoolap Research: `docs/research/stoolap-research.md`
- Stoolap Integration Research: `docs/research/stoolap-integration-research.md`
- BLUEPRINT.md: Documentation standards

---

**Research Status:** Complete
**Recommended Action:** Create Use Case for Stoolap-only persistence, then draft RFC for extensions