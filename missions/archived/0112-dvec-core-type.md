# Mission: DVEC Core Type and Trait Definitions

## Status
Completed (2026-03-21)

## RFC
RFC-0112 v1.14 (Numeric): Deterministic Vectors (DVEC)

## Summary
Implemented the core DVEC type system: `DVec<T>` struct, `DvecScalar` trait, `DvecError` enum, `Dqa` and `Decimal` trait impls. No arithmetic algorithms — just the type and trait wiring with stubs.

## Acceptance Criteria
- [x] `DVec<T>` struct with `data: Vec<T>` field
- [x] `DvecScalar` trait (local RFC-0112 version, supersedable by RFC-0113 `NumericScalar`)
- [x] `Dqa` implements `DvecScalar`
- [x] `Decimal` implements `DvecScalar`
- [x] `DVec<Dfp>` is FORBIDDEN at type level (compile-time rejection via missing impl)
- [x] Element-wise ops (vec_add/sub/mul/scale) fully implemented
- [x] Stubs for dot_product, squared_distance, norm, normalize (fill in arithmetic mission)

## What Was NOT Implemented (per design choice)
- `MaxScale` trait — scale limits are checked inline in each operation (DQA ≤ 9, Decimal ≤ 18)
- RFC-0113 `NumericScalar` — `DvecScalar` is the local version; migration path documented in module docstring

## Implementation Notes
- `DvecScalar::sqrt` → `Err(DqaError::InvalidInput)` for DQA (no SQRT per RFC-0105)
- `DvecScalar::sqrt` → `decimal_sqrt(&self)` for Decimal (canonical input, no raw variant needed)
- All element-wise ops use `validate_uniform_scale` helper
- `DvecError::Unsupported` used as stub return for operations not yet filled in
- RFC-0113 supertrait comment documented in module docstring

## Test Results
- 234 tests pass (225 pre-existing + 9 new DVEC tests)
- New tests cover: Dqa/Decimal trait impl sanity, vec_add basic, scale/dimension mismatch, dot_product/norm/normalize stubs

## Dependencies
- Mission 0111-decimal-core-type (completed — Decimal must exist)

## Location
`determin/src/dvec.rs` (new), `determin/src/lib.rs` (updated)

## Complexity
Low — mostly type/trait definitions and forwarding wrappers

## Reference
- RFC-0112 §Type System
- RFC-0112 §Production Limitations
- RFC-0112 §Element-wise Operations
