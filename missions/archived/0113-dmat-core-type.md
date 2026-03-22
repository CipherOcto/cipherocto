# Mission: DMAT Core Type Implementation

## Status
Completed (2026-03-21)

## RFC
RFC-0113 v1.21 (Numeric): Deterministic Matrices (DMAT)

## Summary
Implemented core DMAT type: `DMat<T>` struct, `NumericScalar` trait (RFC-0113) as sibling to `DvecScalar`, and row-major memory layout.

## Acceptance Criteria
- [x] `DMat<T>` struct with rows, cols, data fields
- [x] `NumericScalar` trait with RFC-0113 additions:
  - [x] `const MAX_MANTISSA: i128`
  - [x] `const MAX_SCALE: u8` (added per design review)
  - [x] `fn new(mantissa: i128, scale: u8) -> Result<Self, Error>`
  - [x] `fn is_trap(&self) -> bool`
- [x] Row-major index calculation: `Index(i, j) = i * cols + j`
- [x] Protocol invariant: `data.len() == rows * cols` enforced at construction
- [x] DMAT<DFP> is FORBIDDEN (no impl of NumericScalar for Dfp)
- [x] Implement NumericScalar for Dqa and Decimal with `&self` receivers

## Design Decisions
- **Sibling traits**: `NumericScalar` (RFC-0113) and `DvecScalar` (RFC-0112) are siblings, not subtrait relationship
- **Error types**: `DmatError` follows `DvecError` pattern with `From` impls for scalar errors
- **Trait receiver convention**: `NumericScalar` uses `&self` receivers per RFC-0113; `DvecScalar` uses consuming `self`

## Test Results
- 20 DMAT unit tests pass
- 309 total tests pass
- Clippy clean

## Dependencies
- RFC-0113 §Type System
- RFC-0113 §Protocol Invariant (CRIT-4)
- RFC-0113 §Trait Version Enforcement (CRITICAL)
- RFC-0105 (DQA)
- RFC-0111 (Decimal)

## Location
`determin/src/dmat.rs` (new file, 529 lines)
`determin/src/lib.rs` (updated)

## Complexity
Medium — type system and trait design

## Reference
- RFC-0113 §Type System
- RFC-0113 §Memory Layout (Row-Major)
- RFC-0113 §Trait Evolution
