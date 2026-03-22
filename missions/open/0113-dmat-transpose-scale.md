# Mission: DMAT Transpose and Scale

## Status
Open (unclaimed)

## RFC
RFC-0113 v1.21 (Numeric): Deterministic Matrices (DMAT)

## Summary
Implement MAT_TRANSPOSE (row-to-column layout swap) and MAT_SCALE (scalar multiplication).

## Acceptance Criteria
- [ ] `mat_transpose(a: &DMat<T>) -> Result<DMat<T>, Error>`
  - [ ] Phase 0: TRAP sentinel pre-check
  - [ ] Phase 1: Dimension validation (M×N ≤ 64, M≤8, N≤8, M≥1, N≥1)
  - [ ] Phase 2: Scale validation (uniform elements)
  - [ ] Phase 3: Compute (result.rows = a.cols, result.cols = a.rows, copy with index swap)
  - [ ] Gas: `2 × M × N`
- [ ] `mat_scale(a: &DMat<T>, scalar: T) -> Result<DMat<T>, Error>`
  - [ ] Phase 0: TRAP sentinel pre-check (scalar FIRST, then matrix)
  - [ ] Phase 1: Dimension validation
  - [ ] Phase 2: Scale validation + result_scale check
  - [ ] Phase 3: Compute element-wise multiplication
  - [ ] Gas: `M × N × (20 + 3 × s_a × s_scalar)`

## Dependencies
- Mission 0113-dmat-core-type (must complete first)

## Location
`determin/src/dmat.rs`

## Complexity
Low — straightforward layout swap and element-wise operations

## Reference
- RFC-0113 §MAT_TRANSPOSE
- RFC-0113 §MAT_SCALE
- RFC-0113 §Gas Model
