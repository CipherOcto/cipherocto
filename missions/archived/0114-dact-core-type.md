# Mission: DACT Core Type (ReLU, ReLU6, LeakyReLU)

## Status
Completed (2026-03-22)

## RFC
RFC-0114 v2.12 (Numeric): Deterministic Activation Functions (DACT)

## Summary
Implement the core DACT type with ReLU, ReLU6, and LeakyReLU activation functions. These are exact operations requiring no LUT storage.

## Acceptance Criteria
- [ ] `relu(x: Dqa) -> Dqa` — return x if x >= 0, else 0; Gas: 2
- [ ] `relu6(x: Dqa) -> Dqa` — return clamp(x, 0, 6); Gas: 3
- [ ] `leaky_relu(x: Dqa, alpha: Dqa = Dqa(1, 2)) -> Dqa` — return x if x >= 0, else x * alpha; Gas: 3
- [ ] TRAP sentinel detection (RFC-0105 canonical encoding)
- [ ] Scale validation (scale > 18 → TRAP)
- [ ] All returns canonicalized per RFC-0105

## Dependencies
None — foundational mission

## Location
`determin/src/dact.rs` (new file)

## Complexity
Low — simple comparisons and conditional returns

## Implementation Notes
- ReLU/ReLU6 skip canonicalization/scale normalization (element-wise direct comparison)
- LeakyReLU uses RFC-0105 `multiply` internally (canonicalizes result)
- alpha is PROTOCOL CONSTANT: Dqa(1, 2) = 0.01 — callers MUST NOT supply custom values
- max_val for ReLU6: `Dqa(6 * 10^x.scale, x.scale)` — internal comparison value

## Reference
- RFC-0114 §ReLU
- RFC-0114 §ReLU6
- RFC-0114 §LeakyReLU
- RFC-0114 §TRAP Invariant
- RFC-0114 §Phase Ordering
