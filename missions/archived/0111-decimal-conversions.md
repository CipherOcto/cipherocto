# Mission: DECIMAL Conversions

## Status
Completed (2026-03-21)

## RFC
RFC-0111 (Numeric): Deterministic DECIMAL

## Summary
Implement DECIMAL conversions to/from DQA, BIGINT, and String per RFC-0111 §Conversions.

## Acceptance Criteria
- [ ] DECIMAL → DQA: scale alignment + RoundHalfEven to DQA scale (0-18)
  - TRAP if DECIMAL scale > 18 or result outside DQA range
- [ ] DQA → DECIMAL: zero-extend + canonicalize
- [ ] DECIMAL → BIGINT: truncate fractional part (no rounding)
  - TRAP if result outside BIGINT range
- [ ] BIGINT → DECIMAL: use RFC-0110 I128_ROUNDTRIP (op 0x000D)
  - TRAP if result outside DECIMAL range
- [ ] DECIMAL → String: deterministic formatting
  - No trailing zeros in fractional part
  - Locale: period (.) as decimal separator, no thousands separators
  - TRAP if result exceeds 256 bytes
- [ ] Numeric Domain Isolation: conversions only at instruction boundaries

## Dependencies
- Mission 0111-decimal-core-type (must complete first)
- RFC-0110 BIGINT implementation available for conversions

## Location
`determin/src/decimal.rs`

## Complexity
Medium — conversion algorithms with boundary checks

## Reference
- RFC-0111 §DECIMAL → DQA, §DQA → DECIMAL
- RFC-0111 §DECIMAL → BIGINT, §BIGINT → DECIMAL
- RFC-0111 §DECIMAL → String (locale specification)
- RFC-0111 §Numeric Domain Isolation
