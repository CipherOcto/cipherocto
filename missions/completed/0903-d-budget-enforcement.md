# Mission: Key Cache (L1)

## Status
Pending

## RFC
RFC-0903: Virtual API Key System

## Summary
Implement L1 in-memory key cache with cache invalidation via RFC-0913 pub/sub events.

## Acceptance Criteria
- [ ] Create cache.rs with KeyCache struct
- [ ] Implement in-memory cache with TTL
- [ ] Implement cache lookup by key_hash
- [ ] Integrate with RFC-0913 pub/sub for invalidation
- [ ] Implement handle_invalidation_event()
- [ ] Unit tests for cache operations

## Complexity
Medium

## Prerequisites
- Mission 0903-a (Key Core) - need keys to cache
- Mission 0913-c (Cache Integration) - need pub/sub events
- RFC-0913 (WAL Pub/Sub) - for event subscription

## Implementation Notes
- Cache key: key_hash
- Cache value: Serialized key data
- TTL: Configurable (default 60s)
- Invalidation events: KeyInvalidated with reason

## Location
`/home/mmacedoeu/_w/ai/cipherocto/crates/quota-router-core/src/`

---
**Mission Type:** Implementation
**Priority:** High
**Phase:** RFC-0903 Virtual API Key
