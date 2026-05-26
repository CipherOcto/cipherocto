# Mission: DOT Core Envelope and Native P2P

## Status

Open

## RFC

RFC-0850: Deterministic Overlay Transport (DOT)

## Summary

Implement the deterministic envelope format, broadcast domain IDs, overlay sequences, replay cache, and NativeP2P platform adapter. This is the foundation all other networking missions depend on.

## Acceptance Criteria

- [ ] `BroadcastDomainId` with BLAKE3-256 hashing and canonical serialization
- [ ] `DeterministicEnvelope` with derive_envelope_id, to_signing_bytes, verify
- [ ] `OverlaySequence` with deterministic ordering (epoch, counter, gateway)
- [ ] `ReplayCache` with BTreeMap, deterministic eviction, configurable window
- [ ] `PlatformAdapter` trait with async send/receive/canonicalize
- [ ] `NativeP2P` adapter implementation using libp2p gossipsub
- [ ] `DotError` enum with all error variants (InvalidSignature, ReplayDetected, InvalidEnvelopeId, CanonicalizationFailed, EnvelopeTooLarge, UnsupportedVersion, TtlExpired, etc.)
- [ ] `CanonicalEvent` struct for cross-platform event normalization (MessageType, payload, metadata)
- [ ] `EnvelopeFlags` enum with ENCRYPTED, FRAGMENTED, MISSION_SCOPED, ROUTE_TRACE_PRESENT
- [ ] Unit tests: 15+ tests covering determinism, serialization, replay detection
- [ ] Integration test: envelope round-trip through NativeP2P adapter
- [ ] `cargo fmt -- --check` passes
- [ ] `cargo test -p octo-network` passes

## Location

`crates/octo-network/src/dot/`

## Complexity

High

## Prerequisites

None (foundation mission)

## Implementation Notes

- See `docs/07-developers/networking-implementation-guide.md` for concrete Rust code
- Use `blake3` crate for all hashing (not SHA-256)
- Use `ed25519-dalek` for signature verification
- Use `BTreeMap` for replay cache (deterministic iteration order)
- All struct fields use big-endian serialization for canonical bytes
- Extend existing `crates/octo-network/src/network.rs` with `DotGateway` struct

## Reference

- RFC-0850: Deterministic Overlay Transport (§3, §4, §5, §9)
- `docs/07-developers/networking-implementation-guide.md` (Module Tree, Error Types, Core Types)
- `crates/octo-core/src/identity.rs` (existing Identity struct to extend)
