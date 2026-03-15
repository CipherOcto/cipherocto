# Mission: BigInt Verification Probe

## Status
Open

## RFC
RFC-0110 (Numeric): Deterministic BIGINT

## Summary
Implement 56-entry Merkle verification probe for BigInt with deterministic encoding.

## Acceptance Criteria
- [ ] Implement probe encoding matching RFC spec (8-byte compact encoding)
- [ ] Encode all 56 probe entries with correct operation IDs
- [ ] Build Merkle tree from entries using SHA-256
- [ ] Verify Merkle root matches: c447fa82db0763435c1a18268843300c2ed811e21fcb400b18c75e579ddac7c0
- [ ] Two-input probe verification procedure
- [ ] One-input probe verification procedure

## Location
`stoolap/src/numeric/bigint_probe.rs` (new file)

## Complexity
Medium

## Prerequisites
- Mission 0110-bigint-conversions-serialization

## Implementation Notes
- Probe format: [op_id: u64, input_a: 8 bytes, input_b: 8 bytes]
- Compact encoding for values ≤ 2^56: bytes 0-6 little-endian, byte 7 = 0x00 (positive) or 0x80 (negative)
- Hash reference for large values: lower 8 bytes of SHA-256
- Special sentinels: MAX = 0xFFFF_FFFF_FFFF_FFFF, TRAP = 0xDEAD_DEAD_DEAD_DEAD
- Merkle tree: pairwise SHA-256 of hashes

## Reference
- RFC-0110: Deterministic BIGINT (§Verification Probe)
- scripts/compute_bigint_probe_root.py
