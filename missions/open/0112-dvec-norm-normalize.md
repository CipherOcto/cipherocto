# Mission: DVEC NORM and NORMALIZE Operations

## Status
Open (unclaimed)

## RFC
RFC-0112 v1.14 (Numeric): Deterministic Vectors (DVEC)

## Summary
Implement NORM (L2 norm = sqrt of dot product) and NORMALIZE (divide each element by norm). NORM is deprecated for consensus but required for the probe. NORMALIZE is FORBIDDEN in consensus (exceeds gas budget).

## Acceptance Criteria
- [ ] `norm(a: &[T]) -> Result<T, Error>` implemented
- [ ] `normalize(a: &[T]) -> Result<Vec<T>, Error>` implemented
- [ ] NORM TRAPs for DQA with `UNSUPPORTED_OPERATION` (DQA has no SQRT per RFC-0105)
- [ ] NORM for Decimal: `a[0].scale <= 9` enforced (for SQRT precision)
- [ ] NORM zero vector returns zero (not an error)
- [ ] NORMALIZE TRAPs with `CONSENSUS_RESTRICTION` (forbidden in consensus)
- [ ] NORMALIZE TRAPs with `CANNOT_NORMALIZE_ZERO_VECTOR` if norm is zero
- [ ] NORMALIZE uses element-wise division: `a[i].div(norm)?`

## Gas Notes
- NORM gas = DOT_PRODUCT + GAS_SQRT ≈ 17,752 (within 50k budget)
- NORMALIZE gas = NORM + N × GAS_DIV ≈ 269,000 (EXCEEDS 50k — FORBIDDEN in consensus)
- NORMALIZE allowed only in Analytics/Off-chain queries

## Dependencies
- Mission 0112-dvec-core-type (must be completed first)
- Mission 0112-dvec-arithmetic-dot-product-squared-distance (NORM calls dot_product)
- RFC-0111 §SQRT algorithm (for NORM's sqrt step)
- RFC-0112 §NORM algorithm
- RFC-0112 §NORMALIZE algorithm
- RFC-0112 §Production Limitations

## Location
`determin/src/dvec.rs`

## Complexity
Medium — NORM is mostly composition (dot_product + sqrt), NORMALIZE is element-wise division

## Reference
- RFC-0112 §NORM
- RFC-0112 §NORMALIZE
- RFC-0112 §Gas Model
- RFC-0111 v1.20 §SQRT (for the sqrt algorithm NORM calls)
- RFC-0112 §Probe Entry Details (entries 40-47, 56)
