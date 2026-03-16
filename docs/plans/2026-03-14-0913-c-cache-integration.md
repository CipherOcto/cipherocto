# Mission 0913-c: Cache Integration

> **For Claude:** Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Integrate WAL pub/sub with quota-router caches for automatic invalidation.

**Background:**
- Stoolap already has pubsub module (from 0913-a/b)
- This mission integrates with quota-router to use the pub/sub for cache invalidation

---

## Task 1: Add pubsub dependency to quota-router-core

**Files:**
- Modify: `crates/quota-router-core/Cargo.toml`

**Step 1: Add stoolap dependency for pubsub**

```toml
# Add pubsub feature to stoolap
stoolap = { path = "/home/mmacedoeu/_w/databases/stoolap", features = ["pubsub"] }
```

**Step 2: Commit**

---

## Task 2: Create cache invalidation handler

**Files:**
- Create: `crates/quota-router-core/src/cache/invalidation.rs`

**Step 1: Create handler**

```rust
use stoolap::pubsub::{DatabaseEvent, PubSubEventType};

/// Handle invalidation events from WAL pub/sub
pub struct CacheInvalidation;

impl CacheInvalidation {
    /// Handle a database event - route to appropriate cache
    pub fn handle_event(&self, event: &DatabaseEvent) {
        match event.event_type() {
            PubSubEventType::KeyInvalidated => {
                // Invalidate key cache
            }
            PubSubEventType::BudgetUpdated => {
                // Refresh budget info
            }
            PubSubEventType::SchemaChanged => {
                // Clear all caches
            }
            _ => {}
        }
    }
}
```

**Step 2: Test**

**Step 3: Commit**

---

## Task 3: Wire up cache invalidation to middleware

**Files:**
- Modify: `crates/quota-router-core/src/middleware.rs`

**Step 1: Add invalidation handling**

Integrate CacheInvalidation with KeyMiddleware to handle events.

**Step 2: Test**

**Step 3: Commit**

