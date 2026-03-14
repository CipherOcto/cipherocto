# Mission: WAL Pub/Sub Core Module

## Status
Pending

## RFC
RFC-0913: Stoolap Pub/Sub for Cache Invalidation

## Summary
Create core WAL pub/sub module with DatabaseEvent, WalPubSubEntry, PubSubEventType enums, WalWriter, WalReader, and dual-write publish() method.

## Implementation Plan
See: `docs/plans/2026-03-14-wal-pubsub-core-implementation.md`

## Acceptance Criteria
- [ ] Create `src/pubsub/mod.rs` with module exports
- [ ] Create `src/pubsub/event_bus.rs` with EventBus and DatabaseEvent
- [ ] Create `src/pubsub/wal_pubsub.rs` with WalPubSub and IdempotencyTracker
- [ ] Unit tests for event_bus publish/subscribe
- [ ] Unit tests for idempotency tracker
- [ ] Unit tests for event_id generation
- [ ] Add pubsub module to executor/mod.rs

## Complexity
Medium

## Prerequisites
- RFC-0912 (FOR UPDATE) completed

## Implementation Notes
- Use `tokio::sync::broadcast` for local events
- Use `tokio::fs` for WAL I/O
- Event ID: SHA-256 hash of payload + timestamp
- WalPubSubEntry: channel, payload, event_type, event_id, timestamp

## Location
`/home/mmacedoeu/_w/databases/stoolap/src/pubsub/`

---
**Mission Type:** Implementation
**Priority:** High
**Phase:** RFC-0913 WAL Pub/Sub
