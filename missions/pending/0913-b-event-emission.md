# Mission: WAL Pub/Sub Event Emission

## Status
Pending

## RFC
RFC-0913: Stoolap Pub/Sub for Cache Invalidation

## Summary
Integrate event emission into key manager and DML operations. Publish KeyInvalidated on revoke/rotate/budget update, TableModified on DML operations.

## Acceptance Criteria
- [ ] Add `publish_invalidation()` to key manager operations
- [ ] Emit `KeyInvalidated` event on key revocation
- [ ] Emit `KeyInvalidated` event on key rotation
- [ ] Emit `KeyInvalidated` event on budget update
- [ ] Emit `TableModified` event on DML operations
- [ ] Add event ID generation (SHA-256) to events
- [ ] Unit tests for event emission

## Complexity
Medium

## Prerequisites
- Mission 0913-a (WAL Pub/Sub Core)

## Implementation Notes
- Wire into existing key_manager.rs operations
- Use dual-write: broadcast + WAL
- Event must include: key_hash, reason, rpm_limit, tpm_limit, event_id

## Location
`/home/mmacedoeu/_w/databases/stoolap/src/executor/`
`/home/mmacedoeu/_w/ai/cipherocto/crates/quota-router-core/src/`

---
**Mission Type:** Implementation
**Priority:** High
**Phase:** RFC-0913 WAL Pub/Sub
