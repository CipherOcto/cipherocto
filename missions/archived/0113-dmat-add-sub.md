# Mission: DMAT Addition and Subtraction

## Status
Completed (2026-03-22)

## RFC
RFC-0113 v1.21 (Numeric): Deterministic Matrices (DMAT)

## Summary
Implemented MAT_ADD and MAT_SUB operations with full Phase 0-3 validation, TRAP sentinel detection, dimension/scale checking, and gas accounting.

## Acceptance Criteria
- [x] `mat_add(a: &DMat<T>, b: &DMat<T>) -> Result<DMat<T>, Error>`
- [x] `mat_sub(a: &DMat<T>, b: &DMat<T>) -> Result<DMat<T>, Error>`
- [x] Phase 0: TRAP sentinel pre-check (all elements, a then b)
- [x] Phase 1: Dimension validation (M×N ≤ 64, M≤8, N≤8, M≥1, N≥1, a.dims == b.dims)
- [x] Phase 2: Scale validation (all elements uniform, cross-matrix scale match)
- [x] Phase 3: Compute (element-wise add/sub)
- [x] Gas: `10 × M × N`

## Implementation Details
- `validate_additive_op<T: NumericScalar>(a: &DMat<T>, b: &DMat<T>) -> Result<(usize, usize, u8), DmatError>`: shared validation helper returning (rows, cols, scale)
- Global TRAP Invariant: scans operand `a` fully before operand `b` (row-major order)
- Gas tracking: `gas_add_sub(rows, cols)` returning `10 * rows * cols`
- `DMatIndex` type and `Index` trait for convenient `mat[(i, j)]` syntax

## Test Results
- 11 new tests added (31 DMAT tests total pass)
- 320 total tests pass
- Clippy clean

## Dependencies
- Mission 0113-dmat-core-type (completed)

## Location
`determin/src/dmat.rs`

## Complexity
Low — straightforward element-wise operations

## Reference
- RFC-0113 §MAT_ADD
- RFC-0113 §MAT_SUB
- RFC-0113 §Gas Model
- RFC-0113 §TRAP Codes
