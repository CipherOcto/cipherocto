# Mission: DFP Type-Aware Compilation

## Status

Claimed

## RFC

RFC-0104 v1.17 (Numeric): Deterministic Floating-Point Abstraction

## Summary

Add schema-to-VM type propagation so the compiler is aware of DFP column types and emits appropriate operations.

## Acceptance Criteria

- [ ] Compiler detects DFP column types from schema
- [ ] Compiler propagates type information to VM for DFP expressions
- [ ] Type-safe compilation for DFP expressions
- [ ] Existing tests pass

## Dependencies

- Mission: 0104-dfp-datatype-integration (completed)

## Location

`/home/mmacedoeu/_w/databases/stoolap/src/executor/expression/compiler.rs`

## Complexity

Medium

## Claimant

Claude Code Agent

## Pull Request

#

## Notes

This mission enables the compiler to track and propagate DFP column type information. The "deterministic flag" concept from RFC-0104 is separate infrastructure (deferred to RFC-0110 per RFC-0104 §A6). This mission focuses on type-aware compilation for DFP.

## Reference

- RFC-0104 §Expression VM Opcodes
- docs/reviews/rfc-0104-dfp-code-review.md (S11 finding)
