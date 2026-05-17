# Mission: WAL Pub/Sub Testing & Configuration

## Status
Open

## RFC
RFC-0913: Stoolap Pub/Sub for Cache Invalidation

## Summary
Add configuration options, integration tests, and verify idempotency. Test multi-process cache invalidation.

## Acceptance Criteria
- [ ] Add `wal_poll_interval_ms` config (default 50)
- [ ] Add `wal_path` config for shared storage
- [ ] Integration test: multi-process cache invalidation
- [ ] Test idempotency via event_id deduplication
- [ ] Test dual-write (broadcast + WAL)
- [ ] Test WAL polling cross-process

## Complexity
Low

## Prerequisites
- Mission 0913-c (Cache Integration)

## Implementation Notes
- Config in quota-router-cli
- Use shared temp directory for multi-process tests
- Test idempotency with duplicate events

## Location
`/home/mmacedoeu/_w/ai/cipherocto/crates/quota-router-cli/src/`
`/home/mmacedoeu/_w/databases/stoolap/src/executor/`

---
**Mission Type:** Implementation
**Priority:** High
**Phase:** RFC-0913 WAL Pub/Sub
