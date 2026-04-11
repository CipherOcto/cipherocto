# Mission: RFC-0202-A Phase 4 — Integration Testing and Verification

## Status

Open

**Blocked by:** Missions 0202-a, 0202-b, 0202-c, 0202-d (all must complete before any AC can be executed). All prerequisite missions are currently Open.

## RFC

RFC-0202-A (Storage): Stoolap BIGINT and DECIMAL Core Types

## Summary

End-to-end integration testing and benchmarking for BIGINT/DECIMAL in stoolap. Verify round-trip serialization, SQL parser coverage, gas cost benchmarking, and cross-type comparison behavior. This is the final verification gate before production deployment.

## Acceptance Criteria

- [ ] Integration tests with RFC-0110 test vectors (56 entries with Merkle root):
  - Execute all 56 test vectors for BIGINT (arithmetic, overflow, SHL, SHR, bitlen, cmp)
  - Verify Merkle root of test vector outputs matches RFC-0110 §Test Vectors Merkle root using SHA-256
  - **Expected root:** `c447fa82db0763435c1a18268843300c2ed811e21fcb400b18c75e579ddac7c0`
  - Document Merkle verification result (pass/fail with computed root hash)
- [ ] Integration tests with RFC-0111 test vectors (57 entries with Merkle root):
  - Execute all 57 test vectors for DECIMAL (arithmetic, sqrt, overflow, canonicalization)
  - Verify Merkle root of test vector outputs matches RFC-0111 §Test Vectors Merkle root using SHA-256
  - **Expected root:** `496bc8038e3fd38462f4308bf03088b3f872d000256a45ddb53d4932efff0c1c`
  - Include explicit DECIMAL SQRT test vectors from RFC-0202-A §9:
    - `SQRT(DECIMAL '2.00')` → `{mantissa: 141, scale: 2}` (scale = ⌈(2+1)/2⌉ = 2)
    - `SQRT(DECIMAL '0.000001')` → `{mantissa: 10, scale: 4}` (scale = ⌈(6+1)/2⌉ = 4)
    - Verify result scale computation matches `⌈(input_scale + 1) / 2⌉`
- [ ] SQL parser tests for `BIGINT '...'` and `DECIMAL '...'` literals
- [ ] SQL parser tests for `DECIMAL(p,s)` and `NUMERIC(p,s)` DDL column creation
- [ ] **Canonical zero verification:** Two-part check — verify determin crate behavior first, then execute:
  - **Part A:** Determine `BigInt::from_str("-0")` behavior in determin crate: returns `Error` or canonical bytes
    - If `Error`: then `BigInt::from_str("-0")` is rejected per RFC-0110 §10.2 (TRAP). Part B uses only `BigInt::from_str("0")`.
    - If canonical bytes: both "-0" and "0" produce identical wire bytes `[13]01000000010000000000000000000000`. Verify both parse and serialize identically.
  - **Part B:** Execute canonical zero verification per above determination.
  - **Note:** If `BigInt::from_str("-0")` returns `Error`, this is NOT a failure — it is correct per RFC-0110 §10.2. Update RFC §9 test vectors to reflect actual canonical form if needed and file issue against determin crate.
- [ ] Cross-type comparison tests: **execute only after Phase 3 (mission 0202-d) is complete** — these tests will panic during Phase 1-2 via `as_float64().unwrap()`. Phase 3 implements the safe cross-type comparison dispatch that avoids the panic. **Prerequisite:** Phase 3 (0202-d) MUST implement safe cross-type comparison dispatch that avoids the `as_float64().unwrap()` panic described in 0202-d Notes.
  - BIGINT vs Integer
  - DECIMAL vs Float
  - BIGINT vs DECIMAL
- [ ] Serialization round-trip tests for BIGINT (verify against RFC §9 wire format test vectors):
  - BIGINT '1' serializes to `[13]01000000010000000100000000000000`
  - BIGINT '-1' serializes to `[13]01FF0000010000000100000000000000`
  - BIGINT '0' serializes to `[13]01000000010000000000000000000000`
  - BIGINT '2^64' serializes to `[13]010000000200000000000000000000000100000000000000`
  - BIGINT → serialize → deserialize → same value (byte-identical)
- [ ] Serialization round-trip tests for DECIMAL (verify against RFC §9 wire format test vectors):
  - DECIMAL '123.45' serializes to `[14]000000000000000000000000000030390000000000000002`
  - DECIMAL '1' serializes to `[14]000000000000000000000000000000010000000000000000`
  - DECIMAL '3' serializes to `[14]000000000000000000000000000000030000000000000000`
  - DECIMAL '0' serializes to `[14]000000000000000000000000000000000000000000000000`
  - DECIMAL '-12.3' serializes to `[14]FFFFFFFFFFFFFFFFFFFFFFFFFFFFFF850000000000000001`
  - DECIMAL → serialize → deserialize → same value (byte-identical)
