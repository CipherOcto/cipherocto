# Mission: OCrypt Onion Relay Extension

## Status

Open

## RFC

RFC-0853: Overlay Cryptography (OCrypt) — §10

## Summary

Implement the onion relay cryptographic extension with per-hop encryption, relay knowledge isolation, and deterministic randomness for nonce generation.

## Acceptance Criteria

- [ ] Per-hop encryption: each relay layer uses distinct session key
- [ ] Relay knowledge isolation: each relay knows only previous/next hop
- [ ] Layered key derivation: X25519 shared secret → HKDF-BLAKE3 → per-hop keys
- [ ] Deterministic randomness: BLAKE3-CTR-drbg for consensus-safe random generation
- [ ] Nonce uniqueness: HKDF-BLAKE3(session_key, "ocrypt:nonce:v1", ...)[0..24]
- [ ] Forward secrecy: compromise of one relay doesn't expose full route
- [ ] Integration with ORR (RFC-0858) for route construction
- [ ] Unit tests: 10+ tests covering layered encryption, relay isolation, nonce uniqueness
- [ ] `cargo fmt -- --check` passes
- [ ] `cargo test -p octo-network` passes

## Location

`crates/octo-network/src/ocrypt/onion.rs`

## Complexity

Very High

## Prerequisites

- Mission 0853: OCrypt Overlay Cryptography

> **Note:** This mission provides the onion encryption primitives that Mission 0858 (ORR) depends on. Mission 0858 should depend on BOTH 0853 AND 0853b.

## Implementation Notes

- Per-hop encryption: encrypt for exit → wrap for middle → wrap for entry
- Relay sees: encrypted_next_hop + encrypted_payload_fragment (one layer)
- Deterministic randomness uses BLAKE3 in CTR mode (not OS random)
- Nonce MUST be unique per message — HKDF derivation ensures this

## Reference

- RFC-0853 §10: Onion Relay Extension
- RFC-0853 §11: Deterministic Randomness
