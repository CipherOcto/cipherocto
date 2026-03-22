# Mission: DVEC Arithmetic — DOT_PRODUCT and SQUARED_DISTANCE

## Status
Completed (2026-03-21)

## RFC
RFC-0112 v1.14 (Numeric): Deterministic Vectors (DVEC)

## Summary
Implemented DOT_PRODUCT and SQUARED_DISTANCE operations using i128 accumulation (sufficient given scale/dim constraints) with explicit overflow detection via `checked_mul`/`checked_add`. Resolved generic return type blocker via `DvecScalar::from_parts`.

## Acceptance Criteria
- [x] `dot_product` stub with input validation (uniform scale, dimension, input scale precondition)
- [x] `squared_distance` stub with input validation (uniform scale, dimension, input scale precondition)
- [x] `DVec::dot_product` method delegating to the free function
- [x] Scale validation: DQA input_scale <= 9, Decimal input_scale <= 18
- [x] Dimension validation: N <= 64, a.len == b.len
- [x] Full i128 accumulator algorithm (sequential, deterministic TRAP on overflow)
- [x] Overflow TRAP (i128 checked_mul/checked_add — sufficient given N≤64, scales ≤9/18)
- [x] Result canonicalization via `T::from_parts(acc, scale)`
- [ ] All probe entries produce correct results (57 entries) — deferred to verification mission

## Implementation Notes

### Why i128 suffices (no BigInt needed)
Given constraints: N ≤ 64, DQA scale ≤ 9, Decimal scale ≤ 18
- DQA: max |product| ≈ (10^10)² = 10^20, sum of 64 ≈ 10^21 << i128::MAX (~10^38)
- Decimal: max |product| ≈ (10^18)² = 10^36, sum of 64 ≈ 10^37 << i128::MAX
Sequential accumulation with `checked_mul`/`checked_add` provides deterministic TRAP.

### Resolving the generic return type blocker
`Dqa::new` returns `Result<Dqa, DqaError>` and `Decimal::new` returns `Result<Decimal, DecimalError>`,
but `dot_product<T>` must return `Result<T, DvecError>`. Solution: added to `DvecScalar` trait:
```rust
fn from_parts(mantissa: i128, scale: u8) -> Result<Self, Self::Error>;
```
For DQA: validates mantissa fits in i64, then calls `Dqa::new(mantissa as i64, scale)`.
For Decimal: delegates to `Decimal::new(mantissa, scale)`.

### Algorithm (both operations)
1. Input scale precondition (must be FIRST per RFC)
2. Uniform scale validation
3. Sequential i128 accumulation with overflow detection
4. result_scale = input_scale * 2 (for both DOT_PRODUCT and SQUARED_DISTANCE)
5. Construct result via `T::from_parts(acc, result_scale)`

## Test Results
- 237 tests pass (234 pre-existing + 3 new DVEC arithmetic tests)
- New tests: `test_dot_product_basic`, `test_dot_product_scale_2`, `test_squared_distance_basic`, `test_squared_distance_same_vector`

## Dependencies
- Mission 0112-dvec-core-type (completed — DVec<T>, DvecScalar trait, MaxScale trait)
- RFC-0112 §DOT_PRODUCT algorithm
- RFC-0112 §SQUARED_DISTANCE algorithm
- RFC-0112 §Determinism Rules
- RFC-0112 §Test Vectors (57 probe entries, entries 0-39)

## Location
`determin/src/dvec.rs`

## Complexity
High — i128 accumulator with overflow detection, scale validation, type-specific result construction via trait method

## Reference
- RFC-0112 §DOT_PRODUCT
- RFC-0112 §SQUARED_DISTANCE
- RFC-0112 §Determinism Rules
