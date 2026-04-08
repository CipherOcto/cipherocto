# Mission: DFP Type-Aware Compilation

## Status

Claimed

## RFC

RFC-0104 v1.17 (Numeric): Deterministic Floating-Point Abstraction

## Summary

Add schema-to-VM type propagation so the compiler is aware of DFP column types and emits appropriate operations. Also add DFP-specific opcodes to avoid runtime type dispatch.

## Acceptance Criteria

- [ ] Compiler detects DFP column types from schema
- [ ] Compiler propagates type information to VM for DFP expressions
- [ ] Add DFP-specific opcodes: DfpAdd, DfpSub, DfpMul, DfpDiv, DfpNeg (fixes S12)
- [ ] Type-safe compilation for DFP expressions using dedicated opcodes
- [ ] Existing tests pass

## Dependencies

- Mission: 0104-dfp-datatype-integration (completed)
- Mission: 0104-dfp-expression-vm (completed)

## Location

`/home/mmacedoeu/_w/databases/stoolap/src/executor/expression/compiler.rs`
`/home/mmacedoeu/_w/databases/stoolap/src/executor/expression/ops.rs`
`/home/mmacedoeu/_w/databases/stoolap/src/executor/expression/vm.rs`

## Complexity

Medium-High

## Claimant

Claude Code Agent

## Pull Request

#

## Notes

This mission enables the compiler to track and propagate DFP column type information. The "deterministic flag" concept from RFC-0104 is separate infrastructure (deferred to RFC-0110 per RFC-0104 §A6).

Includes S12 fix: DFP uses generic `Op::Add/Sub/Mul/Div` with runtime dispatch. This will add dedicated `DfpAdd`, `DfpSub`, `DfpMul`, `DfpDiv`, `DfpNeg` opcodes similar to existing `DqaAdd`, `DqaSub`, etc.

## Reference

- RFC-0104 §Expression VM Opcodes
- docs/reviews/rfc-0104-dfp-code-review.md (S11, S12 findings)
