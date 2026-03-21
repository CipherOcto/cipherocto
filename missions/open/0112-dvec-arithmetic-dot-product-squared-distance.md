# Mission: DVEC Arithmetic — DOT_PRODUCT and SQUARED_DISTANCE

## Status
Open (unclaimed)

## RFC
RFC-0112 v1.14 (Numeric): Deterministic Vectors (DVEC)

## Summary
Implement the two primary DVEC arithmetic operations: DOT_PRODUCT (dot product of two vectors) and SQUARED_DISTANCE (sum of squared element differences). Both use BigInt accumulators and require strict scale validation.

## Acceptance Criteria
- [ ] `dot_product(a: &[T], b: &[T]) -> Result<T, Error>` implemented
- [ ] `squared_distance(a: &[T], b: &[T]) -> Result<T, Error>` implemented
- [ ] Sequential left-to-right accumulation (MANDATORY, not tree reduction)
- [ ] BigInt accumulator for intermediate arithmetic
- [ ] Scale validation: all elements must have same scale
- [ ] Input scale preconditions enforced:
  - DQA: `a[0].scale() <= 9` (result scale <= 18)
  - Decimal: `a[0].scale() <= 18` (result scale <= 36)
- [ ] Scale mismatch TRAP (all elements must match a[0].scale)
- [ ] Dimension validation: `a.len == b.len`, `N <= 64`
- [ ] Result canonicalization before returning
- [ ] Overflow TRAP for i64/i128 conversion (per RFC-0110)
- [ ] All 57 probe entries produce correct results

## Implementation Notes
- **Deterministic TRAP Location**: Sequential accumulation is MANDATORY. `((MAX+1) + 0)` TRAPs at first add; tree reduction would not.
- BigInt accumulator avoids i128 overflow during accumulation
- DOT_PRODUCT: result_scale = a_scale + b_scale
- SQUARED_DISTANCE: result_scale = input_scale * 2

## Dependencies
- Mission 0112-dvec-core-type (must be completed first)
- RFC-0112 §DOT_PRODUCT algorithm
- RFC-0112 §SQUARED_DISTANCE algorithm
- RFC-0105 (DQA) and RFC-0111 (Decimal) for scalar operations

## Location
`determin/src/dvec.rs`

## Complexity
High — BigInt accumulator, strict scale validation, many TRAP conditions

## Reference
- RFC-0112 §DOT_PRODUCT
- RFC-0112 §SQUARED_DISTANCE
- RFC-0112 §Determinism Rules
- RFC-0112 §Test Vectors (57 probe entries, entries 0-39)
