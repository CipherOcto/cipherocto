# Mission: DOT Privacy and Encryption

## Status

Implemented (1717 lines, 75 tests across attestation, envelope, identity, mission, onion, randomness, session, suite)

## RFC

RFC-0850: Deterministic Overlay Transport (DOT) — §10

## Summary

Implement end-to-end encryption for DOT envelopes, metadata minimization, and transport obfuscation so platforms cannot access plaintext mission data.

## Acceptance Criteria

- [ ] Envelope payload encryption using OCrypt (RFC-0853) session keys
- [ ] Platforms observe only ciphertext and relay metadata
- [ ] Metadata minimization: minimize leakage of topology, routing intent, mission structure, peer graph
- [ ] Transport obfuscation: payloads appear opaque to carrier platforms
- [ ] Integration with OCrypt (RFC-0853) for key management
- [ ] Encryption is optional per envelope (flags field controls)
- [ ] Unit tests: 6+ tests covering encryption round-trip, metadata isolation
- [ ] `cargo fmt -- --check` passes
- [ ] `cargo test -p octo-network` passes

## Location

`crates/octo-network/src/dot/envelope.rs` (encryption extensions)

## Complexity

Medium

## Prerequisites

- Mission 0850: DOT Core Envelope and Native P2P
- Mission 0853: OCrypt Overlay Cryptography

## Implementation Notes

- Encryption happens at the envelope level, not the platform level
- Platforms see: encrypted payload + DOT header (version, network_id, message_type)
- Mission data is NEVER plaintext on the carrier platform
- Metadata minimization is a SHOULD, not MUST (performance tradeoff)

## Reference

- RFC-0850 §10: Privacy and Encryption
