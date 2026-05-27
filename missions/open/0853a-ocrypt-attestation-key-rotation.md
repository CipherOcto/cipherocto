# Mission: OCrypt Gateway Attestation and Key Rotation

## Status

Open

## RFC

RFC-0853: Overlay Cryptography (OCrypt) — §9, §12

## Summary

Implement gateway attestation (signed capability proofs), key rotation with backward-compatible re-keying, and revocation mechanisms.

## Acceptance Criteria

- [ ] `GatewayAttestation` with gateway_id, attestation_type, payload_root, timestamp, signature
- [ ] Attestation verification against gateway public key
- [ ] Key rotation: new key signs old key for backward compatibility
- [ ] Re-keying: mission keys rotated with new epoch
- [ ] Key revocation: signed revocation notice propagated via GDP; key revocation propagation uses DGP gossip (Mission 0852) when available
- [ ] Revocation takes effect within configurable grace period
- [ ] Integration with GDP (RFC-0851) for attestation propagation
- [ ] Unit tests: 10+ tests covering attestation, rotation, revocation, grace period
- [ ] `cargo fmt -- --check` passes
- [ ] `cargo test -p octo-network` passes

## Location

`crates/octo-network/src/ocrypt/attestation.rs` and `crates/octo-network/src/ocrypt/key_rotation.rs`

## Complexity

High

## Prerequisites

- Mission 0853: OCrypt Overlay Cryptography
- Mission 0851: GDP Gateway Discovery

## Implementation Notes

- Attestation is a signed proof of capabilities (not just a claim)
- Key rotation preserves backward compatibility (old messages still decryptable)
- Revocation requires signed notice to prevent unauthorized revocation
- Grace period allows transition without service interruption

## Reference

- RFC-0853 §9: Gateway Attestation
- RFC-0853 §12: Key Rotation and Revocation
