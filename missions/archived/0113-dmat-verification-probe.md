# Mission: DMAT Verification Probe

## Status
Completed (2026-03-22)

## RFC
RFC-0113 v1.21 (Numeric): Deterministic Matrices (DMAT)

## Summary
Implemented 64-entry DMAT verification probe with full Merkle tree root verification matching Python reference `compute_dmat_probe_root.py`.

## Acceptance Criteria
- [x] 64 probe entries serialized in canonical format
- [x] `type_id` byte: `1` = DQA, `2` = Decimal
- [x] 24-byte scalar encoding per RFC-0105/0111
- [x] TRAP sentinel: `{mantissa: -(1 << 63), scale: 0xFF}`
- [x] Operation IDs: MAT_ADD=0x0100, MAT_SUB=0x0101, MAT_MUL=0x0102, MAT_VEC_MUL=0x0103, MAT_TRANSPOSE=0x0104, MAT_SCALE=0x0105
- [x] Merkle tree construction from 64 leaf hashes
- [x] Root matches canonical value: `045cf8d1f50e5e67be8d8e63a76be93a40cfc383289a68b8aa585c7244a86b31`

## Implementation Details
- Added DMAT probe constants and encoding functions to `probe.rs`
- `DmatProbeOperand` with `is_vector` flag for proper encoding
- `DmatProbeEntry` and `DmatProbeResult` types
- `dmat_dqa_encode`: 24-byte DQA encoding (version + scale + mantissa as i128)
- `dmat_encode_matrix`: rows(1) || cols(1) || elements...
- `dmat_encode_vector`: len(1) || 1(1) || elements...
- `dmat_encode_scalar`: 1(1) || 1(1) || dqa_encode (for MAT_SCALE)
- `dmat_entry_hash`: SHA256 of op_id || type_id || a_data || b_data || c_data
- `dmat_build_merkle_tree`: standard pair-hash Merkle tree construction

## Test Results
- 1 new DMAT probe test (64 entries verified against Python reference)
- 354 total tests pass
- Clippy clean
- Cargo fmt applied

## Dependencies
- Mission 0113-dmat-core-type (completed)
- Mission 0113-dmat-add-sub (completed)
- Mission 0113-dmat-matrix-multiplication (completed)
- Mission 0113-dmat-matrix-vector-multiplication (completed)
- Mission 0113-dmat-transpose-scale (completed)
- `scripts/compute_dmat_probe_root.py` (reference implementation)

## Location
`determin/src/probe.rs` (DMAT probe section after DVEC probe at line 2917)

## Complexity
High — Merkle tree construction, complex serialization, 64 entries must match Python reference exactly

## Reference
- RFC-0113 §Verification Probe
- RFC-0113 §Probe Entry Serialization Format
- RFC-0113 §Published Merkle Root
- RFC-0113 §Probe Entry Details
- `scripts/compute_dmat_probe_root.py`
