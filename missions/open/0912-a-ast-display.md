# Mission: FOR UPDATE AST and Display Implementation

## Status
Open

## RFC
RFC-0912: Stoolap FOR UPDATE Row Locking

## Summary
Add `for_update: bool` field to SelectStatement AST and implement Display formatting.

## Acceptance Criteria
- [ ] Add `for_update: bool` field to SelectStatement in parser/ast.rs:1435
- [ ] Update Display impl to output "FOR UPDATE" clause
- [ ] Unit tests for AST and Display

## Complexity
Low

## Prerequisites
None

## Implementation Notes
- Located in parser/ast.rs around line 1435
- Field should default to `false`
- Display should append " FOR UPDATE" after all other clauses

## Location
`/home/mmacedoeu/_w/databases/stoolap/src/`

---
**Mission Type:** Implementation
**Priority:** High
**Phase:** Stoolap FOR UPDATE