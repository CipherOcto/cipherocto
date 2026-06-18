# Mission: DECIMAL Display and Error Handling

## Status

Open

## RFC

RFC-0111 (Numeric): Deterministic DECIMAL

## Summary

Fix DECIMAL display formatting and `decimal_to_string` error handling in stoolap's Value API.

## Acceptance Criteria

- [ ] Add DECIMAL(p,s) Display output in SchemaColumn (e.g., `price DECIMAL(36,2)`)
- [ ] Fix `decimal_to_string` error handling: use `<invalid decimal>` instead of empty string
- [ ] All DECIMAL Display tests pass

## Dependencies

- Mission: 0111-decimal-core-type (completed)
- Mission: 0111-decimal-serialization (completed)

## Location

`/home/mmacedoeu/_w/databases/stoolap/src/core/schema.rs`
`/home/mmacedoeu/_w/databases/stoolap/src/core/value.rs`

## Complexity

Low

## Reference

- docs/reviews/round-10-rfc-0202-adversarial.md (M1, M2 findings)
- RFC-0111 §Display and String Representation
