# Mission: RFC-0126 DCS Composite Types

## Status
Open

## RFC
RFC-0126 v2.5.1 (Numeric): Deterministic Serialization

## Summary
Implement DCS composite serialization: Enum, DVEC, DMAT, and Struct with field ordering. DVEC uses index ordering, DMAT uses row-major ordering per RFC-0113.

## Acceptance Criteria
- [ ] `dcs_serialize_enum(tag: u8, payload: &[u8]) -> Vec<u8>` — 1 byte tag + payload
- [ ] `dcs_serialize_struct(fields: &[(u32, &[u8])]) -> Vec<u8>` — field_id u32 + serialized value in declared order
- [ ] `dcs_serialize_dvec<T: DcsSerializable>(elements: &[T]) -> Vec<u8>` — u32 length + elements in index order
- [ ] `dcs_serialize_dmat<T: DcsSerializable>(rows: usize, cols: usize, elements: &[T]) -> Vec<u8>` — rows + cols + row-major elements
- [ ] DVEC: serialize elements in index order (0, 1, 2...)
- [ ] DMAT: serialize elements in row-major order per RFC-0113
- [ ] DQA canonicalization before serialization (per RFC-0105)
- [ ] Struct fields serialize in declared order, NOT alphabetical
- [ ] TRAP on unknown field_id during deserialization
- [ ] TRAP on trailing data after last expected field
- [ ] Unit tests for all composite types

## Dependencies
Mission 0126-dcs-core-types (must complete first)

## Location
`determin/src/dcs.rs`

## Complexity
Medium — requires understanding of field ordering invariants

## Implementation Notes
- Struct serialization: field_id u32_be (1-65535) + serialized value
- No field names in wire format — type context comes from struct schema
- DMAT row-major: element(i, j) = elements[i * cols + j]
- DQA must be canonicalized BEFORE serialization (strip trailing zeros)
- Enum payload MUST use i128 big-endian encoding (16 bytes) for integer variants

## Reference
- RFC-0126 §Enum (Tagged Union)
- RFC-0126 §Struct Serialization
- RFC-0126 §DVEC Serialization
- RFC-0126 §DMAT Serialization
- RFC-0126 §DQA Serialization
- RFC-0113 (DMAT row-major ordering)
