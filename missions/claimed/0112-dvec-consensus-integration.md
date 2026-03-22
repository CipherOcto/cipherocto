# Mission: DVEC Consensus Integration

## Status
Completed (2026-03-21)

## RFC
RFC-0112 v1.14 (Numeric): Deterministic Vectors (DVEC)

## Summary
DVEC consensus integration layer implemented in `determin/src/consensus.rs` with gas accounting per RFC-0112 §Gas Model, operation IDs, and consensus restriction enforcement.

## Acceptance Criteria
- [x] Operation IDs assigned for all DVEC operations — defined in `consensus.rs::op_ids` (matching probe.rs)
- [x] Gas accounting per RFC-0112 §Gas Model:
  - DOT_PRODUCT: N × (30 + 3 × scale²) — max 17,472 for N=64, scale=9
  - SQUARED_DISTANCE: N × (30 + 3 × scale²) + 10 — max 17,482
  - NORM: DOT_PRODUCT + 280 (GAS_SQRT) — max ~17,752
  - NORMALIZE: FORBIDDEN in consensus (TRAP)
  - VEC_ADD/SUB/MUL/SCALE: 5 × N — max 320
- [x] NORMALIZE returns TRAP(CONSENSUS_RESTRICTION) in consensus context — already in dvec.rs
- [x] DVEC<Dfp> rejected at consensus boundary (type system: no DvecScalar impl for Dfp)
- [x] Mixed-type operations rejected (DvecScalar trait prevents mixing DQA/Decimal)
- [x] Scale validation at consensus boundary — already in operations (InputScaleExceeded)
- [x] Dimension limit enforcement (N <= 64) — already in validate_max_dim
- [x] NUMERIC_SPEC_VERSION — not applicable (DVEC is RFC-0112, separate from RFC-0105 numeric types)

## Gas Formulas Implemented
```rust
gas_dot_product(n, scale)     = n * (30 + 3 * scale²)
gas_squared_distance(n, scale) = n * (30 + 3 * scale²) + 10
gas_norm(n, scale)            = gas_dot_product(n, scale) + 280
gas_element_wise(n)           = 5 * n
gas_normalize(n)              = None (FORBIDDEN)
```

## Implementation Details
- `consensus.rs` module with gas calculation functions and constants
- `op_ids` submodule with operation ID constants (matching probe.rs)
- `is_allowed_in_consensus(op_id)` helper to check if operation is permitted
- `MAX_DVEC_GAS = 17_752` constant for maximum operation cost

## Test Results
- 289 tests pass (278 pre-existing + 11 new consensus tests)
- Gas calculation tests verify formulas match RFC specification
- `test_normalize_returns_consensus_restriction` confirms NORMALIZE is blocked

## Dependencies
- Mission 0112-dvec-core-type (completed)
- Mission 0112-dvec-arithmetic-dot-product-squared-distance (completed)
- Mission 0112-dvec-norm-normalize (completed)
- Mission 0112-dvec-element-wise-operations (completed)
- RFC-0112 §Gas Model
- RFC-0112 §Production Limitations
- RFC-0112 §CROSS-2 Note

## Location
`determin/src/consensus.rs` (new file)
`determin/src/lib.rs` (updated to include consensus module)

## Complexity
Low — gas formulas are straightforward calculations, consensus restrictions already implemented

## Reference
- RFC-0112 §Gas Model
- RFC-0112 §Production Limitations
- RFC-0112 §NORMALIZE
- RFC-0112 §Determinism Rules
