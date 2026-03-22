# Mission: RFC-0126 DCS Core Types

## Status
Open

## RFC
RFC-0126 v2.5.1 (Numeric): Deterministic Serialization

## Summary
Implement DCS core serialization primitives: u8, u32, i128, bool, TRAP sentinel, String, Bytes, Option<T>. This mission establishes the foundation for all DCS serialization.

## Acceptance Criteria
- [ ] `dcs_serialize_u8(val: u8) -> Vec<u8>` — raw byte
- [ ] `dcs_serialize_u32(val: u32) -> Vec<u8>` — 4 bytes big-endian
- [ ] `dcs_serialize_i128(val: i128) -> Vec<u8>` — 16 bytes big-endian two's complement
- [ ] `dcs_serialize_bool(val: bool) -> Vec<u8>` — 0x00=false, 0x01=true
- [ ] `dcs_serialize_trap() -> Vec<u8>` — 0xFF (1 byte for primitives)
- [ ] `dcs_serialize_string(s: &str) -> Vec<u8>` — u32 length prefix + UTF-8 bytes (max 1MB)
- [ ] `dcs_serialize_bytes(data: &[u8]) -> Vec<u8>` — u32 length prefix + raw bytes
- [ ] `dcs_serialize_option_none() -> Vec<u8>` — 0x00
- [ ] `dcs_serialize_option_some(payload: &[u8]) -> Vec<u8>` — 0x01 + payload
- [ ] TRAP on invalid bool (not 0x00 or 0x01) during deserialization
- [ ] TRAP on string length > 1MB (DCS_LENGTH_OVERFLOW)
- [ ] Unit tests for all primitives

## Dependencies
None — foundational

## Location
`determin/src/dcs.rs` (new file)

## Complexity
Low — straightforward byte encoding

## Implementation Notes
- All multi-byte integers use big-endian (network byte order)
- Use `to_be_bytes()` / `from_be_bytes()` for serialization
- TRAP on invalid inputs BEFORE serialization (TRAP-Before-Serialize)
- String: length prefix is byte count, not character count
- Option serialization must be within typed container (not bare concatenation)

## Reference
- RFC-0126 §Primitive Type Encodings
- RFC-0126 §String Serialization
- RFC-0126 §Bytes (Raw) Serialization
- RFC-0126 §Option<T> Serialization
- RFC-0126 §Bool Deserialization
