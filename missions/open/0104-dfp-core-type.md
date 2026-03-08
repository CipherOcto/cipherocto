# Mission: DFP Core Type Implementation

## Status
Open

## RFC
RFC-0104: Deterministic Floating-Point Abstraction

## Summary
Implement the core DFP type with deterministic arithmetic operations in pure integer arithmetic.

## Acceptance Criteria
- [ ] DFP struct with mantissa/exponent/class/sign fields
- [ ] Canonical normalization (odd mantissa invariant)
- [ ] Arithmetic: add, sub, mul, div
- [ ] Round-to-nearest-even with sticky bit
- [ ] Special values: NaN, ±Infinity, ±0.0 handling
- [ ] Range bounds and overflow/underflow clamping (saturating to MAX/MIN)
- [ ] From/To f64 conversion with subnormal support
- [ ] Serialization to 24-byte DfpEncoding
- [ ] sqrt (square root) - bit-by-bit integer sqrt with 226-bit scaled input
- [ ] **Test vectors: 500+ verified cases** including edge cases
- [ ] **Differential fuzzing** against Berkeley SoftFloat reference

## Location
`determ/dfp.rs`

## Complexity
Medium

## Prerequisites
None

## Notes
- MUST use u256 for intermediate calculations (not u128 which overflows)
- SQRT must use bit-by-bit integer algorithm (no f64::sqrt seed)
- All iterations must execute (no early termination)
- See RFC-0104 Three Golden Rules for critical implementation details
