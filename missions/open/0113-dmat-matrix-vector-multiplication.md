# Mission: DMAT Matrix-Vector Multiplication

## Status
Open (unclaimed)

## RFC
RFC-0113 v1.21 (Numeric): Deterministic Matrices (DMAT)

## Summary
Implement MAT_VEC_MUL producing Vec<T> compatible with RFC-0112 DVec. Returns result length = a.rows (guaranteed ≤ 8).

## Acceptance Criteria
- [ ] `mat_vec_mul(a: &DMat<T>, v: &[T]) -> Result<Vec<T>, Error>`
- [ ] Phase 0: TRAP sentinel pre-check (matrix elements, then vector elements)
- [ ] Phase 1: Dimension validation (a.cols == v.len, M×N ≤ 64, M≤8, N≤8, M≥1, N≥1)
- [ ] Phase 2: Matrix scale validation (uniform matrix elements)
- [ ] Phase 3: Vector scale validation (uniform vector elements, NOT cross-scale)
- [ ] Phase 4: Result scale = s_a + s_v ≤ MAX_SCALE
- [ ] Phase 5: Compute dot products (sequential, i then j)
- [ ] Gas: `rows × cols × (30 + 3 × s_a × s_v)`

## Note
Mixed-scale multiplication is allowed: result_scale = a.scale() + v.scale(). Matrix and vector may have different scales.

## Dependencies
- Mission 0113-dmat-core-type (must complete first)

## Location
`determin/src/dmat.rs`

## Complexity
Medium — dot product per row, similar to RFC-0112 DOT_PRODUCT

## Reference
- RFC-0113 §MAT_VEC_MUL
- RFC-0113 §MAT_VEC_MUL Scale Derivation
- RFC-0113 §Gas Model
- RFC-0112 §DOT_PRODUCT (for equivalence reference)
