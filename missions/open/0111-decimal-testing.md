# Mission: DECIMAL Testing & Verification Probe

## Status

Open

## RFC

RFC-0111 (Numeric): Deterministic DECIMAL

## Summary

Implement comprehensive test vectors for DECIMAL and integrate 57-entry verification probe per RFC-0111 §Verification Probe.

## Acceptance Criteria

- [ ] Unit tests for all 57 probe entries (ADD, SUB, MUL, DIV, SQRT, ROUND, CANONICALIZE, CMP, SERIALIZE, DESERIALIZE, TO_DQA, FROM_DQA)
- [ ] Fuzz testing against Python reference implementation
- [ ] 57-entry verification probe with SHA256 leaf hashes
- [ ] Merkle root verification: `6e34054d69d697c9a6a65f0ed1fd3a8fcfd7f8b28b86e5c97c4b05c9f5e6b5a`
- [ ] Boundary tests: MAX_DECIMAL_MANTISSA, overflow cases
- [ ] RoundHalfEven rounding tests (even/odd tie-breaking)
- [ ] SQRT 40-iteration verification
- [ ] Precision Growth Control tests (scale ≤ min(36, max+6))

## Dependencies

- Mission 0111-decimal-arithmetic (must complete first)
- Mission 0111-decimal-serialization (must complete first)

## Location

`determin/src/decimal.rs` tests + `scripts/compute_decimal_probe_root.py`

## Complexity

Medium — comprehensive test coverage required

## Reference

- RFC-0111 §Verification Probe (57 entries)
- RFC-0111 §Probe Entries table
- RFC-0111 §Determinism Rules
