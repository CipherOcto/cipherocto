# Mission: DMAT Verification Probe

## Status
Open (unclaimed)

## RFC
RFC-0113 v1.21 (Numeric): Deterministic Matrices (DMAT)

## Summary
Implement 64-entry DMAT verification probe with full Merkle tree root verification. Reference script: `scripts/compute_dmat_probe_root.py`.

## Acceptance Criteria
- [ ] 64 probe entries serialized in canonical format
- [ ] `type_id` byte: `1` = DQA, `2` = Decimal
- [ ] 24-byte scalar encoding per RFC-0105/0111
- [ ] TRAP sentinel: `{mantissa: -(1 << 63), scale: 0xFF}`
- [ ] Operation IDs: MAT_ADD=0x0100, MAT_SUB=0x0101, MAT_MUL=0x0102, MAT_VEC_MUL=0x0103, MAT_TRANSPOSE=0x0104, MAT_SCALE=0x0105
- [ ] Merkle tree construction from 64 leaf hashes
- [ ] Root matches canonical value: `045cf8d1f50e5e67be8d8e63a76be93a40cfc383289a68b8aa585c7244a86b31`
- [ ] Python script `compute_dmat_probe_root.py` produces same root

## Wire Format
```
leaf_input = op_id (8 bytes) || type_id (1 byte) ||
a_rows (1 byte) || a_cols (1 byte) || a_elements... ||
b_rows (1 byte) || b_cols (1 byte) || b_elements... ||
result_rows (1 byte) || result_cols (1 byte) || result_elements...
```

## Dependencies
- Mission 0113-dmat-matrix-multiplication
- Mission 0113-dmat-matrix-vector-multiplication
- Mission 0113-dmat-transpose-scale
- `scripts/compute_dmat_probe_root.py` (reference implementation)
- RFC-0113 §Verification Probe
- RFC-0113 §Probe Entry Details

## Location
`determin/src/probe.rs` (DMAT probe section)

## Complexity
High — Merkle tree construction, complex serialization, 64 entries must match Python reference

## Reference
- RFC-0113 §Verification Probe
- RFC-0113 §Probe Entry Serialization Format
- RFC-0113 §Published Merkle Root
- RFC-0113 §Probe Entry Details
