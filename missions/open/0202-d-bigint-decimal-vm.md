# Mission: RFC-0202-A Phase 3 — Expression VM Support

## Status

Open

## RFC

RFC-0202-A (Storage): Stoolap BIGINT and DECIMAL Core Types

## Summary

Add BIGINT and DECIMAL operation dispatch in Stoolap's expression VM with formula-based gas metering. This mission adds arithmetic, comparison, and bitwise operation opcodes to the VM, wired to the determin crate's BigInt and Decimal operations with gas tracking.

## Acceptance Criteria

- [ ] BIGINT operation dispatch added in `src/executor/expression/vm.rs`: ADD, SUB, MUL, DIV, MOD, CMP, SHL, SHR, BITLEN
  - Division by zero check: before executing DIV, verify divisor is non-zero. If zero, return `Error::invalid_argument("division by zero")` and consume pre-flight gas only (10 gas), not full operation gas.
  - Pre-flight bounds check for SHL/SHR: verify shift count is within valid bounds (0 ≤ shift < 8 × num_limbs). Pre-flight check consumes 10 gas. If bounds check fails, return error without executing full operation.
- [ ] DECIMAL operation dispatch added: ADD, SUB, MUL, DIV, SQRT, CMP
  - Division by zero check: before executing DIV, verify divisor is non-zero. If zero, return `Error::invalid_argument("division by zero")` and consume pre-flight gas only.
  - `decimal_div(a, b, 0)`: the third parameter `_target_scale` is ignored by the implementation — pass `0` as placeholder per RFC §7.
- [ ] BIGINT aggregate dispatch: COUNT, SUM, MIN, MAX, AVG
  - COUNT: returns INTEGER, never overflows
  - SUM: returns BIGINT, returns `BigIntError::OutOfRange` when sum exceeds ±(2^4096 − 1); map to `Error::OutOfGas`
  - MIN/MAX: returns BIGINT, never overflows
  - AVG: blocked — returns `Error::NotSupported("AVG on BIGINT requires RFC-0202-B")` until RFC-0202-B implements internal BIGINT→DECIMAL conversion
- [ ] DECIMAL aggregate dispatch: COUNT, SUM, MIN, MAX, AVG
  - COUNT: returns INTEGER, never overflows
  - SUM: returns DECIMAL, returns `DecimalError::Overflow` if sum exceeds ±(10^36 − 1)
  - MIN/MAX: returns DECIMAL, never overflows
  - AVG: returns DECIMAL; result scale = `min(36, input_scale + 6)`; returns `DecimalError::Overflow` if sum overflows
- [ ] Gas metering wired per RFC §8 formulas:
  - BIGINT: ADD/SUB = 10 + limbs; MUL = 50 + 2 × limbs × limbs; DIV/MOD = 50 + 3 × limbs × limbs; CMP = 5 + limbs; SHL/SHR = 10 + limbs; BITLEN = 10 + limbs (conservative estimate — verify against RFC-0110 reference before production)
  - DECIMAL: ADD/SUB = 10 + 2 × |scale_a − scale_b|; MUL = 20 + 3 × scale_a × scale_b; DIV = 50 + 3 × scale_a × scale_b; SQRT = 100 + 5 × scale; CMP uses `decimal_cmp`
  - Per-operation caps: `MAX_BIGINT_OP_COST` (15,000) and `MAX_DECIMAL_OP_COST` (5,000) from determin crate
- [ ] Per-operation gas accumulated in query gas accumulator
- [ ] `Error::OutOfGas` returned when query exceeds configurable per-query limit (default: 50,000)
- [ ] Streaming aggregation gas checked per-row (COUNT, SUM, MIN/MAX, AVG) per RFC §7a:
  - BIGINT COUNT: 5 gas per row
  - BIGINT SUM: 10 + limbs per row
  - BIGINT MIN/MAX: 5 + limbs per row
  - BIGINT AVG: 15 + 2 × limbs per row
  - DECIMAL COUNT: 5 gas per row
  - DECIMAL SUM: 10 + 2 × scale per row
  - DECIMAL MIN/MAX: 5 + 2 × scale per row
  - DECIMAL AVG: 15 + 3 × scale per row (input column scale, not result scale)
- [ ] Cost estimates added for optimizer (plan cost modeling):
  - Use per-operation gas formulas as the cost unit
  - BIGINT: cost scales with limb count (1–64 limbs)
  - DECIMAL: cost scales with scale (0–36)
  - Provide estimated costs for query planning (e.g., index scan vs. table scan decisions involving BIGINT/DECIMAL columns)

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
- RFC-0110 §Operations (BigInt ADD, SUB, MUL, DIV, MOD, CMP, SHL, SHR, BITLEN — SQRT is N/A for BIGINT per RFC §7)
- RFC-0111 §Operations (Decimal ADD, SUB, MUL, DIV, SQRT, CMP)
- **BITLEN gas:** RFC-0110 §8 specifies `10 + limbs` (v2.14). No amendment needed — implement per RFC.
