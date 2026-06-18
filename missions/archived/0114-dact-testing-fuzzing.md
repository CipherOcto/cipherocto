# Mission: DACT Testing and Fuzzing

## Status
Archived

## RFC
RFC-0114 v2.12 (Numeric): Deterministic Activation Functions (DACT)

## Summary
Comprehensive testing and fuzzing for DACT activation functions including property-based tests, edge cases, and differential fuzzing against reference implementations.

## Acceptance Criteria
- [ ] Unit tests for ReLU:
  - [ ] Positive input returns same value
  - [ ] Negative input returns zero
  - [ ] Zero input returns zero
  - [ ] TRAP input returns TRAP
  - [ ] Scale validation (scale > 18 → TRAP)
- [ ] Unit tests for ReLU6:
  - [ ] Input < 0 returns zero
  - [ ] Input > 6 returns 6
  - [ ] Input in [0, 6] returns input
  - [ ] TRAP input returns TRAP
- [ ] Unit tests for LeakyReLU:
  - [ ] Positive input returns same value
  - [ ] Negative input returns x * 0.01
  - [ ] TRAP input returns TRAP
- [ ] Unit tests for Sigmoid:
  - [ ] sigmoid(-4.0) ≈ 0.0195
  - [ ] sigmoid(0.0) = 0.5
  - [ ] sigmoid(4.0) ≈ 0.9804
  - [ ] sigmoid(< -4.0) = 0
  - [ ] sigmoid(> 4.0) = 1
- [ ] Unit tests for Tanh:
  - [ ] tanh(-4.0) ≈ -1.0
  - [ ] tanh(0.0) = 0
  - [ ] tanh(4.0) ≈ 1.0
  - [ ] tanh(< -4.0) = -1
  - [ ] tanh(> 4.0) = 1
- [ ] LUT correctness tests:
  - [ ] Verify SIGMOID_LUT_V2_SHA256
  - [ ] Verify TANH_LUT_V2_SHA256
  - [ ] Index 200 (x=-2.00): sigmoid Q8.8 = 31, tanh Q8.8 = -247
  - [ ] Index 400 (x=0.00): sigmoid Q8.8 = 128, tanh Q8.8 = 0
  - [ ] Index 600 (x=2.00): sigmoid Q8.8 = 225, tanh Q8.8 = 247
- [ ] normalize_to_scale tests:
  - [ ] Dqa(1234, 2) → Dqa(12340, 3) for scale 3
  - [ ] Dqa(25000, 4) → Dqa(25, 2) for scale 2 (downscale positive)
  - [ ] Dqa(-153, 3) → Dqa(-16, 2) for scale 2 (downscale negative, floor)
- [ ] Fuzz tests (10,000 iterations each):
  - [ ] ReLU fuzz
  - [ ] ReLU6 fuzz
  - [ ] LeakyReLU fuzz
  - [ ] Sigmoid fuzz
  - [ ] Tanh fuzz
- [ ] All probe tests pass (16 entries)
- [ ] Clippy clean, cargo fmt applied

## Dependencies
- Mission 0114-dact-core-type (completed)
- Mission 0114-dact-sigmoid-tanh-lut (completed)
- Mission 0114-dact-verification-probe (completed)

## Location
`determin/src/dact.rs`, `determin/src/fuzz.rs`

## Complexity
Medium — comprehensive test coverage

## Implementation Notes
- Use rand::rngs::StdRng with seed_from_u64(42) for reproducible fuzzing
- TRAP test: create TRAP sentinel via `Dqa::new(i64::MIN, 0xFF)` or similar
- Q8.8 LUT values should be tested directly via q88_to_dqa
- Error bounds: Sigmoid/Tanh max error ≤ 0.004 in domain, ≤ 0.035 at clamp boundaries

## Reference
- RFC-0114 §Error Bounds
- RFC-0114 §Domain Handling
- RFC-0114 §LUT Values
- RFC-0114 §Implementation Checklist
