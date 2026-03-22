# Mission: DMAT Core Type Implementation

## Status
Claimed

## RFC
RFC-0113 v1.21 (Numeric): Deterministic Matrices (DMAT)

## Summary
Implement the core DMAT type: `DMat<T>` struct, `NumericScalar` trait with RFC-0113 extensions (MAX_MANTISSA, new(), is_trap()), and row-major memory layout.

## Acceptance Criteria
- [ ] `DMat<T>` struct with rows, cols, data fields
- [ ] `NumericScalar` trait with RFC-0113 additions:
  - [ ] `const MAX_MANTISSA: i128`
  - [ ] `fn new(mantissa: i128, scale: u8) -> Result<Self, Error>`
  - [ ] `fn is_trap(&self) -> bool`
- [ ] Row-major index calculation: `Index(i, j) = i * cols + j`
- [ ] Protocol invariant: `data.len() == rows * cols` enforced at construction
- [ ] DMAT<DFP> is FORBIDDEN (no impl of NumericScalar for Dfp)
- [ ] Implement NumericScalar for Dqa and Decimal

## Dependencies
- RFC-0113 §Type System
- RFC-0113 §Protocol Invariant (CRIT-4)
- RFC-0113 §Trait Version Enforcement (CRITICAL)
- RFC-0105 (DQA)
- RFC-0111 (Decimal)

## Location
`determin/src/dmat.rs` (new file)

## Complexity
Medium — type system and trait design

## Reference
- RFC-0113 §Type System
- RFC-0113 §Memory Layout (Row-Major)
- RFC-0113 §Trait Evolution
