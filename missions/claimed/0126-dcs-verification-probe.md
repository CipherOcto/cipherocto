# Mission: RFC-0126 DCS Verification Probe

## Status
Claimed

## RFC
RFC-0126 v2.5.1 (Numeric): Deterministic Serialization

## Summary
Implement the 17-entry DCS Merkle-committed verification probe for cross-implementation verification. Probe entries cover all DCS serialization cases.

## Acceptance Criteria
- [ ] Implement 17 probe entries (Entry 0-16) per RFC-0126 Table
- [ ] Entry 0: DQA positive canonicalization DQA(1000,3) → DQA(1,0)
- [ ] Entry 1: DQA negative canonicalization DQA(-5000,4) → DQA(-5,1)
- [ ] Entry 2: DVEC [DQA(1,0), DQA(2,0), DQA(3,0)]
- [ ] Entry 3: DMAT 2×2 [[1,2],[3,4]] row-major
- [ ] Entry 4: String "hello" → 5 bytes UTF-8 + length prefix
- [ ] Entry 5: Option::None → 0x00
- [ ] Entry 6: Option::Some(true) → 0x01 || 0x01
- [ ] Entry 7: Enum::Variant2(42) → tag 2 + i128 16 bytes
- [ ] Entry 8: Bool true → 0x01
- [ ] Entry 9: Bool false → 0x00
- [ ] Entry 10: Numeric TRAP → 24 bytes per RFC-0111
- [ ] Entry 11: Bool TRAP → 0xFF
- [ ] Entry 12: I128 positive 42 → 16 bytes big-endian
- [ ] Entry 13: I128 negative -42 → 16 bytes big-endian
- [ ] Entry 14: BIGINT(42) → RFC-0110 BigIntEncoding
- [ ] Entry 15: DFP(42.0) → RFC-0104 DfpEncoding
- [ ] Entry 16: Struct {id:u32=42, name:String="alice", balance:DQA=1.0} field ordering
- [ ] Merkle root computation with domain separation (SHA256(0x00 || leaf), SHA256(0x01 || internal))
- [ ] Verify Merkle root: `2ed91a62f96f11151cd9211cf90aff36efc16c69d3ef910f4201592095abdaca`
- [ ] Unit tests for all 17 entries

## Dependencies
Mission 0126-dcs-core-types
Mission 0126-dcs-composite-types

## Location
`determin/src/probe.rs` (add dcs_probe_tests module)

## Complexity
Medium — requires correct byte encoding for each entry type

## Implementation Notes
- Leaf hash: SHA256(0x00 || entry_data) per RFC 6962 domain separation
- Internal hash: SHA256(0x01 || left_hash || right_hash)
- Odd leaf count: duplicate last element for pairing
- Entry 10 Numeric TRAP: 24 bytes per RFC-0111 format [version:1=0x01][reserved:3][scale:1=0xFF][reserved:3][mantissa:16]
- Entry 14 BIGINT: little-endian limbs per RFC-0110
- Entry 15 DFP: class=0 (Normal) per RFC-0104 authoritative encoding
- Entry 16 Struct: fields serialize in declared order, NOT alphabetical

## Reference
- RFC-0126 §Verification Probe
- RFC-0126 §Probe Entry Details
- RFC-0126 §Merkle Root Computation
- RFC-6962 (Certificate Transparency) for domain separation
- scripts/compute_dcs_probe_root.py (Python reference)
