# Mission: BigInt Core Algorithms

## Status
Open

## RFC
RFC-0110 (Numeric): Deterministic BIGINT

## Summary
Implement core BigInt arithmetic algorithms: ADD, SUB, MUL, DIV, MOD, CMP, SHL, SHR with full deterministic specification.

## Acceptance Criteria
- [ ] BigInt struct: Vec<u64> limbs + sign: bool
- [ ] ADD: signed addition with canonicalization
- [ ] SUB: signed subtraction with canonicalization
- [ ] MUL: schoolbook O(n²) multiplication with canonicalization
- [ ] DIV: binary long division (Knuth Algorithm D) with canonicalization
- [ ] MOD: remainder operation using divmod
- [ ] CMP: three-way comparison returning -1, 0, or 1
- [ ] SHL: left shift with overflow TRAP
- [ ] SHR: right shift preserving sign
- [ ] All operations enforce MAX_BIGINT_BITS (4096 bits)
- [ ] All operations call canonicalize before returning
- [ ] Determinism: same inputs → same outputs across implementations

## Location
`stoolap/src/numeric/bigint.rs`

## Complexity
High

## Prerequisites
None

## Implementation Notes
- Follow RFC-0110 algorithms exactly (no Karatsuba, no SIMD)
- Use u128 intermediate arithmetic for carry/borrow
- Division must use exactly `a_norm.limbs.len()` outer iterations (no early exit)
- Post-operation canonicalization is MANDATORY
- Implement TRAP on overflow (result exceeds MAX_BIGINT_BITS)

## Reference
- RFC-0110: Deterministic BIGINT (§Algorithms)
- RFC-0110: Deterministic BIGINT (§Determinism Rules)
