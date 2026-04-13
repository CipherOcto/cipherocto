# Mission: DQA Typed Literal Syntax

## Status

Open

## RFC

RFC-0105 v2.14 (Numeric): Deterministic Quant Arithmetic

## Summary

Specify and implement `DQA '...'` typed literal syntax for SQL parser integration. Currently test vectors use `DQA '12345'` syntax which is not formally specified.

## Acceptance Criteria

- [ ] Specify `DQA '...'` literal syntax in RFC-0105 or companion RFC
- [ ] Implement parser support for DQA typed literals in stoolap
- [ ] `CAST(DQA '12345' AS BIGINT)` and similar expressions work
- [ ] Tests use programmatic value construction OR formally specified literal syntax

## Dependencies

- Mission: 0105-dqa-expression-vm (completed in stoolap)

## Location

`/home/mmacedoeu/_w/databases/stoolap/src/parser/` (stoolap)
RFC amendment: `rfcs/accepted/numeric/0105-deterministic-quant-arithmetic.md`

## Complexity

Medium

## Reference

- docs/reviews/round-10-rfc-0202-adversarial.md (M5 finding)
- RFC-0105 §Test Vectors (acknowledges syntax is informal)
