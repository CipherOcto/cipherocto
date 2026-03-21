# Mission: DVEC Element-wise Operations

## Status
Open (unclaimed)

## RFC
RFC-0112 v1.14 (Numeric): Deterministic Vectors (DVEC)

## Summary
Implement element-wise vector operations: VEC_ADD, VEC_SUB, VEC_MUL, and VEC_SCALE. These operate point-wise on corresponding elements and are simpler than the accumulator-based DOT_PRODUCT/SQUARED_DISTANCE.

## Acceptance Criteria
- [ ] `vec_add(a: &[T], b: &[T]) -> Result<Vec<T>, Error>` — element-wise addition
- [ ] `vec_sub(a: &[T], b: &[T]) -> Result<Vec<T>, Error>` — element-wise subtraction
- [ ] `vec_mul(a: &[T], b: &[T]) -> Result<Vec<T>, Error>` — element-wise multiplication
- [ ] `vec_scale(a: &[T], scalar: T) -> Result<Vec<T>, Error>` — multiply all elements by scalar
- [ ] All require `a.len == b.len` (TRAP on dimension mismatch)
- [ ] All require scales to match (TRAP on scale mismatch)
- [ ] Result[i] = a[i].<op>(b[i])? for each element
- [ ] VEC_SCALE: `result[i] = a[i].mul(scalar)?`
- [ ] Results are canonicalized per RFC-0111/RFC-0105

## Probe Entry Notes
Entries 48-51 commit to trivially-verifiable constant values:
- Entry 48 (VEC_ADD): [1,2] + [3,4] = [4,6]
- Entry 49 (VEC_SUB): [4,6] - [1,2] = [3,4]
- Entry 50 (VEC_MUL): [2,3] × [4,5] = [8,15]
- Entry 51 (VEC_SCALE): [1,2] × scalar=2 = [2,4]
- All Decimal type, scale=0

## Gas Notes
- VEC_ADD/SUB/MUL/SCALE: 5 × N gas (N <= 64, so max 320 gas)
- All within consensus budget

## Dependencies
- Mission 0112-dvec-core-type (must be completed first)
- RFC-0112 §Element-wise Operations
- RFC-0112 §Probe Entry Details (entries 48-51)
- RFC-0112 §Gas Model

## Location
`determin/src/dvec.rs`

## Complexity
Low — straightforward element-wise delegation to scalar ops

## Reference
- RFC-0112 §Element-wise Operations
- RFC-0112 §Gas Model
- RFC-0112 §Probe Entry Details (entries 48-51)
