# Mission: DVEC Arithmetic — DOT_PRODUCT and SQUARED_DISTANCE

## Status
In Progress

## RFC
RFC-0112 v1.14 (Numeric): Deterministic Vectors (DVEC)

## Summary
Implementing DOT_PRODUCT and SQUARED_DISTANCE operations with BigInt accumulators and strict scale validation.

## Acceptance Criteria
- [x] `dot_product` stub with input validation (uniform scale, dimension, input scale precondition)
- [x] `squared_distance` stub with input validation (uniform scale, dimension, input scale precondition)
- [x] `DVec::dot_product` method delegating to the free function
- [x] Scale validation: DQA input_scale <= 9, Decimal input_scale <= 18
- [x] Dimension validation: N <= 64, a.len == b.len
- [ ] Full BigInt accumulator algorithm (deferred — type-specific result construction)
- [ ] Overflow TRAP (i64 for DQA, MAX_DECIMAL_MANTISSA for Decimal)
- [ ] Result canonicalization
- [ ] All probe entries produce correct results (57 entries)

## Implementation Notes

### Why the stub returns Unsupported
The full algorithm requires constructing a result scalar from a BigInt accumulator:
- For DQA: accumulator → i64 → `Dqa` (but `Dqa::new` returns `Result<Dqa, DqaError>`)
- For Decimal: accumulator → i128 → `Decimal::new(...)`

The challenge: `dot_product<T: DvecScalar>` must return `Result<T, DvecError>`, but the
result constructors return their own concrete types (`Dqa` or `Decimal`), not `T`. This
requires type-specific paths that don't fit cleanly in a single generic function.

**Solution for the arithmetic mission**: Add `DvecScalar::from_bigint(acc: &BigInt, scale: u8) -> Result<T, DvecError>` to the trait, or use type-tagged helper functions:
- `fn dot_product_dqa(...) -> Result<Dqa, DvecError>`
- `fn dot_product_decimal(...) -> Result<Decimal, DvecError>`

### What the stub validates
- Dimension: `a.len == b.len` → `DimensionMismatch`
- Max dim: `N > 64` → `DimensionExceeded`
- Input scale: `a[0].scale() > 9` (DQA) or `> 18` (Decimal) → `InputScaleExceeded`
- Uniform scale: all elements match `a[0].scale()` → `ScaleMismatch`

Full BigInt accumulation is NOT done in the stub.

## Dependencies
- Mission 0112-dvec-core-type (completed — DVec<T>, DvecScalar trait, MaxScale trait)
- RFC-0112 §DOT_PRODUCT algorithm
- RFC-0112 §SQUARED_DISTANCE algorithm
- RFC-0112 §Determinism Rules
- RFC-0112 §Test Vectors (57 probe entries, entries 0-39)

## Location
`determin/src/dvec.rs`

## Complexity
High — BigInt accumulator, strict scale validation, type-specific result construction

## Reference
- RFC-0112 §DOT_PRODUCT
- RFC-0112 §SQUARED_DISTANCE
- RFC-0112 §Determinism Rules
