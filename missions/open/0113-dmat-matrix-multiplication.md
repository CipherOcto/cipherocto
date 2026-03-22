# Mission: DMAT Matrix Multiplication

## Status
Open (unclaimed)

## RFC
RFC-0113 v1.21 (Numeric): Deterministic Matrices (DMAT)

## Summary
Implement MAT_MUL with naive triple loop algorithm, BigInt accumulator, overflow detection, and full Phase 0-4 validation per RFC specification.

## Acceptance Criteria
- [ ] `mat_mul(a: &DMat<T>, b: &DMat<T>) -> Result<DMat<T>, Error>`
- [ ] Phase 0: TRAP sentinel pre-check (all elements, a then b)
- [ ] Phase 1: Dimension validation (a.cols == b.rows, M×N ≤ 64, M≤8, N≤8, M≥1, N≥1)
- [ ] Phase 2: Scale validation (uniform within each matrix)
- [ ] Phase 3: Result scale validation (s_a + s_b ≤ MAX_SCALE)
- [ ] Phase 4: Naive triple loop with BigInt accumulator and overflow detection
- [ ] Gas: `M × N × K × (30 + 3 × s_a × s_b)`

## Algorithm Requirements
```
For i in 0..a.rows:
  For j in 0..b.cols:
    accumulator = BigInt(0)
    For k in 0..a.cols:
      product = a.data[i*a.cols + k].mul(b.data[k*b.cols + j])?
      accumulator = accumulator + BigInt::from(product.raw_mantissa())
    if abs(accumulator) > T::MAX_MANTISSA: TRAP(OVERFLOW)
    result.data[i*result.cols + j] = T::new(accumulator, result_scale)?
```

## Dependencies
- Mission 0113-dmat-core-type (must complete first)

## Location
`determin/src/dmat.rs`

## Complexity
High — triple loop with overflow detection, scale derivation

## Reference
- RFC-0113 §MAT_MUL
- RFC-0113 §MAT_MUL Scale Derivation
- RFC-0113 §Overflow Detection
- RFC-0113 §Gas Model
