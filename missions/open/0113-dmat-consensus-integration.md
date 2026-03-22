# Mission: DMAT Consensus Integration

## Status
Open (unclaimed)

## RFC
RFC-0113 v1.21 (Numeric): Deterministic Matrices (DMAT)

## Summary
Integrate DMAT operations into the consensus layer with proper gas accounting, operation IDs, and enforcement of consensus restrictions.

## Acceptance Criteria
- [ ] Operation IDs defined: MAT_ADD=0x0100, MAT_SUB=0x0101, MAT_MUL=0x0102, MAT_VEC_MUL=0x0103, MAT_TRANSPOSE=0x0104, MAT_SCALE=0x0105
- [ ] Gas accounting per RFC-0113 §Gas Model:
  - [ ] MAT_ADD/SUB: `10 × M × N`
  - [ ] MAT_MUL: `M × N × K × (30 + 3 × s_a × s_b)`
  - [ ] MAT_VEC_MUL: `rows × cols × (30 + 3 × s_a × s_v)`
  - [ ] MAT_TRANSPOSE: `2 × M × N`
  - [ ] MAT_SCALE: `M × N × (20 + 3 × s_a × s_scalar)`
- [ ] DMAT<DFP> rejected at consensus boundary (FORBIDDEN type)
- [ ] Mixed-type operations rejected (NumericScalar trait)
- [ ] Scale validation at consensus boundary
- [ ] Dimension limit enforcement (M×N ≤ 64, M≤8, N≤8, M≥1, N≥1)

## Gas Formulas Reference
```
MAT_ADD/DECIMAL:     10 × M × N
MAT_SUB/DECIMAL:     10 × M × N
MAT_MUL/DECIMAL:     M × N × K × (30 + 3 × s_a × s_b)
MAT_VEC_MUL/DECIMAL: rows × cols × (30 + 3 × s_a × s_v)
MAT_TRANSPOSE:       2 × M × N
MAT_SCALE:           M × N × (20 + 3 × s_a × s_scalar)
```

## Dependencies
- Mission 0113-dmat-core-type
- Mission 0113-dmat-add-sub
- Mission 0113-dmat-matrix-multiplication
- Mission 0113-dmat-matrix-vector-multiplication
- Mission 0113-dmat-transpose-scale
- RFC-0113 §Gas Model
- RFC-0113 §Production Limitations

## Location
`determin/src/consensus.rs` (add DMAT gas functions)

## Complexity
Medium — gas model formulas are straightforward

## Reference
- RFC-0113 §Gas Model
- RFC-0113 §Production Limitations
- RFC-0113 §Determinism Rules
