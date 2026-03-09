# Mission: DQA Core Type Implementation

## Status
Open

## RFC
RFC-0105: Deterministic Quant Arithmetic (DQA)

## Summary
Implement the core DQA type with deterministic arithmetic operations in pure integer arithmetic. DQA provides high-performance bounded-range deterministic arithmetic for financial computing.

## Acceptance Criteria
- [ ] DQA struct with value (i64) / scale (u8) fields
- [ ] Arithmetic: add, sub, mul, div (all return Result for overflow safety)
- [ ] Scale alignment rules per RFC-0105
- [ ] RoundHalfEven rounding (banker's rounding)
- [ ] Canonical representation (trailing zeros stripped)
- [ ] From/To f64 conversion
- [ ] Serialization to DqaEncoding (16 bytes)
- [ ] Overflow guards using i128 intermediate
- [ ] Test vectors from RFC-0105 passing

## Location
`determin/src/dqa.rs` (outside stoolap workspace to avoid circular dep)

## Complexity
Low

## Prerequisites
None

## Implementation Notes
- Uses i128 for intermediate calculations to prevent overflow
- POW10 lookup table for 10^n (0-36)
- Scale limit: 0-18 decimal places
- Value range: i64 (-9.2×10¹⁸ to 9.2×10¹⁸)
- All arithmetic operations must canonicalize result
- Use ROUND_HALF_EVEN_WITH_REMAINDER helper for division rounding
- See RFC-0105 for detailed algorithm pseudo-code

## Reference
- RFC-0105: Deterministic Quant Arithmetic
- determin/src/lib.rs (DFP reference implementation)
