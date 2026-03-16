# Implementation Plan: WAL Pub/Sub Core (Mission 0913-a)

**Date:** 2026-03-14
**Mission:** 0913-a WAL Pub/Sub Core Module
**RFC:** RFC-0913: Stoolap Pub/Sub for Cache Invalidation

---

## Overview

Create the core pub/sub infrastructure for WAL-based cache invalidation. This includes:
- `pubsub/event_bus.rs` - Local broadcast using tokio::sync::broadcast
- `pubsub/wal_pubsub.rs` - WAL read/write with pub/sub entry type
- `pubsub/mod.rs` - Module exports
- Unit tests for basic operations

---

## Architecture

```
pubsub/
├── mod.rs           # Module exports
├── event_bus.rs     # Local broadcast (tokio::sync::broadcast)
└── wal_pubsub.rs    # WAL read/write (extends WalManager)
```

### Dual-Write Pattern

```rust
// Every publish does both:
pub fn publish(&self, event: DatabaseEvent) -> Result<EventId> {
    // 1. Local broadcast - immediate same-process
    self.event_bus.send(event.clone());

    // 2. WAL write - cross-process
    self.wal_pubsub.write(&event)?;
}
```

---

## File Structure

### pubsub/mod.rs

```rust
pub mod event_bus;
pub mod wal_pubsub;

pub use event_bus::{DatabaseEvent, EventBus, InvalidationReason};
pub use wal_pubsub::{IdempotencyTracker, PubSubEventType, WalPubSub, WalPubSubEntry};
```

### pubsub/event_bus.rs

```rust
use tokio::sync::broadcast;

/// Local broadcast for same-process cache invalidation
pub struct EventBus {
    tx: broadcast::Sender<DatabaseEvent>,
}

impl EventBus {
    pub fn new(capacity: usize) -> Self
    pub fn subscribe(&self) -> broadcast::Receiver<DatabaseEvent>
    pub fn publish(&self, event: DatabaseEvent) -> Result<()>
}

/// Database events for pub/sub
#[derive(Clone, Debug)]
pub enum DatabaseEvent {
    KeyInvalidated {
        key_hash: Vec<u8>,
        reason: InvalidationReason,
        rpm_limit: Option<u32>,
        tpm_limit: Option<u32>,
        event_id: [u8; 32],
    },
    TableModified {
        table_name: String,
        operation: OperationType,
        txn_id: i64,
        event_id: [u8; 32],
    },
    SchemaChanged {
        table_name: String,
        change_type: SchemaChangeType,
        event_id: [u8; 32],
    },
}

pub enum InvalidationReason {
    Revoke,
    Rotate,
    UpdateBudget,
    UpdateRateLimit,
    Expire,
    SchemaChange,
}

pub enum OperationType {
    Insert,
    Update,
    Delete,
}

pub enum SchemaChangeType {
    CreateTable,
    DropTable,
    AlterTable,
}
```

### pubsub/wal_pubsub.rs

```rust
use tokio::sync::mpsc;
use std::collections::HashSet;
use std::sync::Arc;

/// WAL-based pub/sub for cross-process cache invalidation
pub struct WalPubSub {
    wal_path: PathBuf,
    event_type: PubSubEventType,
    idempotency: Arc<IdempotencyTracker>,
}

/// Entry written to WAL for pub/sub
#[derive(Debug, Clone)]
pub struct WalPubSubEntry {
    pub channel: String,
    pub payload: Vec<u8>,
    pub event_type: PubSubEventType,
    pub event_id: [u8; 32],
    pub timestamp: i64,
}

pub enum PubSubEventType {
    KeyInvalidated,
    BudgetUpdated,
    RateLimitUpdated,
    SchemaChanged,
    CacheCleared,
}

/// Idempotency tracker for deduplication
pub struct IdempotencyTracker {
    seen: Arc<RwLock<HashSet<[u8; 32]>>>,
    max_size: usize,
}

impl IdempotencyTracker {
    pub fn new(max_size: usize) -> Self
    pub fn is_duplicate(&self, event_id: [u8; 32]) -> bool
    pub fn mark_seen(&self, event_id: [u8; 32])
}

impl WalPubSub {
    pub fn new(wal_path: &Path) -> Self
    pub fn write(&self, event: &DatabaseEvent) -> Result<EventId>
    pub fn read_from_lsn(&self, last_lsn: u64) -> Result<Vec<WalPubSubEntry>>
}

/// Compute event ID (SHA-256 of payload + timestamp)
fn compute_event_id(payload: &[u8]) -> [u8; 32]
```

