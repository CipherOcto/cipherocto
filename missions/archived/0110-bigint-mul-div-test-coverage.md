# Mission: BigInt MUL/DIV Test Coverage Gap

## Status
Completed (2026-03-21)

## RFC
RFC-0110 (Numeric): Deterministic BIGINT

## Summary
Add missing test cases for MUL overflow (post-check TRAP) and DIV iteration count verification per RFC-0110 v2.13 changes.

## Background

RFC-0110 v2.13 changed the MUL overflow check from pre-check to post-check:
- **Old (v2.12)**: Pre-check inputs, reject if `a.bits() > MAX_BIGINT_BITS || b.bits() > MAX_BIGINT_BITS`
- **New (v2.13)**: Pre-check inputs, then post-check result: `if result.bits() > MAX_BIGINT_BITS: TRAP`

Additionally, the DIV iteration count was clarified: use `m+1` where `m = dividend.len() - divisor.len()`, with the `j=0` special case removed.

## Missing Test Cases

### MUL Overflow (Post-Check)

**Gap:** No test verifies that MUL correctly TRAPs when inputs are valid but product exceeds MAX_BIGINT_BITS.

**Required test:**
```rust
/// MUL that overflows: two ~2049-bit numbers multiply to ~4098 bits > MAX_BIGINT_BITS
/// - a = 2^2048 (one limb at bit 2048)
/// - b = 2^2048
/// - Both individually are within MAX_BIGINT_BITS (4096)
/// - Product = 2^4096 > MAX_BIGINT_BITS → MUST TRAP
#[test]
fn bug2_mul_overflow_at_max_boundary() {
    let a = BigInt::new(vec![0, 0, 1], false); // 2^128 (example, need 2^2048)
    // Actually need limbs to reach 2048 bits...
    // Find two numbers where: a.bits() <= 4096, b.bits() <= 4096, but (a*b).bits() > 4096
}
```

**Strategy:** Find two maximum-sized inputs whose product exceeds 4096 bits:
- Each input can be at most 4096 bits
- Two 2049-bit numbers: a = 2^2049 - 1, b = 2^2049 - 1
- Product bits = ~4098 > 4096 → TRAP

### DIV Iteration Count

**Gap:** No test explicitly verifies DIV executes correct number of iterations.

**Required test:** Verify that division with the j=0 case removed produces correct results for edge cases:
- Single-limb dividend / single-limb divisor (the removed special case)
- Cases where old j=0 logic might have differed from new logic

## Acceptance Criteria

- [ ] Add `bug2_mul_overflow_at_max_boundary` test: two ~2049-bit numbers whose product exceeds MAX_BIGINT_BITS → Err(Overflow)
- [ ] Add `bug2_mul_exactly_at_max_boundary` test: two 2048-bit numbers whose product equals exactly 4096 bits → OK
- [ ] Add `bug3_div_single_limb_result` test: verify j=0 removal doesn't affect single-limb quotient results
- [ ] All tests pass with `cargo test --release -p determin`

## Location
`determin/src/bigint.rs` - add tests in existing test module

## Complexity
Low — additions only, no algorithm changes

## Prerequisites
- RFC-0110 v2.13 understanding
- BigInt arithmetic basics

## Reference
- RFC-0110 v2.13 commit: `b34bd45`
- RFC-0110 MUL algorithm: lines 500-543
- RFC-0110 DIV algorithm: lines 545+

## Claimant
@claude-code (AI Agent)
