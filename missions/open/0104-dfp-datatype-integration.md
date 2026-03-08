# Mission: DFP DataType Integration

## Status
Open

## RFC
RFC-0104: Deterministic Floating-Point Abstraction

## Summary
Integrate DFP as a first-class SQL data type with proper type checking and CAST support.

## Acceptance Criteria
- [ ] Add `DataType::DeterministicFloat` variant to SQL parser
- [ ] SQL parser accepts `DFP` type keyword
- [ ] Parse `CAST(... AS DFP)` expressions
- [ ] Type error for FLOAT in deterministic context (compile error)
- [ ] Type promotion rules: INT → DFP implicit, FLOAT → DFP explicit only

## Location
`src/parser/ast.rs`, `src/parser/statements.rs`

## Complexity
Low

## Prerequisites
- Mission 1: DFP Core Type (for DFP type definition)
