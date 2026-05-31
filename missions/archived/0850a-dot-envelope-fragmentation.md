# Mission: DOT Envelope Fragmentation

## Status

Implemented (494 lines, 11 tests, fragment_envelope, reassembly)

## RFC

RFC-0850: Deterministic Overlay Transport (DOT) — §9

## Summary

Implement envelope fragmentation and reassembly for platforms with payload size limits (IRC: 512 bytes, LoRa: 256 bytes, Telegram: 4096 bytes). Fragments are self-describing and reassembly is deterministic.

## Acceptance Criteria

- [ ] `EnvelopeFragment` struct with envelope_id, fragment_index, fragment_total, envelope_hash, payload
- [ ] `fragment_envelope()` — splits envelope into platform-appropriate fragments
- [ ] `reassemble_fragments()` — deterministic reassembly by fragment_index order
- [ ] Fragment header size calculation per platform (subtract from max payload)
- [ ] Reassembly timeout with configurable duration
- [ ] Partial reassembly discard on timeout
- [ ] Fragment integrity verification via envelope_hash
- [ ] Unit tests: 8+ tests covering fragmentation, reassembly, timeout, integrity
- [ ] `cargo fmt -- --check` passes
- [ ] `cargo test -p octo-network` passes

## Location

`crates/octo-network/src/dot/fragment.rs`

## Complexity

Medium

## Prerequisites

- Mission 0850: DOT Core Envelope and Native P2P

## Implementation Notes

- Fragments MUST be self-describing (include envelope_id, fragment_index, fragment_total)
- Reassembly concatenates fragment payloads in fragment_index order
- Fragment payloads MUST NOT exceed platform maximum minus fragment header size
- Deterministic reassembly: given identical fragment sets, all nodes produce identical envelope bytes

## Reference

- RFC-0850 §9: Envelope Fragmentation
- `docs/07-developers/networking-implementation-guide.md` (Module Tree)
