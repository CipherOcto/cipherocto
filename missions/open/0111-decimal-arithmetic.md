# Mission: DECIMAL Arithmetic Operations

## Status
Open

## RFC
RFC-0111 (Numeric): Deterministic DECIMAL

## Summary
Implement DECIMAL arithmetic operations: ADD, SUB, MUL, DIV, SQRT, ROUND, CMP with all deterministic algorithms and i128 intermediate arithmetic.

## Acceptance Criteria
- [ ] ADD: scale alignment with checked_mul(i256), overflow check
- [ ] SUB: scale alignment with checked_mul(i256), overflow check
- [ ] MUL: i128 × i128 → i256 intermediate, RoundHalfEven for negative products
- [ ] DIV: scale adjustment + i256 division, remainder for rounding
- [ ] SQRT: Newton-Raphson with exactly 40 iterations (no early exit)
- [ ] ROUND: RoundHalfEven targeting specified scale
- [ ] CMP: i256 comparison for magnitude, sign-aware result
- [ ] All operations use i128 intermediate arithmetic (not i256 fast path)
- [ ] All operations call canonicalize() before returning
- [ ] Precision Growth Control: scale_result ≤ min(36, max(scale_a, scale_b) + 6)
- [ ] 57 probe entries with SHA256 leaf hashes verified

## Dependencies
- Mission 0111-decimal-core-type (must complete first)

## Location
`determin/src/decimal.rs`

## Complexity
High — SQRT Newton-Raphson and DIV are most complex

## Reference
- RFC-0111 §ADD, §SUB, §MUL, §DIV, §SQRT, §ROUND, §CMP
- RFC-0111 §Precision Growth Control
- RFC-0111 §Determinism Rules (fixed iterations, no SIMD, i128 intermediate)
