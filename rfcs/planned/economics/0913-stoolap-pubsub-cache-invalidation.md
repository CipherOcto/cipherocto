# RFC-0913 (Economics): Stoolap Pub/Sub for Cache Invalidation

## Status

Planned (v1)

## Authors

- Author: @cipherocto

## Summary

Add pub/sub mechanism to CipherOcto/stoolap for distributed cache invalidation across multiple router instances.

## Dependencies

**Requires:**

- RFC-0903: Virtual API Key System (Final)

## Motivation

Multi-node quota router deployments require cache invalidation across all instances. Currently requires Redis pub/sub:

```rust
// Current: Redis pub/sub
redis::publish("key-invalidation", key_hash);
```

Stoolap-only deployment needs equivalent mechanism without Redis.

## Design

### Option A: In-Process Broadcast (Recommended for MVE)

For single-process, multi-threaded deployments:

```rust
use tokio::sync::broadcast;

// Global broadcast channel for invalidation events
static INVALIDATION_TX: OnceLock<broadcast::Sender<InvalidationEvent>> = OnceLock::new();

pub struct InvalidationEvent {
    pub key_hash: Vec<u8>,
    pub reason: InvalidationReason,  // Revoke, Rotate, Update
    pub timestamp: i64,
}

pub enum InvalidationReason {
    Revoke,
    Rotate,
    UpdateBudget,
    Expire,
}
```

### Option B: WAL-Based Pub/Sub

For multi-process deployments (future):

- Write invalidation events to WAL
- Other instances poll/analyze WAL for changes
- More complex but supports distributed部署

### SQL Interface

```sql
-- Subscribe to channel (application-level)
CREATE SUBSCRIPTION key_invalidation ON 'cache:invalidate:*';

-- Publish notification
NOTIFY 'cache:invalidate:abc123', 'revoked';
```

### Use Cases

1. **Key Revocation**: When key is revoked on one node, all nodes update cache
2. **Key Rotation**: Invalidate old key, propagate new key
3. **Budget Updates**: Notify other nodes of balance changes
4. **Rate Limit Sync**: Share rate limit state across nodes

## Implementation Notes

- Estimated effort: ~2-3 days for Option A
- Start with in-process broadcast (single process, multiple threads)
- Expand to WAL-based for distributed deployments later

## Why Needed

- Eliminates Redis dependency for multi-node cache invalidation
- Completes Stoolap as standalone persistence layer
- Enables horizontal scaling without external cache

## Out of Scope

- Complex message routing (single broadcast sufficient for MVE)
- Persistent message queues (use WAL for durability if needed)
- Multiple pub/sub protocols (single broadcast channel per event type)

## Approval Criteria

- [ ] In-process broadcast channel implemented
- [ ] Key cache subscribes to invalidation channel
- [ ] Key mutation publishes invalidation events
- [ ] Multi-router test confirms cache consistency

## Related Use Case

- `docs/use-cases/stoolap-only-persistence.md`

## Related RFCs

- RFC-0912: Stoolap FOR UPDATE Row Locking