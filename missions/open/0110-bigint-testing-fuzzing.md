# Mission: BigInt Testing & Differential Fuzzing

## Status
Open

## RFC
RFC-0110 (Numeric): Deterministic BIGINT

## Summary
Implement comprehensive test vectors and differential fuzzing harness for BigInt against num-bigint reference.

## Acceptance Criteria
- [ ] 40+ test vectors covering boundary cases (per RFC table)
- [ ] i128 round-trip test vectors (entries 42-46)
- [ ] Boundary case tests: MAX_BIGINT, zero, negative zero, overflow
- [ ] Differential fuzzing harness against num-bigint (Rust)
- [ ] Run 100,000+ random input fuzzing cases
- [ ] All fuzzing cases must produce identical results to reference
- [ ] Gas calculation verification

## Location
`stoolap/src/numeric/bigint.rs`, `stoolap/fuzz/`

## Complexity
Medium

## Prerequisites
- Mission 0110-bigint-core-algorithms

## Implementation Notes
- Reference implementation: num-bigint crate
- Use proptest or rand for fuzzing
- Test vectors must include all RFC boundary cases
- Verify gas proof: worst-case 64-limb DIV + canonicalization ≤ 15,000 gas

## Reference
- RFC-0110: Deterministic BIGINT (§Test Vectors)
- RFC-0110: Deterministic BIGINT (§Gas Model)
