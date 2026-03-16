# Mission: DFP Consensus Integration

## Status
Blocked

## RFC
RFC-0104: Deterministic Floating-Point Abstraction

## Summary
Integrate DFP into the consensus layer with Merkle state encoding, replay validation, and fork handling.

## Acceptance Criteria
- [ ] DFP encoding in Merkle state: DfpEncoding serialized to 24 bytes, included in state trie
- [ ] Deterministic view enforcement: CREATE DETERMINISTIC VIEW syntax parsed, view flags stored, query planner enforces DFP-only types
- [ ] Consensus replay validation: On replay, DFP ops re-executed and result hashes compared against stored state
- [ ] **Fork handling**: When two nodes produce different DFP results, network detects divergence within 1 epoch, triggers soft fork (reject blocks with divergent results)
- [ ] **Spec version pinning**: Block header includes `dfp_spec_version: u32`, historical blocks validated against their pinned spec version
- [ ] **Divergence detection latency**: Probe runs every 100,000 blocks; interim divergence detected via Merkle root mismatch on each block (validators verify DFP results before signing)
- [ ] **Replay pinning**: Node binary version tied to DFP spec version; during replay, use spec version from block header to select correct arithmetic behavior

## Location
`src/storage/`, `src/consensus/`

## Complexity
High

## Prerequisites
- Mission 3: DFP Expression VM Opcodes
- Mission 4: DFP Hardware Verification
