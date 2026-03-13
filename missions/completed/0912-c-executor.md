# Mission: FOR UPDATE Executor Integration

## Status
Completed

## RFC
RFC-0912: Stoolap FOR UPDATE Row Locking

## Summary
Wire FOR UPDATE flag to table scan methods to use get_visible_versions_for_update.

## Acceptance Criteria
- [x] Modify execute_select in executor/query.rs to check for_update flag
- [x] Route to appropriate version store method based on flag
- [ ] Integration test for concurrent budget updates

## Complexity
High

## Prerequisites
- Mission-0912-b: Parser (must complete first)

## Implementation Notes
- In executor/query.rs:193 execute_select entry point
- Check stmt.for_update flag
- Pass flag to table scan methods
- Use version_store.get_all_visible_rows_for_update() when flag is true

## Location
`/home/mmacedoeu/_w/databases/stoolap/src/executor/`

---
**Mission Type:** Implementation
**Priority:** High
**Phase:** Stoolap FOR UPDATE