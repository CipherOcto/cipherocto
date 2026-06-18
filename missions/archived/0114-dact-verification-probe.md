# Mission: DACT Verification Probe

## Status
Completed (2026-03-22)

## RFC
RFC-0114 v2.12 (Numeric): Deterministic Activation Functions (DACT)

## Summary
Implement the DACT Merkle tree verification probe with 16 entries matching the RFC-0114 canonical Merkle root.

## Acceptance Criteria
- [ ] 16 probe entries (indices 0-15) encoding all activation function types
- [ ] Merkle root matches: "4904af886aac5b581fefcf5d275c0753a0f804bc749d47bdd5bed74565c09fce"
- [ ] Entry 0: relu(5.0) → Dqa(5, 0)
- [ ] Entry 1: relu(-5.0) → Dqa(0, 0)
- [ ] Entry 2: relu6(10.0) → Dqa(6, 0)
- [ ] Entry 3: relu6(3.0) → Dqa(3, 0)
- [ ] Entry 4: sigmoid(0.0) → Dqa(5, 1) = 0.5
- [ ] Entry 5: sigmoid(4.0) → Dqa(9804, 4)
- [ ] Entry 6: sigmoid(-4.0) → Dqa(195, 4)
- [ ] Entry 7: tanh(0.0) → Dqa(0, 0)
- [ ] Entry 8: tanh(2.0) → Dqa(9649, 4)
- [ ] Entry 9: tanh(-2.0) → Dqa(-9649, 4)
- [ ] Entry 10: leaky_relu(-1.0) → Dqa(-1, 2)
- [ ] Entry 11: leaky_relu(1.0) → Dqa(1, 0)
- [ ] Entry 12: First 4 sigmoid LUT entries (raw Q8.8 bytes, 8 bytes)
- [ ] Entry 13: First 4 tanh LUT entries (raw Q8.8 bytes, 8 bytes)
- [ ] Entry 14: Normalization invariant Dqa(1234, 2) = 12.34
- [ ] Entry 15: TRAP sentinel Dqa(-2^63, 0xFF)
- [ ] Pairwise Merkle tree construction (SHA256 of individual leaves, then pairs)

## Dependencies
- Mission 0114-dact-core-type (completed)
- Mission 0114-dact-sigmoid-tanh-lut (completed)

## Location
`determin/src/probe.rs`

## Complexity
Low — fixed verification structure

## Implementation Notes
- Entry 12/13: Raw Q8.8 i16 big-endian serialization (2 bytes per entry, 4 entries = 8 bytes)
- All DQA values must be canonical per RFC-0105 before serialization
- DQA serialization: value.to_bytes(8, "big", signed=True) + bytes([scale]) + bytes(7)
- TRAP serialization: serialize_dqa(-(1 << 63), 0xFF)
- Merkle: if odd number at any level, duplicate last node (RFC-0113 convention)

## Reference
- RFC-0114 §Verification Probe
- RFC-0114 §Merkle Root
- RFC-0114 §Probe Leaf Hashes
- RFC-0114 §Serialization
