# Mission: DECIMAL Core Type Implementation

## Status
Completed (2026-03-21)

## RFC
RFC-0111 (Numeric): Deterministic DECIMAL

## Summary
Implement the core Decimal type with i128 mantissa and scale (0-36), including data structure, canonical form, and POW10 table.

## Acceptance Criteria
- [ ] Decimal struct with mantissa (i128) and scale (u8) fields
- [ ] Canonical form enforcement (trailing zeros removed, zero = {0, 0})
- [ ] Decimal Range Invariant: |mantissa| ≤ 10^36 − 1, scale ∈ [0, 36]
- [ ] POW10 table: 37 entries (10^0 to 10^36) as i128
- [ ] DECIMAL_OVERFLOW error for values outside range
- [ ] decimal_is_canonical() validation
- [ ] decimal_canonicalize() function

## Dependencies
None

## Location
`determin/src/decimal.rs` (or within determin crate)

## Complexity
Low — type definition and basic validation

## Reference
- RFC-0111 §Data Structure, §Canonical Form, §Constants, §POW10 Table
