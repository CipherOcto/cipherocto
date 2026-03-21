# Mission: DECIMAL Serialization

## Status
Open

## RFC
RFC-0111 (Numeric): Deterministic DECIMAL

## Summary
Implement DECIMAL serialization (to wire format) and deserialization (from wire format) per RFC-0111 §Canonical Byte Format.

## Acceptance Criteria
- [ ] SERIALIZE: Decimal → 24-byte canonical wire format
  - bytes 0-15: mantissa (big-endian i128, two's complement)
  - bytes 16-22: zero padding
  - byte 23: scale (u8)
- [ ] DESERIALIZE: 24-byte → Decimal with validation
  - Reject non-canonical representations
  - Validate mantissa range
  - Validate scale ≤ 36
- [ ] Byte format uses big-endian for network order
- [ ] Canonical form required for serialization (reject non-canonical input)

## Dependencies
- Mission 0111-decimal-core-type (must complete first)

## Location
`determin/src/decimal.rs`

## Complexity
Low — straightforward byte encoding

## Reference
- RFC-0111 §Canonical Byte Format
- RFC-0111 §Serialization Invariant
- RFC-0111 §Deserialization Algorithm
