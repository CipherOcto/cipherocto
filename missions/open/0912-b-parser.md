# Mission: FOR UPDATE Parser Implementation

## Status
Open

## RFC
RFC-0912: Stoolap FOR UPDATE Row Locking

## Summary
Add FOR UPDATE syntax parsing in parser/statements.rs after OFFSET clause.

## Acceptance Criteria
- [ ] Parse FOR UPDATE after ORDER BY, LIMIT, and OFFSET
- [ ] Handle keyword validation (FOR UPDATE, not FOR in other contexts)
- [ ] Unit tests for parser

## Complexity
Medium

## Prerequisites
- Mission-0912-a: AST and Display (must complete first)

## Implementation Notes
- Parse around line 190 in statements.rs after OFFSET parsing
- Use peek_token_is_keyword("FOR") then expect_keyword("UPDATE")
- Add grammar rule to documentation

## Location
`/home/mmacedoeu/_w/databases/stoolap/src/parser/`

---
**Mission Type:** Implementation
**Priority:** High
**Phase:** Stoolap FOR UPDATE