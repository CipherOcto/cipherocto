# RFC-0913 (Economics): Stoolap Pub/Sub for Cache Invalidation

## Status

Accepted (v3) - WAL-only with dual-write

## Changelog

- **v3** (2026-03-14): WAL-only architecture with dual-write (broadcast + WAL). 50ms polling interval. Explicit WAL events with event_id for idempotency.
- **v2** (2026-03-14): Added optional `rpm_limit`/`tpm_limit` fields to `KeyInvalidated` event; clarified WAL polling assumptions for multi-process deployments

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

### Architecture: WAL-Only with Dual-Write

**Recommended for all deployments** - single-process and multi-process use the same WAL-based architecture:

```
┌─────────────────────────────────────────────────────────────────┐
│                    WAL-Only Architecture                         │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  ┌──────────────┐     ┌─────────────────────────────────────┐   │
│  │   Process A  │     │            Process B                │   │
│  │              │     │                                       │   │
│  │  ┌────────┐  │     │  ┌────────┐    ┌──────────────┐     │   │
│  │  │ Write  │──┼─────┼──│  WAL   │◄───│   Poller     │     │   │
│  │  └────────┘  │     │  └────────┘    │  (50ms)      │     │   │
│  │       │       │     │       ▲        └──────┬───────┘     │   │
│  │       ▼       │     │       │                │             │   │
│  │  ┌────────┐   │     │  ┌────▼────────────────▼───────┐   │   │
│  │  │Broad-  │   │     │  │    Cache Invalidation       │   │   │
│  │  │cast    │   │     │  │    + Local Broadcast       │   │   │
│  │  └────────┘   │     │  └────────────────────────────┘   │   │
│  │       │       │     │                                      │   │
│  │       ▼       │     │                                      │   │
│  │  ┌────────┐   │     │                                      │   │
│  │  │Cache   │   │     │  ┌────────┐                        │   │
│  │  │Inval  │   │     │  │Cache   │                        │   │
│  │  └────────┘   │     │  │Inval   │                        │   │
│  └──────────────┘     │  └────────┘                        │   │
│                       └─────────────────────────────────────┘   │
│                                                                 │
│  Shared WAL Storage (NFS/GlusterFS/shared block)               │
└─────────────────────────────────────────────────────────────────┘
```

### Dual-Write Pattern

Every write performs two actions:

1. **Local broadcast** - Immediately invalidate cache in current process (<1ms latency)
2. **WAL write** - Persist explicit event to WAL for cross-process propagation

This ensures:
- Same-process gets immediate local invalidation (broadcast)
- Cross-process gets eventual invalidation via WAL polling (50ms)
- No wasted polling for same-process events

### Configuration

| Parameter | Default | Description |
|-----------|---------|-------------|
| `wal_poll_interval` | 50ms | Polling interval for WAL changes |
| `wal_channel_capacity` | 1000 | In-memory channel buffer |
| `wal_enabled` | true | Enable WAL pub/sub (always for multi-process) |

### Event Schema

```rust
/// Explicit event written to WAL for cross-process propagation
pub struct WalPubSubEntry {
    /// Channel name (e.g., "cache:invalidate", "key:revoke")
    pub channel: String,
    /// Serialized event payload
    pub payload: Vec<u8>,
    /// Event type for routing
    pub event_type: PubSubEventType,
    /// Unique identifier for idempotency (txn_id + event_seq)
    pub event_id: [u8; 32],
    /// Timestamp for TTL/decay tracking
    pub timestamp: i64,
}

pub enum PubSubEventType {
    KeyInvalidated,
    BudgetUpdated,
    RateLimitUpdated,
    SchemaChanged,
    CacheCleared,
}

/// Invalidation reason for KeyInvalidated events
pub enum InvalidationReason {
    Revoke,       // API key revoked
    Rotate,       // Key rotation
    UpdateBudget, // Balance changed
    UpdateRateLimit, // RPM/TPM changed
    Expire,       // TTL expired
    SchemaChange, // Table DDL change
}
```

### Why Not Option A (Broadcast-Only)

Option A (pure in-process broadcast) was considered but rejected:

