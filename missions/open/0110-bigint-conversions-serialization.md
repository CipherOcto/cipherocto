# Mission: BigInt Conversions & Serialization

## Status
Open

## RFC
RFC-0110 (Numeric): Deterministic BIGINT

## Summary
Implement BigInt conversions (i64, i128, string) and canonical wire serialization format.

## Acceptance Criteria
- [ ] From<i64> trait implementation
- [ ] From<i128> trait implementation
- [ ] To<i64> trait implementation (TRAP on overflow)
- [ ] To<i128> trait implementation (TRAP on overflow)
- [ ] FromStr and Display trait implementations
- [ ] Serialization: canonical wire format with version byte
- [ ] Deserialization: with canonical form verification
- [ ] bigint_to_i128_bytes for i128 round-trip conversion

## Location
`stoolap/src/numeric/bigint.rs`

## Complexity
Medium

## Prerequisites
- Mission 0110-bigint-core-algorithms

## Implementation Notes
- Wire format: [version: u8, sign: u8, num_limbs: u8, limbs: [u64; n]]
- Sign byte: 0x00 = positive, 0xFF = negative
- Limbs stored as little-endian u64
- No leading zero limbs in canonical form
- bigint_to_i128_bytes must produce valid two's complement BE representation

## Reference
- RFC-0110: Deterministic BIGINT (§Wire Format)
- RFC-0110: Deterministic BIGINT (§BIGINT to i128 conversion)
