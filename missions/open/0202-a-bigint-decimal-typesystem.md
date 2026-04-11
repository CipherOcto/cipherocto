# Mission: RFC-0202-A Phase 1 — BigInt/DECIMAL Type System Integration

## Status

Open

## RFC

RFC-0202-A (Storage): Stoolap BIGINT and DECIMAL Core Types

## Summary

Integrate BIGINT and DECIMAL into Stoolap's core type system: DataType enum extension (Bigint=13, Decimal=14), SQL keyword parsing, Display, type predicates (is_numeric, is_orderable, from_u8), and the NUMERIC_SPEC_VERSION constant. This is the foundational layer that enables all subsequent phases.

## Acceptance Criteria

- [ ] `DataType::Bigint = 13` and `DataType::Decimal = 14` added to `src/core/types.rs`
- [ ] `FromStr` updated to parse `BIGINT` keyword and `DECIMAL`/`NUMERIC` keywords (with `starts_with` for parameterized forms DECIMAL(p,s), NUMERIC(p,s))
- [ ] `NUMERIC_SPEC_VERSION: u32 = 2` constant defined in `src/storage/mvcc/persistence.rs` (canonical location per RFC §4a)
- [ ] `from_str_versioned(s: &str, spec_version: u32) -> Result<DataType, Error>` added to `src/storage/mvcc/` (not types.rs — this is a persistence-layer DDL-replay function, not a types.rs concern)
- [ ] `Display` updated to render `BIGINT` and `DECIMAL`
- [ ] `is_numeric()` updated to include `Bigint | Decimal`
- [ ] `is_orderable()` updated to include `Bigint | Decimal`
- [ ] `from_u8()` entries added for discriminants 13 and 14
- [ ] Unit tests updated in `src/core/types.rs`:
  - `test_datatype_is_numeric`: assert `Bigint.is_numeric()` and `Decimal.is_numeric()` are true
  - `test_datatype_is_orderable`: assert `Bigint.is_orderable()` and `Decimal.is_orderable()` are true
  - `test_datatype_u8_conversion`: add `from_u8(13)` → `Bigint` and `from_u8(14)` → `Decimal`
  - `test_datatype_from_str`: add cases for `"BIGINT"`, `"DECIMAL"`, `"NUMERIC(10,2)"`, `"NUMERIC"` → Decimal
  - `test_datatype_display`: assert `Bigint.display()` → "BIGINT" and `Decimal.display()` → "DECIMAL"
- [ ] Unit tests for `from_str_versioned()` in persistence layer:
  - `from_str_versioned("BIGINT", 1)` → `Ok(DataType::Integer)`
  - `from_str_versioned("DECIMAL", 1)` → `Ok(DataType::Float)`
  - `from_str_versioned("BIGINT", 2)` → `Ok(DataType::Bigint)`
  - `from_str_versioned("DECIMAL", 2)` → `Ok(DataType::Decimal)`
  - `from_str_versioned("DECIMAL(10,2)", 1)` → `Ok(DataType::Float)` (legacy parameterized form)
  - `from_str_versioned("DECIMAL(10,2)", 2)` → `Ok(DataType::Decimal)`
  - **Note:** `as_int64()` and `as_float64()` Extension methods are in mission 0202-b scope (value.rs), not types.rs — per RFC §6.13 Key Files table

## Dependencies

- Mission: 0110-bigint-core-algorithms (completed) — provides BigInt type
- Mission: 0111-decimal-core-type (completed) — provides Decimal type
- Mission: 0110-wal-numeric-spec-version (open) — WAL header integration

## Location

`/home/mmacedoeu/_w/databases/stoolap/src/core/types.rs`
`/home/mmacedoeu/_w/databases/stoolap/src/storage/mvcc/persistence.rs`

## Complexity

Medium — primarily type system extension and parser integration

## Reference

- RFC-0202-A §1 (DataType discriminants)
- RFC-0202-A §4a (NUMERIC_SPEC_VERSION migration gate)
- RFC-0202-A §6.1 (FromStr update)
- RFC-0202-A §6.2 (Display update)
- RFC-0202-A §6.3 (is_numeric, is_orderable, from_u8)
- RFC-0202-A §6.4–§6.9 (Value layer extensions — implemented in mission 0202-b)
- RFC-0202-A §6.13 (as_int64/as_float64 Extension methods — implemented in mission 0202-b)

## Notes

**Latent panic warning (C1):** Adding BigInt/Decimal to `is_numeric()` enables the `is_numeric()` branch in `Value::compare()` for these types before Phase 3 implements safe cross-type comparison dispatch. RFC-0202-A §6.12 warns: `Value::compare()` uses `as_float64().unwrap()` for cross-type numeric comparison, which **panics** for Extension-based numeric types. During Phase 1-2, any cross-type comparison involving BigInt/Decimal (e.g., `WHERE bigint_col > 42`, `SELECT bigint_col = float_col`) will panic. Phase 3 (mission 0202-d) resolves this. **No test exercising cross-type BigInt/Decimal comparison should be written or executed until Phase 3 is complete.**

**from_str_versioned location (C2):** The `from_str_versioned()` function is persistence-layer infrastructure used during WAL replay and schema loading. It belongs in `src/storage/mvcc/` (or a dedicated schema module), not `src/core/types.rs`. The constant `NUMERIC_SPEC_VERSION` is defined in persistence.rs per RFC §4a; types.rs imports it if needed.

**Decimal Display limitation:** `DataType::Decimal.display()` outputs `"DECIMAL"` only — precision and scale are not included. For `SHOW CREATE TABLE` or schema introspection, use `SchemaColumn.decimal_scale` directly. This is a known RFC-0202-A limitation (§6.2).