| Criterion | Option A (Broadcast) | Option B (WAL-Only) |
|-----------|----------------------|---------------------|
| Multi-process | Not supported | Native |
| Code paths | Two implementations | Single WAL-based |
| Durability | None (lost on crash) | Events persist |
| Operational | Additional Redis | Stoolap-only |

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

#### 1. WAL Poller Module (`src/executor/wal_pubsub.rs`)

```rust
use tokio::sync::mpsc;
use std::time::Duration;

/// WAL-based pub/sub for cross-process cache invalidation
pub struct WalPubSub {
    /// Channel for local broadcast (same-process)
    local_broadcast: broadcast::Sender<DatabaseEvent>,
    /// WAL writer for cross-process events
    wal_writer: Arc<WalWriter>,
    /// Polling interval
    poll_interval: Duration,
}

impl WalPubSub {
    pub fn new(wal_path: &Path, poll_interval_ms: u64) -> Self {
        let (local_broadcast, _) = broadcast::channel(1000);
        Self {
            local_broadcast,
            wal_writer: Arc::new(WalWriter::new(wal_path)),
            poll_interval: Duration::from_millis(poll_interval_ms),
        }
    }

    /// Dual-write: broadcast locally + write to WAL
    pub fn publish(&self, event: DatabaseEvent) -> Result<EventId> {
        // 1. Local broadcast (immediate, same-process)
        let _ = self.local_broadcast.send(event.clone());

        // 2. Write to WAL (for cross-process)
        let event_id = self.write_to_wal(&event)?;
        Ok(event_id)
    }

    /// Subscribe to local events (same-process)
    pub fn subscribe_local(&self) -> broadcast::Receiver<DatabaseEvent> {
        self.local_broadcast.subscribe()
    }

    /// Start WAL polling task (for cross-process)
    pub fn start_polling(&self, cache: Arc<SemanticCache>) {
        let wal_reader = self.wal_reader.clone();
        let poll_interval = self.poll_interval;

        tokio::spawn(async move {
            let mut last_lsn = 0u64;

            loop {
                tokio::time::sleep(poll_interval).await;

                // Poll WAL for new events since last LSN
                let events = wal_reader.read_from_lsn(last_lsn).await;
                for event in events {
                    cache.handle_invalidation_event(&event);
                    last_lsn = event.lsn;
                }
            }
        });
    }
}

/// WAL writer for pub/sub events
pub struct WalWriter {
    path: PathBuf,
}

impl WalWriter {
    pub fn write_event(&self, event: &DatabaseEvent) -> Result<EventId> {
        // Serialize event and write to WAL with PubSubEntry header
        let payload = serde_json::to_vec(event)?;
        let event_id = compute_event_id(&payload);

        let entry = WalPubSubEntry {
            channel: event.channel_name(),
            payload,
            event_type: event.pub_sub_type(),
            event_id,
            timestamp: epoch_millis(),
        };

        self.write(&entry)?;
        Ok(EventId(event_id))
    }
}

/// Event ID for idempotency (SHA-256 hash)
#[derive(Clone, Copy)]
pub struct EventId([u8; 32]);

fn compute_event_id(payload: &[u8]) -> [u8; 32] {
    use sha2::{Sha256, Digest};
    let mut hasher = Sha256::new();
    hasher.update(payload);
    hasher.update(&epoch_millis().to_le_bytes());
    hasher.finalize()
}
```

#### 2. DatabaseEvent Enum

