# Mission: Bare DECIMAL Default Scale Design Decision

## Status

Open

## RFC

RFC-0111 (Numeric): Deterministic DECIMAL

## Summary

Resolve the bare DECIMAL default scale behavior. Currently `DECIMAL` maps to `decimal_scale=0` which rounds all fractional values to integers.

## Acceptance Criteria

- [ ] Choose default behavior for bare DECIMAL:
  - Option 1: decimal_scale=0 (current, rounds to integer)
  - Option 2: sentinel value 255 (no limit, SQL-standard behavior)
  - Option 3: decimal_scale=36 (max scale, accepts everything)
- [ ] Document the chosen design decision in RFC-0111
- [ ] Update implementation if needed

## Dependencies

- None

## Location

`rfcs/accepted/numeric/0111-deterministic-decimal.md`
`/home/mmacedoeu/_w/databases/stoolap/src/core/value.rs`

## Complexity

Low

## Reference

- docs/reviews/round-10-rfc-0202-adversarial.md (H3 finding)
- RFC-0111 §DECIMAL Type Definition
