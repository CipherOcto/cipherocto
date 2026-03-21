# Mission: DVEC Consensus Integration

## Status
Open (unclaimed)

## RFC
RFC-0112 v1.14 (Numeric): Deterministic Vectors (DVEC)

## Summary
Integrate DVEC operations into the consensus layer with proper gas accounting, operation IDs, and enforcement of consensus restrictions (NORMALIZE forbidden in consensus).

## Acceptance Criteria
- [ ] Operation IDs assigned for all DVEC operations
- [ ] Gas accounting per RFC-0112 §Gas Model:
  - DOT_PRODUCT: N × (30 + 3 × scale²) — max 17,472 for N=64, scale=9
  - SQUARED_DISTANCE: N × (30 + 3 × scale²) + 10 — max 17,482
  - NORM: DOT_PRODUCT + GAS_SQRT — max ~17,752
  - NORMALIZE: FORBIDDEN in consensus (TRAP)
  - VEC_ADD/SUB/MUL/SCALE: 5 × N — max 320
- [ ] NORMALIZE returns TRAP(CONSENSUS_RESTRICTION) in consensus context
- [ ] DVEC<Dfp> rejected at consensus boundary (FORBIDDEN type)
- [ ] Mixed-type operations rejected (DQA vs Decimal)
- [ ] Scale validation at consensus boundary
- [ ] Dimension limit enforcement (N <= 64)
- [ ] NUMERIC_SPEC_VERSION incremented if needed (per RFC-0110)

## Gas Formulas Reference
```
DOT_PRODUCT_DQA(N, scale)     = N × (30 + 3 × scale²)
DOT_PRODUCT_Decimal(N, scale) = N × (30 + 3 × scale²)
SQUARED_DISTANCE_DQA(N, scale) = N × (30 + 3 × scale²) + 10
SQUARED_DISTANCE_Decimal(N, scale) = N × (30 + 3 × scale²) + 10
NORM_Decimal(N, scale)         = DOT_PRODUCT + 280 (GAS_SQRT max)
VEC_ADD/SUB/MUL/SCALE          = 5 × N
NORMALIZE                       = FORBIDDEN (TRAP)
```

## CROSS-2 Note
DOT_PRODUCT enforces `input_scale <= 9` for DQA at Phase 1 (INPUT_VALIDATION_ERROR), while DMAT's MAT_VEC_MUL does not enforce this precondition — it relies on Phase 4's `result_scale > MAX_SCALE` check. Same logical inputs may produce different TRAP codes depending on the operation path. Document this discrepancy.

## Dependencies
- Mission 0112-dvec-core-type
- Mission 0112-dvec-arithmetic-dot-product-squared-distance
- Mission 0112-dvec-norm-normalize
- Mission 0112-dvec-element-wise-operations
- RFC-0112 §Gas Model
- RFC-0112 §Production Limitations
- RFC-0112 §CROSS-2 Note

## Location
`determin/src/consensus/` (consensus integration layer)

## Complexity
Medium — gas model formulas are straightforward, consensus restriction enforcement is simple

## Reference
- RFC-0112 §Gas Model
- RFC-0112 §Production Limitations
- RFC-0112 §NORMALIZE
- RFC-0112 §Determinism Rules
