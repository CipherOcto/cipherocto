# Mission: DECIMAL Whitespace Specification Amendment

## Status

Open

## RFC

RFC-0111 (Numeric): Deterministic DECIMAL

## Summary

Align DECIMAL parsing specification with actual code behavior for whitespace handling. The spec says "no leading/trailing whitespace" but the code trims whitespace.

## Acceptance Criteria

- [ ] Amend RFC-0111 spec to say whitespace is stripped before parsing
- [ ] Or clarify in spec that whitespace rejection is intentional and code should be fixed
- [ ] Update version history in RFC

## Dependencies

- None (spec task)

## Location

`rfcs/accepted/numeric/0111-deterministic-decimal.md`

## Complexity

Low

## Reference

- docs/reviews/round-10-rfc-0202-adversarial.md (C3 finding)
- stoolap/src/core/value.rs (stoolap_parse_decimal trims whitespace)
