# RFC-0913 (Economics): Stoolap Pub/Sub for Cache Invalidation

## Status

Draft (v1)

## Authors

- Author: @cipherocto

## Summary

Add pub/sub mechanism to CipherOcto/stoolap for distributed cache invalidation across multiple router instances. Eliminates Redis dependency for multi-node quota router deployments.

## Dependencies

**Requires:**

- RFC-0903: Virtual API Key System (Final)
- RFC-0901: Quota Router Agent (Draft)

## Motivation

Multi-node quota router deployments require cache invalidation across all instances. Currently requires Redis pub/sub:

```rust
// Current: Redis pub/sub
redis::publish("key-invalidation", key_hash);
```

Stoolap-only deployment needs equivalent mechanism without Redis. Additionally, the Stoolap codebase analysis reveals:

### Current Architecture Gaps

1. **No Event System**: Cache invalidation is synchronous and direct
   - `executor/dml.rs` calls `invalidate_semi_join_cache_for_table()`, `invalidate_scalar_subquery_cache_for_table()` directly
   - No pub/sub or notification patterns exist in codebase
   - Grep for "notify", "listener", "EventBus" returns only standard threading primitives

2. **Three Cache Types Need Invalidation**:
   - **Query Cache** (`query_cache.rs`): Caches parsed SQL statements, uses schema epoch for fast invalidation
   - **Semantic Cache** (`semantic_cache.rs`): Intelligent result caching with predicate subsumption, TTL (default 300s), LRU per table+column
   - **Pattern Cache** (`pattern_cache.rs`): Join pattern caching

3. **WAL Already Tracks Operations**:
   - WAL manager (`wal_manager.rs`) has operation type tracking (Insert, Update, Delete, Commit, Rollback)
   - 32-byte header with flags, LSN, entry size
   - Could emit events on commit/rollback

### Stoolap Architecture Summary

| Module | Responsibility |
|--------|----------------|
| `storage/mvcc/engine.rs` | Transaction management, table operations |
| `storage/mvcc/registry.rs` | Track active/committed/aborted transactions |
| `storage/mvcc/version_store.rs` | Row versioning, visibility checking |
| `storage/mvcc/wal_manager.rs` | Durability, crash recovery |
| `executor/dml.rs` | INSERT/UPDATE/DELETE execution, calls invalidation |
| `executor/semantic_cache.rs` | Query result caching with RwLock |
| `executor/context.rs` | Per-query context, cache invalidation functions |

## Design

### Option A: In-Process Broadcast (Recommended for MVE)

For single-process, multi-threaded deployments:

```rust
use tokio::sync::broadcast;

// Global broadcast channel for invalidation events
static INVALIDATION_TX: OnceLock<broadcast::Sender<InvalidationEvent>> = OnceLock::new();

pub struct InvalidationEvent {
    pub key_hash: Vec<u8>,
    pub reason: InvalidationReason,
    pub timestamp: i64,
    pub source_txn_id: Option<i64>,
}

pub enum InvalidationReason {
    Revoke,      // API key revoked
    Rotate,      // Key rotation
    UpdateBudget, // Balance changed
    Expire,      // TTL expired
    SchemaChange, // Table DDL change
}

impl InvalidationEvent {
    pub fn new(reason: InvalidationReason) -> Self {
        Self {
            key_hash: Vec::new(),
            reason,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64,
            source_txn_id: None,
        }
    }
}
```

### Option B: WAL-Based Pub/Sub (For Distributed)

For multi-process deployments (future):

- Write invalidation events to WAL
- Other instances poll/analyze WAL for changes
- More complex but supports distributed deployments

**WAL Event Extension**:
```rust
// Extend WAL entry to include pub/sub events
pub struct WalPubSubEntry {
    pub channel: String,
    pub payload: Vec<u8>,
    pub event_type: PubSubEventType,
}

pub enum PubSubEventType {
    KeyInvalidated,
    BudgetUpdated,
    SchemaChanged,
    CacheCleared,
}
```

### SQL Interface

```sql
-- Subscribe to channel (application-level)
CREATE SUBSCRIPTION key_invalidation ON 'cache:invalidate:*';

-- Publish notification
NOTIFY 'cache:invalidate:abc123', 'revoked';

-- List active subscriptions
SELECT * FROM pg_subscriptions;
```

### Implementation Architecture

#### 1. Event Bus Module (`src/executor/event_bus.rs`)

```rust
use tokio::sync::broadcast;

pub struct EventBus {
    tx: broadcast::Sender<DatabaseEvent>,
}

impl EventBus {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(1000);
        Self { tx }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<DatabaseEvent> {
        self.tx.subscribe()
    }

    pub fn publish(&self, event: DatabaseEvent) {
        let _ = self.tx.send(event);
    }
}

pub enum DatabaseEvent {
    TableModified {
        table_name: String,
        operation: OperationType,
        txn_id: i64,
    },
    KeyInvalidated {
        key_hash: Vec<u8>,
        reason: InvalidationReason,
    },
    SchemaChanged {
        table_name: String,
        change_type: SchemaChangeType,
    },
    TransactionCommited {
        txn_id: i64,
        affected_tables: Vec<String>,
    },
}
```

