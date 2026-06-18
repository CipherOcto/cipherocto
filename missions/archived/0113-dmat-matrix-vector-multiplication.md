# Mission: DMAT Matrix-Vector Multiplication

## Status
Completed (2026-03-22)

## RFC
RFC-0113 v1.21 (Numeric): Deterministic Matrices (DMAT)

## Summary
Implemented MAT_VEC_MUL producing Vec<T> compatible with RFC-0112 DVec. Returns result length = a.rows (guaranteed ≤ 8).

## Acceptance Criteria
- [x] `mat_vec_mul(a: &DMat<T>, v: &[T]) -> Result<Vec<T>, Error>`
- [x] Phase 0: TRAP sentinel pre-check (matrix elements, then vector elements)
- [x] Phase 1: Dimension validation (a.cols == v.len, M×N ≤ 64, M≤8, N≤8, M≥1, N≥1)
- [x] Phase 2: Matrix scale validation (uniform matrix elements)
- [x] Phase 3: Vector scale validation (uniform vector elements, NOT cross-scale)
- [x] Phase 4: Result scale = s_a + s_v ≤ MAX_SCALE
- [x] Phase 5: Compute dot products (sequential, i then j)
- [x] Gas: `rows × cols × (30 + 3 × s_a × s_v)`

## Implementation Details
- Uses `enumerate()` on vector to avoid `j` only indexing vector clippy warning
- BigInt accumulator for overflow detection (same pattern as MAT_MUL)
- Mixed-scale multiplication: matrix and vector may have different uniform scales

## Test Results
- 10 new MAT_VEC_MUL tests (51 DMAT tests total pass)
- 340 total tests pass
- Clippy clean

## Dependencies
- Mission 0113-dmat-core-type (completed)

## Location
`determin/src/dmat.rs`

## Complexity
Medium — dot product per row, similar to RFC-0112 DOT_PRODUCT

## Reference
- RFC-0113 §MAT_VEC_MUL
- RFC-0113 §MAT_VEC_MUL Scale Derivation
- RFC-0113 §Gas Model
- RFC-0112 §DOT_PRODUCT (for equivalence reference)