```rust
/// Database events for pub/sub
#[derive(Clone, Debug)]
pub enum DatabaseEvent {
    /// Key invalidated (revoked, rotated, budget changed)
    KeyInvalidated {
        key_hash: Vec<u8>,
        reason: InvalidationReason,
        /// Updated rate limits for cross-process sync
        rpm_limit: Option<u32>,
        tpm_limit: Option<u32>,
        /// Event ID for idempotency
        event_id: [u8; 32],
    },
    /// Table modified (for query cache invalidation)
    TableModified {
        table_name: String,
        operation: OperationType,
        txn_id: i64,
        event_id: [u8; 32],
    },
    /// Schema changed (DDL)
    SchemaChanged {
        table_name: String,
        change_type: SchemaChangeType,
        event_id: [u8; 32],
    },
    /// Transaction committed
    TransactionCommited {
        txn_id: i64,
        affected_tables: Vec<String>,
        event_id: [u8; 32],
    },
}

impl DatabaseEvent {
    /// Channel name for routing
    pub fn channel_name(&self) -> String {
        match self {
            DatabaseEvent::KeyInvalidated { .. } => "key:invalidate".to_string(),
            DatabaseEvent::TableModified { table_name, .. } => {
                format!("table:{}", table_name)
            }
            DatabaseEvent::SchemaChanged { table_name, .. } => {
                format!("schema:{}", table_name)
            }
            DatabaseEvent::TransactionCommited { .. } => "txn:commit".to_string(),
        }
    }

    /// PubSubEventType for WAL entry
    pub fn pub_sub_type(&self) -> PubSubEventType {
        match self {
            DatabaseEvent::KeyInvalidated { .. } => PubSubEventType::KeyInvalidated,
            DatabaseEvent::TableModified { .. } => PubSubEventType::CacheCleared,
            DatabaseEvent::SchemaChanged { .. } => PubSubEventType::SchemaChanged,
            DatabaseEvent::TransactionCommited { .. } => PubSubEventType::CacheCleared,
        }
    }
}
```

#### 3. Integration Points

**Dual-write on mutation** (`dml.rs` / `key_manager.rs`):

```rust
/// Publish cache invalidation with dual-write
fn publish_invalidation(&self, event: DatabaseEvent) -> Result<()> {
    // 1. Local broadcast - immediate same-process invalidation
    self.wal_pubsub.publish(event.clone())?;

    // 2. WAL write - cross-process propagation
    // (publish() does both)
    Ok(())
}

/// Example: Key revocation
pub fn revoke_key(&self, key_id: Uuid) -> Result<()> {
    // ... DB operations ...

    // Publish invalidation (dual-write)
    self.wal_pubsub.publish(DatabaseEvent::KeyInvalidated {
        key_hash: key_hash.to_vec(),
        reason: InvalidationReason::Revoke,
        rpm_limit: None,
        tpm_limit: None,
        event_id: [0; 32],
    })?;
}
```

**WAL Poller** (per process, spawns on startup):

```rust
/// Initialize WAL pub/sub with cache subscription
pub fn init_wal_pubsub(cache: Arc<SemanticCache>, config: &Config) -> WalPubSub {
    let pubsub = WalPubSub::new(&config.wal_path, config.wal_poll_interval_ms);

    // Start polling for cross-process events
    pubsub.start_polling(cache);

    pubsub
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

### Phase 1: Core WAL Pub/Sub (2 days)

- [ ] Create `src/executor/wal_pubsub.rs`
- [ ] Define `DatabaseEvent`, `WalPubSubEntry`, `PubSubEventType` enums
- [ ] Implement `WalWriter` for writing events to WAL
- [ ] Implement `WalReader` for polling WAL changes
- [ ] Add dual-write to `publish()` method

### Phase 2: Event Emission (1 day)

- [ ] Add `publish_invalidation()` to key manager operations
- [ ] Emit `KeyInvalidated` on revoke, rotate, budget update
- [ ] Emit `TableModified` on DML operations
- [ ] Add event ID generation (SHA-256)

### Phase 3: Cache Integration (1 day)

- [ ] Create `WalPubSub` instance in quota-router initialization
- [ ] Add `start_polling()` task to spawn background poller
- [ ] Wire `SemanticCache::handle_invalidation_event()`
- [ ] Add event subscription to QueryCache

### Phase 4: Testing & Config (1 day)

- [ ] Integration test: multi-process cache invalidation
- [ ] Configuration: `wal_poll_interval_ms` (default 50)
- [ ] Configuration: `wal_path` for shared storage
- [ ] Test idempotency via event_id deduplication

## Why Needed

- Eliminates Redis dependency for multi-node cache invalidation
- Completes Stoolap as standalone persistence layer
- Enables horizontal scaling without external cache
- Provides foundation for distributed query coordination

## Out of Scope

- PostgreSQL NOTIFY/LISTEN (WAL-only, no SQL interface)
- Redis pub/sub replacement (WAL-based only)
- Leader election for rate limiting (single primary per RFC-0903)
- Cross-database replication (future enhancement)
- Multi-WAL sharding (single shared WAL)

## Approval Criteria

- [ ] WAL pub/sub module implemented with dual-write
- [ ] Local broadcast provides <1ms same-process invalidation
- [ ] WAL polling provides <50ms cross-process invalidation
- [ ] Semantic cache handles invalidation events from poller
- [ ] Query cache handles invalidation events from poller
- [ ] Integration test confirms multi-process cache consistency
- [ ] Idempotency verified via event_id deduplication

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

WAL Pub/Sub uses:
- `tokio::sync::broadcast` for local (same-process) events
- `tokio::time::interval` for WAL polling
- File I/O with `tokio::fs` for WAL read/write

### WAL Polling Implementation

The poller reads from the shared WAL file:

```rust
async fn poll_wal(last_lsn: u64) -> Vec<WalPubSubEntry> {
    // Open WAL file in read mode
    // Seek to last known LSN
    // Read new entries since last LSN
    // Filter for PubSubEntry type
    // Update last_lsn
}
```

**Critical:** WAL must be on shared storage (NFS, GlusterFS, or shared block device). Each process tracks its own `last_lsn` position.

### Idempotency

Events include `event_id` (SHA-256 hash of payload + timestamp). Each process maintains a seen set:

```rust
struct IdempotencyTracker {
    seen: Arc<RwLock<HashSet<[u8; 32]>>>,
    max_size: usize, // Prevent unbounded growth
}

