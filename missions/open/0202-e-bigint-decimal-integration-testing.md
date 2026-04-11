# Mission: RFC-0202-A Phase 4 — Integration Testing and Verification

## Status

Open

## RFC

RFC-0202-A (Storage): Stoolap BIGINT and DECIMAL Core Types

## Summary

End-to-end integration testing and benchmarking for BIGINT/DECIMAL in stoolap. Verify round-trip serialization, SQL parser coverage, gas cost benchmarking, and cross-type comparison behavior. This is the final verification gate before production deployment.

## Acceptance Criteria

- [ ] Integration tests with RFC-0110 test vectors: bigint arithmetic, overflow, SHL/SHR, bitlen, cmp
- [ ] Integration tests with RFC-0111 test vectors: decimal arithmetic, sqrt, overflow, canonicalization
- [ ] SQL parser tests for `BIGINT '...'` and `DECIMAL '...'` literals
- [ ] SQL parser tests for `DECIMAL(p,s)` and `NUMERIC(p,s)` DDL column creation
- [ ] **Verify `BigInt::from_str("-0")` produces canonical zero** — compare zero encoding with `BigInt::from_str("0")`; if different, update test vectors accordingly
- [ ] Cross-type comparison tests: BIGINT vs Integer, DECIMAL vs Float, BIGINT vs DECIMAL
- [ ] Serialization round-trip tests: BIGINT → serialize → deserialize → same value
- [ ] Serialization round-trip tests: DECIMAL → serialize → deserialize → same value
- [ ] **Benchmark serialization/deserialization gas costs** across representative payload sizes (1-limb through 64-limb BIGINT; scale 0 through scale 36 DECIMAL). Compare measured values against §8 formulas. If divergence exceeds 2×, update formulas.
- [ ] BTree index range scan tests: `WHERE bigint_col > BIGINT '1000'`, `WHERE dec_col < DECIMAL '99.99'`
- [ ] NULL handling tests: BIGINT/DECIMAL NULL in expressions, IS NULL, ORDER BY NULL

## Dependencies

- Mission: 0202-c-bigint-decimal-persistence (for serialization tests)
- Mission: 0202-d-bigint-decimal-vm (for arithmetic and gas tests)

## Location

`/home/mmacedoeu/_w/databases/stoolap/tests/`

## Complexity

Medium — integration test coverage

## Reference

- RFC-0202-A §9 (Test Vectors)
- RFC-0202-A §8 (Gas Metering Model)
- RFC-0110 §Test Vectors
- RFC-0111 §Test Vectors