#### 2. Integration Points

**Transaction Commit** (`engine.rs`):
```rust
// After commit_all_tables() in engine.rs
self.event_bus.publish(DatabaseEvent::TransactionCommited {
    txn_id,
    affected_tables: committed_tables,
});
```

**DML Operations** (`dml.rs`):
```rust
// Replace direct invalidation calls
self.event_bus.publish(DatabaseEvent::TableModified {
    table_name: table_name.to_string(),
    operation: op_type,
    txn_id: self.txn_id,
});
```

**Cache Subscription** (`semantic_cache.rs`):
```rust
impl SemanticCache {
    pub fn with_event_subscription(event_bus: EventBus) -> Self {
        let mut cache = Self::new();
        // Spawn task to handle invalidation events
        tokio::spawn(async move {
            let mut rx = event_bus.subscribe();
            while let Some(event) = rx.recv().await {
                if let DatabaseEvent::TableModified { table_name, .. } = event {
                    cache.invalidate_for_table(&table_name);
                }
            }
        });
        cache
    }
}
```

### Use Cases

1. **Key Revocation**: When key is revoked on one node, all nodes update cache
2. **Key Rotation**: Invalidate old key, propagate new key
3. **Budget Updates**: Notify other nodes of balance changes
4. **Rate Limit Sync**: Share rate limit state across nodes
5. **Schema Changes**: Invalidate cached query plans on DDL
6. **Cross-Cache Coordination**: Vector store indexes sync with table changes

## Implementation Plan

### Phase 1: Core Event System (2 days)

- [ ] Create `src/executor/event_bus.rs`
- [ ] Define `DatabaseEvent` enum with all event types
- [ ] Implement broadcast channel in EventBus
- [ ] Add to ExecutionEngine struct

### Phase 2: Transaction Integration (1 day)

- [ ] Emit events on transaction commit in `engine.rs`
- [ ] Emit events on DML operations in `dml.rs`
- [ ] Add transaction context to events

### Phase 3: Cache Integration (1 day)

- [ ] Add event subscription to SemanticCache
- [ ] Add event subscription to QueryCache
- [ ] Replace synchronous invalidation calls

### Phase 4: SQL Interface (1 day)

- [ ] Implement NOTIFY/LISTEN syntax in parser
- [ ] Add pg_subscriptions system view

## Why Needed

- Eliminates Redis dependency for multi-node cache invalidation
- Completes Stoolap as standalone persistence layer
- Enables horizontal scaling without external cache
- Provides foundation for distributed query coordination

## Out of Scope

- Complex message routing (single broadcast sufficient for MVE)
- Persistent message queues (use WAL for durability if needed)
- Multiple pub/sub protocols (single broadcast channel per event type)
- Cross-database replication (future enhancement)

## Approval Criteria

- [ ] Event bus implemented with broadcast channel
- [ ] Transaction commit publishes events
- [ ] DML operations publish table modified events
- [ ] Semantic cache subscribes to invalidation events
- [ ] Integration test confirms multi-thread cache consistency

## Related Use Cases

- `docs/use-cases/stoolap-only-persistence.md`
- `docs/use-cases/enhanced-quota-router-gateway.md`

## Related RFCs

- RFC-0901: Quota Router Agent
- RFC-0903: Virtual API Key System
- RFC-0914: Stoolap-only Quota Router Persistence (depends on this)

## Technical Notes

### Thread Safety

The codebase uses:
- `parking_lot::RwLock` for most synchronization
- `dashmap` for concurrent hash maps
- `crossbeam` for channels

Event bus should use:
- `tokio::sync::broadcast` for async-safe pub/sub
- `std::sync::broadcast` for sync context

### Performance Considerations

- Broadcast channel buffer: 1000 events (configurable)
- Event serialization: Zero-copy where possible
- Subscription filtering: By table name prefix

### Testing Strategy

```rust
#[test]
fn test_event_bus_publish_subscribe() {
    let bus = EventBus::new();
    let mut rx = bus.subscribe();

    bus.publish(DatabaseEvent::TableModified {
        table_name: "users".to_string(),
        operation: OperationType::Update,
        txn_id: 1,
    });

    let event = rx.recv().unwrap();
    assert!(matches!(event, DatabaseEvent::TableModified { .. }));
}

#[test]
fn test_cache_invalidation_via_event() {
    // Multi-threaded test
    let bus = EventBus::new();
    let cache = SemanticCache::with_event_subscription(bus.clone());

    // Publish invalidation
    bus.publish(DatabaseEvent::TableModified {
        table_name: "users".to_string(),
        operation: OperationType::Update,
        txn_id: 1,
    });

    // Wait for event propagation
    std::thread::sleep(Duration::from_millis(10));

    // Cache should be invalidated
    assert!(!cache.contains_key("SELECT * FROM users"));
}
```
