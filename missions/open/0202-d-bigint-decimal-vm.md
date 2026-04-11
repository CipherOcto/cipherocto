# Mission: RFC-0202-A Phase 3 — Expression VM Support

## Status

Open

## RFC

RFC-0202-A (Storage): Stoolap BIGINT and DECIMAL Core Types

## Summary

Add BIGINT and DECIMAL operation dispatch in Stoolap's expression VM with formula-based gas metering. This mission adds arithmetic, comparison, and bitwise operation opcodes to the VM, wired to the determin crate's BigInt and Decimal operations with gas tracking.

## Acceptance Criteria

- [ ] BIGINT operation dispatch added in `src/executor/expression/vm.rs`: ADD, SUB, MUL, DIV, MOD, CMP, SHL, SHR, BITLEN, SQRT
- [ ] DECIMAL operation dispatch added: ADD, SUB, MUL, DIV, SQRT, CMP
- [ ] Gas metering wired: compute gas per RFC-0110/RFC-0111 formulas using operand sizes (limb count for BIGINT, scales for DECIMAL)
- [ ] Per-operation gas accumulated in query gas accumulator
- [ ] `Error::OutOfGas` returned when query exceeds configurable per-query limit (default: 50,000)
- [ ] `MAX_BIGINT_OP_COST` (15,000) and `MAX_DECIMAL_OP_COST` (5,000) as per-operation caps
- [ ] Cost estimates added for optimizer (plan cost modeling)
- [ ] Streaming aggregation gas checked per-row (SUM, AVG)

## Dependencies

- Mission: 0202-c-bigint-decimal-persistence (open)
- Mission: 0110-bigint-mul-div-test-coverage (completed) — algorithms verified
- Mission: 0111-decimal-arithmetic (completed) — algorithms verified

## Location

`/home/mmacedoeu/_w/databases/stoolap/src/executor/expression/vm.rs`
`/home/mmacedoeu/_w/databases/stoolap/src/executor/expression/ops.rs`
`/home/mmacedoeu/_w/databases/stoolap/src/core/value.rs`

## Complexity

Medium — VM dispatch and gas integration

## Reference

- RFC-0202-A §7 (Arithmetic Operations)
- RFC-0202-A §7a (Aggregate Operations)
- RFC-0202-A §8 (Gas Metering Model)
- RFC-0110 §Operations (BigInt ADD, SUB, MUL, DIV, MOD, CMP, SHL, SHR, BITLEN, SQRT)
- RFC-0111 §Operations (Decimal ADD, SUB, MUL, DIV, SQRT, CMP)
