# Mission: DACT Sigmoid/Tanh LUT and Q8.8 Conversion

## Status
Completed (2026-03-22)

## RFC
RFC-0114 v2.12 (Numeric): Deterministic Activation Functions (DACT)

## Summary
Implement Sigmoid and Tanh activation functions with LUT-based lookup and Q8.8→DQA conversion. Includes 801-entry LUT generation, floor division conversion, and normalize_to_scale helper.

## Acceptance Criteria
- [ ] LUT storage: 801 entries each for SIGMOID and TANH (Q8.8 signed i16, big-endian)
- [ ] LUT commitment verification: SHA-256 matches RFC-0114 canonical values
  - SIGMOID_LUT_V2_SHA256 = "7af8a570e86bf433bc558d66473b2460663d3be98c85f258e98dc93dc3aff5df"
  - TANH_LUT_V2_SHA256 = "dc92c87e65f8fe3b0070daa09d0d5a8a97b15b39e5f6040e280052605389b379"
- [ ] `normalize_to_scale(x: Dqa, target_scale: u8) -> Dqa` helper
- [ ] `q88_to_dqa(q: i16, target_scale: u8) -> (i64, u8)` — floor division conversion
- [ ] `sigmoid(x: Dqa) -> Dqa` — normalize_to_scale(x, 2), clamp, index, LUT lookup, convert; Gas: 10
- [ ] `tanh(x: Dqa) -> Dqa` — normalize_to_scale(x, 2), clamp, index, LUT lookup, convert; Gas: 10
- [ ] All returns canonicalized per RFC-0105

## Dependencies
- Mission 0114-dact-core-type (completed)

## Location
`determin/src/dact.rs`

## Complexity
Medium — LUT storage and conversion logic

## Implementation Notes
- Domain: x ∈ [-4.00, 4.00], step 0.01, index formula: idx = x_int + 400
- Clamp: x_norm.value < -400 → Dqa(0, 4) or Dqa(-10000, 4); x_norm.value > 400 → Dqa(10000, 4)
- Q8.8→DQA: `(q * 10^4) // 256` with floor division (Python // semantics)
- normalize_to_scale does NOT canonicalize — it only adjusts mantissa alignment
- Phase ordering: TRAP → normalize → clamp → index → lookup → convert → canonicalize

## Reference
- RFC-0114 §Sigmoid
- RFC-0114 §Tanh
- RFC-0114 §Q8.8 → DQA Conversion
- RFC-0114 §normalize_to_scale Helper
- RFC-0114 §LUT Specification
- RFC-0114 §SHA-256 Commitments
