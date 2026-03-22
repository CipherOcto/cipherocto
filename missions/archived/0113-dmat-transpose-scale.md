# Mission: DMAT Transpose and Scale

## Status
Completed (2026-03-22)

## RFC
RFC-0113 v1.21 (Numeric): Deterministic Matrices (DMAT)

## Summary
Implemented MAT_TRANSPOSE (row-to-column layout swap) and MAT_SCALE (scalar multiplication) with full Phase 0-3 validation and TRAP sentinel detection.

## Acceptance Criteria
- [x] `mat_transpose(a: &DMat<T>) -> Result<DMat<T>, Error>`
  - [x] Phase 0: TRAP sentinel pre-check
  - [x] Phase 1: Dimension validation (M×N ≤ 64, M≤8, N≤8, M≥1, N≥1)
  - [x] Phase 2: Scale validation (uniform elements)
  - [x] Phase 3: Compute (result.rows = a.cols, result.cols = a.rows, copy with index swap)
  - [x] Gas: `2 × M × N`
- [x] `mat_scale(a: &DMat<T>, scalar: T) -> Result<DMat<T>, Error>`
  - [x] Phase 0: TRAP sentinel pre-check (scalar FIRST, then matrix)
  - [x] Phase 1: Dimension validation
  - [x] Phase 2: Scale validation + result_scale check
  - [x] Phase 3: Compute element-wise multiplication
  - [x] Gas: `M × N × (20 + 3 × s_a × s_scalar)`

## Implementation Details
- Fixed transpose bug: inner loop was using `cols` instead of `rows` for stride
- Scalar TRAP checked before matrix TRAP (per RFC specification)

## Test Results
- 13 new tests for transpose and scale (64 DMAT tests total pass)
- 353 total tests pass
- Clippy clean

## Dependencies
- Mission 0113-dmat-core-type (completed)

## Location
`determin/src/dmat.rs`

## Complexity
Low — straightforward layout swap and element-wise operations

## Reference
- RFC-0113 §MAT_TRANSPOSE
- RFC-0113 §MAT_SCALE
- RFC-0113 §Gas Model
