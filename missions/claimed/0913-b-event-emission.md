# Mission: WAL Pub/Sub Event Emission

## Status
Claimed

## RFC
RFC-0913: Stoolap Pub/Sub for Cache Invalidation

## Summary
Integrate event emission into DML operations. Define EventPublisher trait, wire into ExecutionContext, emit TableModified events after INSERT/UPDATE/DELETE.

## Implementation Plan
See: `docs/plans/2026-03-14-wal-pubsub-event-emission.md`

## Acceptance Criteria
- [ ] Define EventPublisher/EventSubscriber traits in pubsub/
- [ ] Add optional event_publisher to ExecutionContext
- [ ] Emit TableModified event after INSERT
- [ ] Emit TableModified event after UPDATE
- [ ] Emit TableModified event after DELETE
- [ ] Unit tests for event emission

## Claimant
Claude (Agent)

## Pull Request
#

## Notes
- Design: Trait-based pub/sub (EventPublisher/EventSubscriber)
- Location: pubsub/traits.rs
- Integration: ExecutionContext holds Arc<dyn EventPublisher>
- Events emitted: After successful DML operations
- Key management (revoke/rotate) deferred to quota-router-core (future)

## Complexity
Medium

## Prerequisites
- Mission 0913-a (WAL Pub/Sub Core) - COMPLETED

## Implementation Notes
- Use trait-based pub/sub for flexibility
- Events emitted after successful DML operations
- Event must include: table_name, operation, txn_id, event_id

## Location
`/home/mmacedoeu/_w/databases/stoolap/src/pubsub/`
`/home/mmacedoeu/_w/databases/stoolap/src/executor/`

---
**Mission Type:** Implementation
**Priority:** High
**Phase:** RFC-0913 WAL Pub/Sub
