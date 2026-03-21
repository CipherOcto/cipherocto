# Mission: DVEC Core Type and Trait Definitions

## Status
Open (unclaimed)

## RFC
RFC-0112 v1.14 (Numeric): Deterministic Vectors (DVEC)

## Summary
Implement the core DVEC type system: DVec<T> struct, NumericScalar trait (per RFC-0113 version), MaxScale trait, and type system enforcement. No arithmetic operations yet — just the container type and trait bounds.

## Acceptance Criteria
- [ ] `DVec<T>` struct with `data: Vec<T>` field
- [ ] `MaxScale` trait with `MAX_SCALE` constant (DQA=18, Decimal=36)
- [ ] `NumericScalar` trait (RFC-0113 canonical version) with: `scale()`, `raw_mantissa()`, `mul`, `add`, `sub`, `div`, `sqrt`, `is_zero`
- [ ] `Dqa` implements both traits (per RFC-0105)
- [ ] `Decimal` implements both traits (per RFC-0111)
- [ ] `DVec<Dfp>` is FORBIDDEN at type level (compile-time rejection)
- [ ] Mixed-type vector rejection (Vec<DVEC<DQA>> vs Vec<DVEC<Decimal>>)

## Implementation Notes
- DVEC<DFP> must be a compile-time error (ZkFriendly constraint)
- The RFC-0113 `NumericScalar` trait supersedes the v1.12 definition in this RFC
- `raw_mantissa()` returns the raw i128 mantissa for probe serialization

## Dependencies
- RFC-0112 §Type System
- RFC-0113 (for canonical NumericScalar trait)
- Mission 0111-decimal-core-type (completed first — Decimal must exist)

## Location
`determin/src/dvec.rs` (new file)

## Complexity
Medium — mostly type/trait definitions, no complex algorithms

## Reference
- RFC-0112 §Type System
- RFC-0112 §Production Limitations
- RFC-0113 (canonical NumericScalar trait)
