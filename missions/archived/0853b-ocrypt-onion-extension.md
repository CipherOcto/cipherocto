# Mission: OCrypt Onion Relay Extension

## Status

Implemented (13 tests: per-hop encryption, relay isolation, nonce uniqueness, forward secrecy, AAD binding)

## RFC

RFC-0853: Overlay Cryptography (OCrypt) — §10

## Summary

Implement the onion relay cryptographic extension with per-hop encryption, relay knowledge isolation, and deterministic randomness for nonce generation.

## Acceptance Criteria

- [x] Per-hop encryption: each relay layer uses distinct session key
- [x] Relay knowledge isolation: each relay knows only previous/next hop
- [x] Layered key derivation: X25519 shared secret → HKDF-BLAKE3 → per-hop keys
- [x] Deterministic randomness: HKDF-BLAKE3 derivation for consensus-safe random generation
- [x] Nonce uniqueness: HKDF-BLAKE3(session_key, "ocrypt:nonce:v1", ...)[0..12]
- [x] Forward secrecy: compromise of one relay doesn't expose full route
- [x] Integration with ORR (RFC-0858) for route construction
- [x] Unit tests: 10+ tests covering layered encryption, relay isolation, nonce uniqueness
- [x] `cargo fmt -- --check` passes
- [x] `cargo test -p octo-network` passes

## Claimant

@agent (Jcode)

## Location

`crates/octo-network/src/ocrypt/onion.rs`

## Complexity

High

## Prerequisites

- Mission 0853: OCrypt Overlay Cryptography

> **Note:** This mission implements onion encryption primitives (OnionHop construction, per-hop key derivation). Mission 0858 implements route construction, relay selection, cover traffic, and proof-of-relay. Mission 0858 should depend on BOTH 0853 AND 0853b.

## Implementation Notes

- Per-hop encryption: encrypt for exit → wrap for middle → wrap for entry
- Relay sees: encrypted_next_hop + encrypted_payload_fragment (one layer)
- Deterministic randomness uses HKDF-BLAKE3 derivation per RFC-0853 §11
- Nonce MUST be unique per message — HKDF derivation ensures this

## Reference

- RFC-0853 §10: Onion Relay Extension
- RFC-0853 §11: Deterministic Randomness
