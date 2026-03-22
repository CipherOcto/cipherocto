# Mission: DACT Consensus Integration (Gas Accounting)

## Status
Completed (2026-03-22)

## RFC
RFC-0114 v2.12 (Numeric): Deterministic Activation Functions (DACT)

## Summary
Integrate DACT operations into the consensus layer with gas accounting per RFC-0114 specification.

## Acceptance Criteria
- [ ] Gas constants defined in consensus.rs:
  - GAS_RELU = 2
  - GAS_RELU6 = 3
  - GAS_LEAKY_RELU = 3
  - GAS_SIGMOID = 10
  - GAS_TANH = 10
- [ ] DACT Operation IDs in consensus.rs (RFC-0114 defines 5 functions)
- [ ] Gas budget proof: 3-layer MLP with 1000 neurons each stays under 50,000 gas per block
- [ ] TRAP propagation: If any input is TRAP → output MUST be TRAP

## Dependencies
- Mission 0114-dact-core-type (completed)
- Mission 0114-dact-sigmoid-tanh-lut (completed)

## Location
`determin/src/consensus.rs`

## Complexity
Low — gas constant definitions

## Implementation Notes
- Gas breakdown for Sigmoid/Tanh (10 total):
  - TRAP check + scale validation: 1
  - normalize_to_scale: 2
  - Domain clamp: 1
  - Index computation: 1
  - LUT lookup: 1
  - Q8.8→DQA conversion: 2
  - Return construction: 2
  - CANONICALIZE: 0
- Per-block allocation per RFC-0110: 50,000 gas
- Typical MLP (3-layer, 1000 neurons each): 30,000 gas

## Reference
- RFC-0114 §Gas Model
- RFC-0114 §Gas Budget Proof
- RFC-0114 §TRAP Invariant
