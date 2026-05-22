# Mission: BIGINT/DECIMAL Compiler CAST Integration

## Status

Open

## RFC

RFC-0110 (Numeric): Deterministic BIGINT
RFC-0111 (Numeric): Deterministic DECIMAL

## Summary

Add compiler support for CAST expressions involving BIGINT and DECIMAL types in stoolap.

## Acceptance Criteria

- [ ] AST support for `CAST(expr AS BIGINT)` and `CAST(expr AS DECIMAL)`
- [ ] Mapping from `ast::CastTarget::Bigint` → `Op::Cast(DataType::Bigint)`
- [ ] Mapping from `ast::CastTarget::Decimal` → `Op::Cast(DataType::Decimal)`
- [ ] Error handling for invalid cast targets
- [ ] Tests for all BIGINT/DECIMAL cast combinations

## Dependencies

- Mission: 0110-bigint-core-algorithms (completed in cipherocto)
- Mission: 0111-decimal-core-type (completed)

## Location

`/home/mmacedoeu/_w/databases/stoolap/src/executor/expression/compiler.rs`
`/home/mmacedoeu/_w/databases/stoolap/src/parser/`

## Complexity

Medium

## Reference

- docs/reviews/round-10-rfc-0202-adversarial.md (M4 finding)
- RFC-0110 §CAST Operations
- RFC-0111 §CAST Operations