- [ ] **Benchmark serialization/deserialization gas costs** per RFC §8:
  - BIGINT: measure `BigInt::serialize()` and `BigInt::deserialize()` gas across 1-limb, 16-limb, 32-limb, 64-limb payloads
  - DECIMAL: measure `decimal_to_bytes()` and `decimal_from_bytes()` gas across scale 0, 12, 24, 36
  - Compare measured values against RFC-0202-A §8 estimates (serialize ~100, deserialize ~100 for BIGINT; ~20 each for DECIMAL)
  - If measured/estimated ratio exceeds 2× in either direction, update the RFC §8 formulas before production deployment
  - Document benchmark methodology and results
- [ ] BTree index range scan tests with lexicographic ordering verification:
  - Cross-sign ordering: `BIGINT '-100' < BIGINT '0' < BIGINT '100'`
  - Cross-sign ordering: `DECIMAL '-12.3' < DECIMAL '0' < DECIMAL '12.3'`
  - Within-negative ordering (per RFC §6.11: more limbs = more negative): `BIGINT '-2^64' < BIGINT '-1'` (2 limbs vs 1 limb, both negative)
  - Within-positive ordering (per RFC §6.11: more limbs = larger): `BIGINT '2^64' > BIGINT '1'` (2 limbs vs 1 limb, both positive)
  - Zero vs positive: `BIGINT '0' < BIGINT '1'` — byte comparison confirms zero's all-zero limb array sorts before non-zero limbs
  - DECIMAL within-negative: `DECIMAL '-2' < DECIMAL '-1'` (both negative; -2 sorts below -1 after sign-flip encoding); `DECIMAL '-100' < DECIMAL '-1'` (3-digit vs 1-digit mantissa, both negative); verify sign-flip: `DECIMAL '-1'` (mantissa = -1) → encoded byte0 = 0x80 XOR 0x7F = 0xFF → sorts among negatives
  - Verify range scan returns correctly ordered results (not just non-empty results)
  - `WHERE bigint_col > BIGINT '1000'`, `WHERE dec_col < DECIMAL '99.99'`
- [ ] Aggregate operation tests for BIGINT:
  - `COUNT(BIGINT col)` on NULL-only column → `0` (COUNT never returns NULL for empty sets)
  - `SUM(BIGINT col)` on NULL-only column → NULL
  - `MIN/MAX(BIGINT col)` on NULL-only column → NULL
  - `SUM` overflow: `SUM` of values exceeding ±(2^4096 − 1) → `BigIntError::OutOfRange`
  - `AVG(BIGINT col)` → `Error::NotSupported("AVG on BIGINT requires RFC-0202-B")`
- [ ] Aggregate operation tests for DECIMAL:
  - `COUNT(DECIMAL col)` on NULL-only column → `0`
  - `SUM(DECIMAL col)` on NULL-only column → NULL
  - `MIN/MAX(DECIMAL col)` on NULL-only column → NULL
  - `SUM` overflow: `SUM` of values exceeding ±(10^36 − 1) → `DecimalError::Overflow`
  - `AVG(DECIMAL '1.000000')` → result scale ≥ 6 (input_scale + 6 capped at 36)
- [ ] Aggregate operation tests for mixed NULL/data columns:
  - Verify NULLs are excluded from SUM/AVG/MIN/MAX but counted by COUNT
  - Verify NULL sorts as lowest in MIN/MAX
- [ ] NULL handling tests: BIGINT/DECIMAL NULL in expressions, IS NULL, ORDER BY NULL
- [ ] Division by zero tests:
  - `BIGINT '1' / BIGINT '0'` → `Error::invalid_argument("division by zero")`
  - `DECIMAL '1.0' / DECIMAL '0.0'` → `Error::invalid_argument("division by zero")`
  - Verify error is returned, not panic or incorrect value
- [ ] `as_int64()` and `as_float64()` round-trip tests:
  - `BIGINT '42'.as_int64()` → `Some(42)`
  - `BIGINT '99999999999999999999'.as_int64()` → `None` (out of i64 range)
  - `DECIMAL '0.1'.as_float64()` → `10.0` (exact representable)
  - `DECIMAL '12345678901234567890.0'.as_float64()` → f64 value (precision loss acceptable per RFC §6.13)

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
