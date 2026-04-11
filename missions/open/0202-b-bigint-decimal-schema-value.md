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
- [ ] `Value::bigint()` constructor added (wraps BigInt in Value::Extension with tag 13). **BIGINT is variable-length** (1–520 bytes payload): the constructor uses `b.serialize().to_bytes()` producing `BigIntEncoding` format `[version:1][sign:1][reserved:2][num_limbs:1][reserved:3][limb0:8]...[limbN:8]`
- [ ] `Value::decimal()` constructor added (wraps Decimal in Value::Extension with tag 14). **DECIMAL is fixed-length** (24 bytes payload): uses `decimal_to_bytes(&d)` which returns exactly 24 bytes `[mantissa:16][reserved:7][scale:1]`
- [ ] `Value::as_bigint()` extractor added. Uses `&data[1..]` (variable-length slice) passed to `BigInt::deserialize()` — do NOT use a fixed slice bound
- [ ] `Value::as_decimal()` extractor added. Uses `data[1..25].try_into()` — the `[u8; 24]` conversion enforces the fixed 24-byte length
- [ ] `stoolap_parse_decimal(s: &str) -> Result<Decimal, DecimalError>` implemented per RFC §6.8a:
  - Input format: `^[+-]?[0-9]+(\.[0-9]+)?$` (rejects scientific notation, bare dots, whitespace-only)
  - Returns `DecimalError::InvalidScale` if fractional digits > 36
  - Returns `DecimalError::Overflow` if mantissa exceeds i128 range (>38 digits)
  - Returns `DecimalError::ParseError` for malformed input
- [ ] `Value::from_typed()` updated for Bigint/Decimal per RFC §6.8:
  - String input parse failures return `Err(Error::invalid_argument(...))`
  - Type mismatches return `Ok(Value::Null(data_type))`
  - DECIMAL path calls `stoolap_parse_decimal()`
- [ ] `Value::coerce_to_type()` / `into_coerce_to_type()` updated for BIGINT/DECIMAL coercion hierarchy per RFC §6.7:
  - INTEGER→BIGINT (always valid via `BigInt::from(i64)`)
  - INTEGER→DECIMAL (always valid via `Decimal::new(i128, 0)`)
  - BIGINT→DECIMAL: returns `Error::NotSupported("BIGINT → DECIMAL requires RFC-0202-B")` — do NOT return NULL
  - BIGINT/DECIMAL→FLOAT: blocked, use explicit CAST
- [ ] `Value::cast_to_type()` updated for explicit CAST per RFC §6.7:
  - AC-9a: BIGINT→INTEGER: uses `i64::try_from(&BigInt)`, returns `Error::invalid_argument("bigint out of range")` on overflow (maps `BigIntError::OutOfRange`)
  - AC-9b: DECIMAL→BIGINT: blocked by RFC-0202-B, returns `Error::NotSupported("DECIMAL → BIGINT requires RFC-0202-B")`
- [ ] `Value::as_string()` updated for BIGINT/DECIMAL per RFC §6.4:
  - BIGINT: `as_bigint().map(|bi| bi.to_string())`
  - DECIMAL: `as_decimal().and_then(|d| decimal_to_string(&d).ok())`
- [ ] `Value::Display` updated for BIGINT/DECIMAL numeric string output per RFC §6.3:
  - BIGINT: uses `as_bigint().to_string()`
  - DECIMAL: uses `decimal_to_string()` free function
- [ ] `compare_same_type()` updated for BIGINT/DECIMAL full ordering per RFC §6.6:
  - BIGINT: calls `ba.compare(&bb)` returning Ordering via match with explicit -1/0/1/ wildcard arms
  - DECIMAL: calls `decimal_cmp(&da, &db)` returning Ordering via match with explicit -1/0/1/ wildcard arms
  - The wildcard arms (`n => { debug_assert!(false, ...); Ordering::Greater }`) must be included per RFC

## Dependencies

- Mission: 0202-a-bigint-decimal-typesystem (open) — must complete first

## Location

`/home/mmacedoeu/_w/databases/stoolap/src/core/schema.rs`
`/home/mmacedoeu/_w/databases/stoolap/src/core/value.rs`

## Complexity

Medium — Value layer extension with type coercion rules

## Notes

**Coercion contract deviation (C4):** RFC-0202-A §6.7 changes the coerce_to_type contract for DECIMAL→INTEGER. Existing coerce_to_type returns NULL on failure. DECIMAL→INTEGER via BIGINT currently returns `Error::NotSupported` (not NULL) because the intermediate DECIMAL→BIGINT step is blocked by RFC-0202-B. This is intentional per RFC — "silent coercion failure would cause data correctness issues." Callers of coerce_to_type for DECIMAL→INTEGER must handle both `Value::Null` and `Error` returns during Phase 1-2.

**Cross-type comparison hazard:** RFC-0202-A §6.12 warns that adding BigInt/Decimal to is_numeric() (Phase 1) creates a latent as_float64().unwrap() panic for cross-type comparisons. This mission (Phase 1b) implements compare_same_type() but Phase 3 (mission 0202-d) implements the safe cross-type comparison dispatch. During Phase 1-2, comparing BigInt/Decimal with other numeric types will panic. No such tests should be written or executed until Phase 3 is complete.

**Integer→DECIMAL shortcut:** RFC §6.7 specifies an INTEGER→DECIMAL shortcut (`Decimal::new(i128::from(i), 0)`) separate from the full coercion hierarchy. This is a direct From implementation, not via BIGINT. AC-8 must implement this shortcut.

## Reference

- RFC-0202-A §2 (Value constructors/extractors — BigIntEncoding wire format)
- RFC-0202-A §6.4 (as_string update)
- RFC-0202-A §6.5 (NULL handling)
- RFC-0202-A §6.6 (compare_same_type — includes wildcard arm requirement)
- RFC-0202-A §6.7 (type coercion hierarchy — INTEGER→DECIMAL shortcut, DECIMAL→INTEGER via BIGINT)
- RFC-0202-A §6.8 (from_typed update — Result semantics)
- RFC-0202-A §6.8a (stoolap_parse_decimal — standalone parser function)
- RFC-0202-A §6.9 (SchemaColumn extension — decimal_scale: Option<u8>)
