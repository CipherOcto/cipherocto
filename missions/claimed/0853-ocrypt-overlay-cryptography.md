# Mission: OCrypt Overlay Cryptography

## Status

Claimed

## RFC

RFC-0853: Overlay Cryptography (OCrypt)

## Summary

Implement the cryptographic layer with CryptoSuiteId, sovereign identity extension, session handshake (X25519 + HKDF-BLAKE3), mission key hierarchy, envelope encryption, and consensus boundary enforcement.

## Acceptance Criteria

- [ ] `CryptoSuiteId` with hash_id, signature_id, kdf_id, aead_id, kex_id
- [ ] Implement `OverlayIdentity` with peer_id, public_key, identity_epoch, capabilities_root, signature
- [ ] PlatformBinding with platform_type, external_identifier_hash, proof_signature
- [ ] Session handshake: X25519 → HKDF-BLAKE3 → ChaCha20-Poly1305
- [ ] Forward secrecy via ephemeral per-message keys
- [ ] Mission key hierarchy: mission_root_key, transport_keys_root, relay_keys_root, execution_keys_root
- [ ] Envelope encryption/decryption with canonicalization before encryption
- [ ] `CryptoError` enum with ConsensusBoundaryViolation variant
- [ ] Unit tests: 15+ tests covering key derivation, encryption round-trip, nonce uniqueness
- [ ] `cargo fmt -- --check` passes
- [ ] `cargo test -p octo-network` passes

## Claimant

@agent (Jcode)

## Location

`crates/octo-network/src/ocrypt/`

## Complexity

Very High

## Prerequisites

- Mission 0850: DOT Core Envelope and Native P2P

## Implementation Notes

- See `docs/07-developers/networking-implementation-guide.md` for concrete Rust code
- Use `x25519-dalek` for key exchange, `chacha20poly1305` for AEAD, `hkdf` for KDF
- Nonce MUST be derived via HKDF-BLAKE3(session_key, "ocrypt:nonce:v1", ...)[0..24] — NEVER zero
- Plaintext canonicalization happens BEFORE encryption (consensus boundary)
- CryptoError::ConsensusBoundaryViolation enforces RFC-0008 Class A/B/C separation

## Reference

- RFC-0853: Overlay Cryptography (§1, §3, §4, §5, §6, §7, §13, §14)
- `docs/07-developers/networking-implementation-guide.md` (Error Types, Cargo Dependencies)
- `crates/octo-core/src/identity.rs` (existing Identity to extend)
