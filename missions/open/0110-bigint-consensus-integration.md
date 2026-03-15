# Mission: BigInt Consensus Integration

## Status
Open

## RFC
RFC-0110 (Numeric): Deterministic BIGINT

## Summary
Integrate BigInt into stoolap's consensus layer with Merkle state encoding, replay validation, and spec version pinning.

## Acceptance Criteria
- [ ] BigInt encoding in Merkle state trie
- [ ] Consensus replay validation: re-execute BigInt ops and compare result hashes
- [ ] Fork handling: detect divergent BigInt results within 1 epoch
- [ ] NUMERIC_SPEC_VERSION = 1 constant defined
- [ ] Block header numeric_spec_version integration

## Location
`stoolap/src/storage/`, `stoolap/src/consensus/`

## Complexity
Medium

## Prerequisites
- Mission 0110-bigint-core-algorithms
- Mission 0110-bigint-conversions-serialization

## Implementation Notes
- Use canonical serialization for Merkle hashing
- Follow DFP consensus integration pattern (RFC-0104)
- Probe root verification during state transition

## Reference
- RFC-0110: Deterministic BIGINT (§Consistency)
- missions/claimed/0104-dfp-consensus-integration.md (DFP pattern)
