# Mission: DFP Core Type Implementation

## Status
In Progress

## RFC
RFC-0104: Deterministic Floating-Point Abstraction

## Summary
Implement the core DFP type with deterministic arithmetic operations in pure integer arithmetic.

## Acceptance Criteria
- [x] DFP struct with mantissa/exponent/class/sign fields
- [x] Canonical normalization (odd mantissa invariant)
- [x] Arithmetic: add, sub, mul, div
- [x] Round-to-nearest-even with sticky bit
- [x] Special values: NaN, ±Infinity, ±0.0 handling
- [x] Range bounds and overflow/underflow clamping (saturating to MAX/MIN)
- [x] From/To f64 conversion with subnormal support
- [x] Serialization to 24-byte DfpEncoding
- [x] sqrt (square root) - bit-by-bit integer sqrt with 226-bit scaled input
- [x] **Test vectors: 18 verified cases** including edge cases (includes signed-zero)
- [x] **Differential fuzzing** against Berkeley SoftFloat reference

## Location
`determin/src/` (outside workspace to avoid circular dep with stoolap)

## Complexity
Medium

## Prerequisites
None

## Implementation Notes
- Uses U256 (hi:lo tuple) for intermediate calculations
- SQRT uses bit-by-bit integer algorithm with 256-bit multiplication
- All iterations execute (no early termination) - RFC-0104 §3 rule
- See RFC-0104 Three Golden Rules for critical implementation details
- Can be imported by stoolap as path dependency
- **Compiler flags (RFC-0104 §2.4):**
  - Use `release` profile (overflow checks OFF)
  - Do NOT use debug profile for DFP operations
  - LTO enabled for optimization
  - Run tests/fuzz in release mode: `cargo test --release`
  - Or use custom profiles: `cargo test --profile test`

## Completed
- Created `determin/` crate (moved from crates/octo-determin)
- Implemented Dfp struct with normalization
- Implemented dfp_add, dfp_sub, dfp_mul, dfp_div
- Implemented round_to_113 with RNE and sticky bit
- Implemented 256-bit arithmetic helpers (mul_256, shl_256, cmp_256)
- 12 passing unit tests (added from_f64, from_i64, to_f64, to_string)
- Clippy clean
- Excluded from workspace to allow stoolap integration
- Integrated into stoolap: Value::dfp(), Value::dfp_from_encoding(), Value::as_dfp()
- Added DFP comparison operators (compare_dfp, compare_dfp_magnitude)
- Added DFP arithmetic in VM (dfp_add, dfp_sub, dfp_mul, dfp_div, dfp_sqrt)
- Added DFP casts: Integer→DFP, Float→DFP, DFP→Float, DFP→Integer, DFP→Text, DFP→Boolean
- stoolap compiles with octo-determin as path dependency
- Added differential fuzzing module (determin/src/fuzz.rs)
- Fuzzing found and fixed bugs:
  * Subtraction sign handling (wraparound)
  * Division overflow panic (shift overflow)
  * Signed zero division (sign not preserved)
- Known issues found by fuzzing (not yet fixed):
  * Division algorithm produces wrong results - needs complete rewrite
  * Edge case alignment issues in add/sub for extreme exponents
