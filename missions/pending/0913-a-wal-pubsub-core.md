# Mission: WAL Pub/Sub Core Module

## Status
Pending

## RFC
RFC-0913: Stoolap Pub/Sub for Cache Invalidation

## Summary
Create core WAL pub/sub module with DatabaseEvent, WalPubSubEntry, PubSubEventType enums, WalWriter, WalReader, and dual-write publish() method.

## Acceptance Criteria
- [ ] Create `src/executor/wal_pubsub.rs`
- [ ] Define `DatabaseEvent` enum with all event types
- [ ] Define `WalPubSubEntry` struct
- [ ] Define `PubSubEventType` enum
- [ ] Implement `WalWriter` for writing events to WAL
- [ ] Implement `WalReader` for polling WAL changes
- [ ] Implement dual-write in `publish()` method
- [ ] Unit tests for basic operations

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
`/home/mmacedoeu/_w/databases/stoolap/src/executor/`

---
**Mission Type:** Implementation
**Priority:** High
**Phase:** RFC-0913 WAL Pub/Sub
