# Mission: DVEC Verification Probe

## Status
Open (unclaimed)

## RFC
RFC-0112 v1.14 (Numeric): Deterministic Vectors (DVEC)

## Summary
Implement the 57-entry DVEC verification probe with Merkle tree root verification. The probe verifies all DVEC operations (DOT_PRODUCT, SQUARED_DISTANCE, NORM, NORMALIZE, VEC_ADD/SUB/MUL/SCALE) and their TRAP cases.

## Acceptance Criteria
- [ ] 57 probe entries serialized in canonical format
- [ ] `type_id` byte: `1` = DQA, `2` = Decimal
- [ ] 24-byte scalar encoding (version + reserved + scale + reserved + mantissa big-endian)
- [ ] DQA sign-extension for 64-bit → 128-bit slot
- [ ] TRAP sentinel: `{mantissa: 0x8000000000000000, scale: 0xFF}`
- [ ] Merkle tree construction from 57 leaf hashes
- [ ] Root matches canonical value: `74a4c3b44b88bae483ae24b26d04980868a0cc26772b06fe2029c328c1118998`
- [ ] `compute_dvec_probe_root.py` script produces same root

## Probe Entry Distribution
- Entries 0-15: DOT_PRODUCT DQA (various N, scales)
- Entries 16-31: DOT_PRODUCT Decimal (various N, scales)
- Entries 32-39: SQUARED_DISTANCE (DQA/Decimal)
- Entries 40-47: NORM (Decimal + DQA TRAPs)
- Entries 48-51: Element-wise Decimal ADD/SUB/MUL/SCALE
- Entries 52-56: TRAP cases (overflow, scale, dimension, consensus)

## Entry Details to Implement
All 57 entries with correct expected values per RFC table:
- DOT_PRODUCT entries: scale constraints, BigInt accumulation, canonicalization
- SQUARED_DISTANCE entries: diff² accumulation, scale doubling
- NORM entries: sqrt of dot_product, zero vector special case
- NORMALIZE TRAP: CONSENSUS_RESTRICTION
- TRAP entries: DIMENSION, INPUT_VALIDATION_ERROR, OVERFLOW, INPUT_SCALE

## Dependencies
- Mission 0112-dvec-core-type
- Mission 0112-dvec-arithmetic-dot-product-squared-distance
- Mission 0112-dvec-norm-normalize
- Mission 0112-dvec-element-wise-operations
- `scripts/compute_dvec_probe_root.py` (reference implementation)
- RFC-0112 §Verification Probe
- RFC-0112 §Probe Entry Details
- RFC-0111 v1.20 §TRAP Sentinel

## Location
`determin/src/probe.rs` (DVEC probe section, alongside existing BIGINT/DECIMAL probes)

## Complexity
High — Merkle tree construction, complex serialization, all 57 entries must match

## Reference
- RFC-0112 §Verification Probe
- RFC-0112 §Probe Entry Serialization Format
- RFC-0112 §Published Merkle Root
- RFC-0112 §Probe Entry Details (all 57 entries)
- `scripts/compute_dvec_probe_root.py`
