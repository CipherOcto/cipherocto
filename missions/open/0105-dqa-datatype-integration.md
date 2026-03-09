# Mission: DQA DataType Integration

## Status
Open

## RFC
RFC-0105: Deterministic Quant Arithmetic (DQA)

## Summary
Integrate DQA as a first-class SQL data type in stoolap, with parser support, type checking, and column storage.

## Acceptance Criteria
- [ ] Add `DataType::Quant` variant in parser AST
- [ ] SQL parser accepts `DQA(n)` syntax where n is scale (0-18)
- [ ] Type checking for scale alignment in expressions
- [ ] DQA column storage with fixed scale
- [ ] DQA_ASSIGN_TO_COLUMN for inserting values into columns
- [ ] Round-to-column-scale using RoundHalfEven

## Location
`stoolap/src/parser/ast.rs`, `stoolap/src/parser/statements.rs`, `stoolap/src/determ/value.rs`

## Complexity
Low

## Prerequisites
- Mission 1: DQA Core Type (determin/src/dqa.rs must exist)

## Implementation Notes
- Import DQA as path dependency from determin crate
- Column scale is fixed; inserted values are rounded to column scale
- Use DQA_ASSIGN_TO_COLUMN algorithm from RFC-0105
- Storage encoding should canonicalize for Merkle hashing

## Reference
- RFC-0105: Deterministic Quant Arithmetic (§Scale Alignment, §SQL Column Semantics)
- stoolap/src/determ/value.rs (existing DetermValue patterns)