---

## Implementation Steps

### Step 1: Create pubsub directory structure

```bash
mkdir -p /home/mmacedoeu/_w/databases/stoolap/src/pubsub
```

### Step 2: Create pubsub/mod.rs

- Define module exports
- Re-export types

### Step 3: Create pubsub/event_bus.rs

- Implement EventBus with tokio::sync::broadcast
- Define DatabaseEvent enum with all variants
- Define supporting enums (InvalidationReason, OperationType, SchemaChangeType)
- Add unit tests

### Step 4: Create pubsub/wal_pubsub.rs

- Implement WalPubSub with WAL file operations
- Define WalPubSubEntry and PubSubEventType
- Implement IdempotencyTracker
- Implement event_id generation (SHA-256)
- Add WAL read/write methods
- Add unit tests

### Step 5: Add to executor module

Update `src/executor/mod.rs`:
```rust
pub mod pubsub;
```

---

## Key Design Decisions (from Brainstorming)

| Decision | Choice | Rationale |
|----------|--------|-----------|
| WAL file | Separate `wal_pubsub.wal` | Cleaner separation from main WAL |
| WAL format | Reuse existing 32-byte header + pubsub flag | Consistency, robustness |
| Module location | `pubsub/` directory | Clean separation from executor |
| Event emission | Observer pattern | Decoupled, flexible |
| Polling | Single shared poller | Efficiency |
| Event schema | Explicit WalPubSubEntry | Per RFC-0913 |

---

## Testing Strategy

### Unit Tests

```rust
#[test]
fn test_event_bus_publish_subscribe() {
    let bus = EventBus::new(100);
    let mut rx = bus.subscribe();

    bus.publish(DatabaseEvent::KeyInvalidated {
        key_hash: vec![1, 2, 3],
        reason: InvalidationReason::Revoke,
        rpm_limit: None,
        tpm_limit: None,
        event_id: [0; 32],
    }).unwrap();

    let event = rx.recv().unwrap();
    assert!(matches!(event, DatabaseEvent::KeyInvalidated { .. }));
}

#[test]
fn test_idempotency_deduplication() {
    let tracker = IdempotencyTracker::new(1000);
    let event_id = [1u8; 32];

    assert!(!tracker.is_duplicate(event_id));
    tracker.mark_seen(event_id);
    assert!(tracker.is_duplicate(event_id));
}

#[test]
fn test_event_id_unique() {
    let id1 = compute_event_id(b"test1");
    let id2 = compute_event_id(b"test2");
    assert_ne!(id1, id2);
}
```

---

## Dependencies

- **Internal:** tokio, sha2 (for event_id), WalManager (reference)
- **Mission 0913-b:** Will wire event emission into key manager
- **Mission 0913-c:** Will integrate caches with EventBus + SharedPoller

---

## Acceptance Criteria

- [ ] `src/pubsub/mod.rs` created with exports
- [ ] `src/pubsub/event_bus.rs` with EventBus and DatabaseEvent
- [ ] `src/pubsub/wal_pubsub.rs` with WalPubSub and IdempotencyTracker
- [ ] Unit tests for event_bus publish/subscribe
- [ ] Unit tests for idempotency tracker
- [ ] Unit tests for event_id generation
- [ ] Module added to executor/mod.rs

---

## Complexity Estimate

- **Lines of code:** ~300-400
- **New files:** 3
- **Dependencies:** 1 (sha2)
- **Risk:** Low - no existing code modification, pure addition
