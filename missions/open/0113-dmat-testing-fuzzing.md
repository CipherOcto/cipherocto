# Mission: DMAT Testing and Fuzzing

## Status
Open (unclaimed)

## RFC
RFC-0113 v1.21 (Numeric): Deterministic Matrices (DMAT)

## Summary
Comprehensive test suite for all DMAT operations covering edge cases, boundary conditions, and TRAP scenarios.

## Acceptance Criteria
- [ ] Unit tests for all operations (MAT_ADD, MAT_SUB, MAT_MUL, MAT_VEC_MUL, MAT_TRANSPOSE, MAT_SCALE)
- [ ] Test all probe entries (64 entries with known expected results) — via probe module
- [ ] Boundary tests: M×N=64 (max), M×N=65 (should TRAP), M=9 or N=9 (should TRAP)
- [ ] Empty matrix tests (M=0 or N=0 should TRAP per CRIT-NEW-1)
- [ ] Scale mismatch tests (should TRAP with SCALE_MISMATCH)
- [ ] Cross-matrix scale mismatch tests for ADD/SUB (should TRAP)
- [ ] Dimension mismatch tests (should TRAP)
- [ ] Overflow tests (accumulator > MAX_MANTISSA)
- [ ] Invalid scale tests (result_scale > MAX_SCALE)
- [ ] TRAP sentinel detection tests
- [ ] Fuzz tests for all operations
- [ ] Cross-impl determinism: Rust vs Python reference — via probe module

## Dependencies
- Mission 0113-dmat-core-type
- Mission 0113-dmat-add-sub
- Mission 0113-dmat-matrix-multiplication
- Mission 0113-dmat-matrix-vector-multiplication
- Mission 0113-dmat-transpose-scale
- Mission 0113-dmat-verification-probe (for probe entry verification)
- RFC-0113 §Test Vectors
- RFC-0113 §Boundary Cases
- RFC-0113 §Probe Entry Details

## Location
`determin/src/dmat.rs` (unit tests), `determin/src/probe.rs` (probe tests)

## Complexity
Medium — mostly test coverage, fuzzing required

## Reference
- RFC-0113 §Test Vectors
- RFC-0113 §Boundary Cases
- RFC-0113 §Probe Entry Details
