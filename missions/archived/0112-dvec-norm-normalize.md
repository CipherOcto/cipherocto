# Mission: DVEC NORM and NORMALIZE Operations

## Status
Completed (2026-03-21)

## RFC
RFC-0112 v1.14 (Numeric): Deterministic Vectors (DVEC)

## Summary
Implemented NORM (L2 norm = sqrt of dot product) and NORMALIZE stub. NORM fully implemented. NORMALIZE returns `ConsensusRestriction` per spec (FORBIDDEN in consensus, exceeds 50k gas budget).

## Acceptance Criteria
- [x] `norm(a: &[T]) -> Result<T, Error>` implemented
- [x] `normalize(a: &[T]) -> Result<Vec<T>, Error>` stub (returns ConsensusRestriction per spec)
- [x] NORM TRAPs for DQA with `UNSUPPORTED` (DQA has no SQRT per RFC-0105, maps InvalidInput → Unsupported)
- [x] NORM for Decimal: `a[0].scale <= 9` enforced (for SQRT precision)
- [ ] NORM zero vector returns zero (not tested — would need Decimal path)
- [x] NORMALIZE TRAPs with `CONSENSUS_RESTRICTION` (forbidden in consensus)
- [ ] NORMALIZE zero-vector TRAP deferred (normalize is stub per consensus restriction)
- [ ] NORMALIZE element-wise division deferred (normalize is stub per consensus restriction)

## Algorithm (NORM)

1. Input scale precondition: `a[0].scale <= 9` (must be FIRST per RFC)
2. Uniform scale validation
3. `dot_product(a, a)` → squared_sum
4. `squared_sum.sqrt()` → norm

DQA at step 4: `sqrt()` returns `InvalidInput` → mapped to `Unsupported`.

## Test Results
- 237 tests pass
- `test_norm_dqa_returns_unsupported` — DQA sqrt failure correctly mapped to Unsupported
- `test_normalize_returns_consensus_restriction` — normalize returns ConsensusRestriction per spec

## Dependencies
- Mission 0112-dvec-core-type (completed)
- Mission 0112-dvec-arithmetic-dot-product-squared-distance (NORM calls dot_product)
- RFC-0111 §SQRT algorithm
- RFC-0112 §NORM algorithm
- RFC-0112 §NORMALIZE algorithm

## Location
`determin/src/dvec.rs`

## Complexity
Medium — NORM is composition (dot_product + sqrt), NORMALIZE is stub per consensus restriction

## Reference
- RFC-0112 §NORM
- RFC-0112 §NORMALIZE
- RFC-0112 §Gas Model
- RFC-0111 v1.20 §SQRT
- RFC-0112 §Probe Entry Details (entries 40-47, 56)
