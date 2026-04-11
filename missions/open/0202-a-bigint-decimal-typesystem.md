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
- [ ] `from_str_versioned()` added for NUMERIC_SPEC_VERSION migration gate: version 1 routes NUMERIC/DECIMAL to Float; version 2 parses as DataType::Decimal
- [ ] `Display` updated to render `BIGINT` and `DECIMAL`
- [ ] `is_numeric()` updated to include `Bigint | Decimal`
- [ ] `is_orderable()` updated to include `Bigint | Decimal`
- [ ] `from_u8()` entries added for discriminants 13 and 14
- [ ] `NUMERIC_SPEC_VERSION: u32 = 2` constant added to `src/storage/mvcc/persistence.rs`

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
- RFC-0202-A §6.1 (FromStr update)
- RFC-0202-A §6.2 (Display update)
- RFC-0202-A §6.3 (is_numeric, is_orderable, from_u8)
- RFC-0202-A §4a (NUMERIC_SPEC_VERSION migration gate)
