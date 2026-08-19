# Mission: DQA Typed Literal Syntax

## Status

**LANDED 2026-08-19 (AC-1 + AC-4 spec-only).** RFC-0105 v2.1 → v2.2
amendment adds §DQA Typed Literal Syntax subsection (§SQL Value
Ingress) formalizing the `DQA '...'` / `DQA(n) '...'` grammar.
CAST interaction table with BIGINT/DECIMAL/DQA targets. Parser
implementation (AC-2 + AC-3) deferred to fork-side work — out of
cipherocto scope.

## RFC

RFC-0105: Deterministic Quant Arithmetic

## Summary

Specify and implement `DQA '...'` typed literal syntax for SQL parser integration. Currently test vectors use `DQA '12345'` syntax which is not formally specified.

## Acceptance Criteria

- [x] Specify `DQA '...'` literal syntax in RFC-0105 or companion RFC
      (RFC-0105 v2.2 §DQA Typed Literal Syntax subsection)
- [ ] Implement parser support for DQA typed literals in stoolap
      (FORK-SIDE — out of cipherocto scope; track in fork repo)
- [ ] `CAST(DQA '12345' AS BIGINT)` and similar expressions work
      (depends on parser impl above)
- [x] Tests use programmatic value construction OR formally specified
      literal syntax (formal spec now in RFC-0105 §DQA Typed Literal
      Syntax — fixtures may continue using programmatic value
      construction until parser impl lands)

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
