# Mission: RFC-0202-A Phase 1b — SchemaColumn and Value Layer

## Status

Open

## RFC

RFC-0202-A (Storage): Stoolap BIGINT and DECIMAL Core Types

## Summary

Extend Stoolap's SchemaColumn and Value types with BIGINT/DECIMAL support: decimal_scale field, Value constructors/extractors, from_typed with Result semantics, type coercion, and comparison. This mission extends the Value layer once the type system is in place.

## Acceptance Criteria

- [ ] `SchemaColumn.decimal_scale: Option<u8>` field added (None = not a DECIMAL column, Some(s) = DECIMAL with scale s)
- [ ] `SchemaBuilder::set_last_decimal_scale()` builder method added (consuming builder pattern)
- [ ] `Value::bigint()` constructor added (wraps BigInt in Value::Extension with tag 13)
- [ ] `Value::decimal()` constructor added (wraps Decimal in Value::Extension with tag 14)
- [ ] `Value::as_bigint()` extractor added
- [ ] `Value::as_decimal()` extractor added
- [ ] `Value::from_typed()` updated for Bigint/Decimal: String input parse failures return `Err`, type mismatches return `Ok(Value::Null(data_type))`
- [ ] `Value::coerce_to_type()` / `into_coerce_to_type()` updated for BIGINT/DECIMAL coercion hierarchy
- [ ] `Value::cast_to_type()` updated for explicit CAST (BIGINT→INTEGER trap, DECIMAL→BIGINT trap)
- [ ] `Value::Display` updated for BIGINT/DECIMAL numeric string output
- [ ] `compare_same_type()` updated for BIGINT/DECIMAL full ordering (calls BigInt::compare and decimal_cmp)

## Dependencies

- Mission: 0202-a-bigint-decimal-typesystem (open) — must complete first

## Location

`/home/mmacedoeu/_w/databases/stoolap/src/core/schema.rs`
`/home/mmacedoeu/_w/databases/stoolap/src/core/value.rs`

## Complexity

Medium — Value layer extension with type coercion rules

## Reference

- RFC-0202-A §6.4 (as_string update)
- RFC-0202-A §6.5 (NULL handling)
- RFC-0202-A §6.6 (compare_same_type)
- RFC-0202-A §6.7 (type coercion hierarchy)
- RFC-0202-A §6.8 (from_typed update)
- RFC-0202-A §6.9 (SchemaColumn extension)
