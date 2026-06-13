# Mission: DECIMAL Serialization

## Status
Completed (2026-03-21)

## RFC
RFC-0111 (Numeric): Deterministic DECIMAL

## Summary
Implement DECIMAL serialization (to wire format) and deserialization (from wire format) per RFC-0111 §Canonical Byte Format.

## Acceptance Criteria
- [ ] SERIALIZE: Decimal → 24-byte canonical wire format per RFC-0111 §Canonical Byte Format
  - Byte 0: Version (0x01)
  - Byte 1: Reserved (0x00)
  - Bytes 2-3: Reserved (0x00)
  - Byte 4: Scale (u8, range 0-36)
  - Bytes 5-7: Reserved (0x00)
  - Bytes 8-23: Mantissa (i128 big-endian, two's complement)
- [ ] DESERIALIZE: 24-byte → Decimal with validation
  - Reject non-canonical representations (bytes 1-3, 5-7 must be 0x00)
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