impl IdempotencyTracker {
    fn is_duplicate(&self, event_id: [u8; 32]) -> bool {
        self.seen.read().contains(&event_id)
    }

    fn mark_seen(&self, event_id: [u8; 32]) {
        let mut seen = self.seen.write();
        if seen.len() >= self.max_size {
            // Evict oldest (simple strategy: clear half)
            let to_keep: HashSet<_> = seen.iter().skip(self.max_size / 2).cloned().collect();
            *seen = to_keep;
        }
        seen.insert(event_id);
    }
}
```
- `std::sync::broadcast` for sync context

### Performance Considerations

- Broadcast channel buffer: 1000 events (configurable)
- Event serialization: Zero-copy where possible
- Subscription filtering: By table name prefix

### Testing Strategy

```rust
#[test]
fn test_dual_write_broadcast_and_wal() {
    // Test that publish() does both local broadcast and WAL write
    let pubsub = WalPubSub::new(&temp_wal_dir, 50);
    let mut rx = pubsub.subscribe_local();

    let event = DatabaseEvent::KeyInvalidated {
        key_hash: vec![1, 2, 3],
        reason: InvalidationReason::Revoke,
        rpm_limit: None,
        tpm_limit: None,
        event_id: [0; 32],
    };

    let event_id = pubsub.publish(event.clone()).unwrap();

    // 1. Local broadcast should be immediate
    let local_event = rx.recv().unwrap();
    assert!(matches!(local_event, DatabaseEvent::KeyInvalidated { .. }));

    // 2. WAL should contain the event
    let wal_events = read_wal(&temp_wal_dir).await;
    assert!(wal_events.iter().any(|e| e.event_id == event_id));
}

#[tokio::test]
async fn test_wal_polling_cross_process() {
    // Set up two processes with shared WAL
    let wal_path = shared_wal_dir();

    let pubsub_a = WalPubSub::new(&wal_path, 50);
    let cache_b = Arc::new(SemanticCache::new());
    let pubsub_b = WalPubSub::new(&wal_path, 50);
    pubsub_b.start_polling(cache_b.clone());

    // Process A publishes invalidation
    pubsub_a.publish(DatabaseEvent::KeyInvalidated {
        key_hash: vec![1, 2, 3],
        reason: InvalidationReason::Revoke,
        rpm_limit: None,
        tpm_limit: None,
        event_id: [0; 32],
    }).await.unwrap();

    // Process B should receive it via polling (within 50ms)
    tokio::time::sleep(Duration::from_millis(100)).await;

    assert!(!cache_b.contains_key(&[1, 2, 3]));
}

#[test]
fn test_idempotency_deduplication() {
    let tracker = IdempotencyTracker::new(1000);
    let event_id = [1u8; 32];

    // First occurrence: not a duplicate
    assert!(!tracker.is_duplicate(event_id));
    tracker.mark_seen(event_id);

    // Second occurrence: duplicate
    assert!(tracker.is_duplicate(event_id));
}
```
