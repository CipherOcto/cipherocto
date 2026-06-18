# Mission: DVEC Verification Probe

## Status
Completed (2026-03-21)

## RFC
RFC-0112 v1.14 (Numeric): Deterministic Vectors (DVEC)

## Summary
Implemented 57-entry DVEC verification probe with full Merkle tree root verification. All entries verified against Python reference implementation (`compute_dvec_probe_root.py`). Root matches canonical value: `74a4c3b44b88bae483ae24b26d04980868a0cc26772b06fe2029c328c1118998`.

## Acceptance Criteria
- [x] 57 probe entries serialized in canonical format
- [x] `type_id` byte: `1` = DQA, `2` = Decimal
- [x] 24-byte scalar encoding (version + reserved + scale + reserved + mantissa big-endian)
- [x] DQA mantissa stored in bytes 16-23 (last 8 bytes of 16-byte mantissa slot)
- [x] TRAP sentinel: `{mantissa: 0x8000000000000000, scale: 0xFF}`
- [x] Merkle tree construction from 57 leaf hashes
- [x] Root matches canonical value: `74a4c3b44b88bae483ae24b26d04980868a0cc26772b06fe2029c328c1118998`
- [x] `compute_dvec_probe_root.py` script produces same root

## Bug Fixes During Implementation
1. **Entry 15 (DOT_PRODUCT_DQA_15)**: Expected `(220, 1)` → `(22, 1)` (canonicalization of 220 with scale 1 gives scale 0, mantissa 22)
2. **Entry 45 (NORM_45)**: Expected `(10000000000, 10)` → `(1, 4)` (RFC-0111 sqrt of (1,8) correctly returns (1,4))
3. **Entry 54 (TRAP_OVERFLOW)**: Changed from `i128::MAX / 2` to `10^18` per Python reference
4. **DQA encoding**: Fixed to use bytes 16-23 for mantissa (not 8-15)
5. **op_id endianness**: Fixed to use big-endian (not little-endian)

## Probe Entry Distribution
- Entries 0-15: DOT_PRODUCT DQA (various N, scales)
- Entries 16-31: DOT_PRODUCT Decimal (various N, scales)
- Entries 32-39: SQUARED_DISTANCE (DQA/Decimal)
- Entries 40-47: NORM (Decimal + DQA TRAPs)
- Entries 48-51: Element-wise Decimal ADD/SUB/MUL/SCALE
- Entries 52-56: TRAP cases (overflow, scale, dimension, consensus)

## Test Results
- 253 tests pass (244 pre-existing + 9 new DVEC probe tests)
- `test_merkle_root` — verifies root matches canonical value
- `test_all_entry_hashes_vs_python` — verifies all 57 entries match Python hashes
- `test_dvec_encode_dqa` — DQA scalar encoding verification
- `test_dvec_encode_trap` — TRAP sentinel encoding verification
- `test_dvec_make_entry` — entry construction verification
- `test_dvec_entry_hash` — single entry hash verification
- `test_encode_vector` — vector encoding verification
- `test_all_57_entries` — entry count verification

## Dependencies
- Mission 0112-dvec-core-type (completed)
- Mission 0112-dvec-arithmetic-dot-product-squared-distance (completed)
- Mission 0112-dvec-norm-normalize (completed)
- Mission 0112-dvec-element-wise-operations (completed)
- `scripts/compute_dvec_probe_root.py` (reference implementation)
- RFC-0112 §Verification Probe
- RFC-0112 §Probe Entry Details
- RFC-0111 v1.20 §TRAP Sentinel

## Location
`determin/src/probe.rs` (DVEC probe section, lines ~2066-2440)

## Complexity
High — Merkle tree construction, complex serialization, all 57 entries must match Python reference

## Reference
- RFC-0112 §Verification Probe
- RFC-0112 §Probe Entry Serialization Format
- RFC-0112 §Published Merkle Root
- RFC-0112 §Probe Entry Details (all 57 entries)
- `scripts/compute_dvec_probe_root.py`
