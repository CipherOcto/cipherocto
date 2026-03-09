# Mission: DQA Consensus Integration

## Status
Open

## RFC
RFC-0105: Deterministic Quant Arithmetic (DQA)

## Summary
Integrate DQA into stoolap's consensus layer with Merkle state encoding, replay validation, and divergence detection.

## Acceptance Criteria
- [x] DQA encoding in Merkle state: DqaEncoding serialized, included in state trie
- [ ] Deterministic view enforcement: CREATE DETERMINISTIC VIEW syntax for DQA-only queries
- [ ] Consensus replay validation: On replay, DQA ops re-executed and result hashes compared
- [ ] Fork handling: Detect divergent DQA results within 1 epoch
- [x] Spec version pinning: DQA_SPEC_VERSION = 1 constant defined

## Location
`stoolap/src/storage/`, `stoolap/src/consensus/`

## Complexity
Medium

## Prerequisites
- Mission 1: DQA Core Type
- Mission 2: DQA DataType Integration
- Mission 3: DQA Expression VM Opcodes

## Implementation Notes
- Use DQA's canonical serialization for Merkle hashing
- Similar pattern to DFP consensus integration (RFC-0104)
- DQA is simpler than DFP (no special values, fixed range)
- Probe/hardware verification may not be needed for DQA (bounded range)

## Reference
- RFC-0105: Deterministic Quant Arithmetic (§Consistency)
- missions/claimed/0104-dfp-consensus-integration.md (DFP pattern)
