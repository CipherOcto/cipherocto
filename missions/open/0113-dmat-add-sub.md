# Mission: DMAT Addition and Subtraction

## Status
Open (unclaimed)

## RFC
RFC-0113 v1.21 (Numeric): Deterministic Matrices (DMAT)

## Summary
Implement MAT_ADD and MAT_SUB operations with full Phase 0-3 validation, TRAP sentinel detection, and dimension/scale checking.

## Acceptance Criteria
- [ ] `mat_add(a: &DMat<T>, b: &DMat<T>) -> Result<DMat<T>, Error>`
- [ ] `mat_sub(a: &DMat<T>, b: &DMat<T>) -> Result<DMat<T>, Error>`
- [ ] Phase 0: TRAP sentinel pre-check (all elements, a then b)
- [ ] Phase 1: Dimension validation (M×N ≤ 64, M≤8, N≤8, M≥1, N≥1, a.dims == b.dims)
- [ ] Phase 2: Scale validation (all elements uniform, cross-matrix scale match)
- [ ] Phase 3: Compute (element-wise add/sub)
- [ ] Gas: `10 × M × N`

## Dependencies
- Mission 0113-dmat-core-type (must complete first)

## Location
`determin/src/dmat.rs`

## Complexity
Low — straightforward element-wise operations

## Reference
- RFC-0113 §MAT_ADD
- RFC-0113 §MAT_SUB
- RFC-0113 §Gas Model
- RFC-0113 §TRAP Codes
