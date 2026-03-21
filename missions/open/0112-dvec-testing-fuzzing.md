# Mission: DVEC Testing and Fuzzing

## Status
Open (unclaimed)

## RFC
RFC-0112 v1.14 (Numeric): Deterministic Vectors (DVEC)

## Summary
Comprehensive test suite and fuzzing for all DVEC operations. Verify determinism invariants, edge cases, and gas model compliance.

## Acceptance Criteria
- [ ] Unit tests for all operations (DOT_PRODUCT, SQUARED_DISTANCE, NORM, NORMALIZE, VEC_ADD/SUB/MUL/SCALE)
- [ ] Test all probe entries (57 entries with known expected results)
- [ ] Boundary tests: N=64 (max), N=65 (should TRAP), zero vectors
- [ ] Scale mismatch tests (should TRAP with SCALE_MISMATCH)
- [ ] Dimension mismatch tests (should TRAP)
- [ ] Overflow tests (BigInt accumulator → i64/i128 conversion)
- [ ] DQA NORM TRAP test (UNSUPPORTED_OPERATION)
- [ ] NORMALIZE zero vector TRAP (CANNOT_NORMALIZE_ZERO_VECTOR)
- [ ] NORMALIZE consensus restriction test (CONSENSUS_RESTRICTION)
- [ ] Zero vector NORM returns zero (not error)
- [ ] Fuzz tests: random vectors, random scales, many iterations
- [ ] Cross-impl determinism: Rust vs Python reference (compute_dvec_probe_root.py)
- [ ] All tests pass: 100+ tests expected

## Test Vector Categories
1. **DOT_PRODUCT**: N=1..64, scales 0..18 (DQA), 0..36 (Decimal), overflow cases
2. **SQUARED_DISTANCE**: same coverage, scale doubling (×2)
3. **NORM**: Decimal only, zero vector, various scales, DQA TRAP
4. **NORMALIZE**: zero vector TRAP, consensus restriction
5. **Element-wise**: dimension mismatch, scale mismatch, scalar multiplication
6. **TRAP entries**: 52-56 verify all TRAP types

## Dependencies
- Mission 0112-dvec-core-type
- Mission 0112-dvec-arithmetic-dot-product-squared-distance
- Mission 0112-dvec-norm-normalize
- Mission 0112-dvec-element-wise-operations
- Mission 0112-dvec-verification-probe (for probe entry verification)
- RFC-0112 §Test Vectors
- RFC-0112 §Probe Entry Details

## Location
`determin/src/dvec.rs` (unit tests), `determin/tests/` (integration tests)

## Complexity
Medium — mostly test coverage, fuzzing is straightforward for deterministic ops

## Reference
- RFC-0112 §Test Vectors
- RFC-0112 §Boundary Cases
- RFC-0112 §Probe Entry Details
- RFC-0112 §Determinism Rules
