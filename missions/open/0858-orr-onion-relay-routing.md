# Mission: ORR Onion Relay Routing

## Status

Open

## RFC

RFC-0858: Onion Relay Routing (ORR)

## Summary

Implement privacy-preserving onion routing with layered encryption (ChaCha20-Poly1305), X25519 session key derivation, multi-hop route construction, cover traffic generation, and replay protection.

## Acceptance Criteria

- [ ] `OnionRoute` with route_id, mission_id, route_epoch, hop_count, entry_gateway, exit_gateway, layered_route_root
- [ ] `OnionHop` with relay_gateway, transport_vector_root, encrypted_next_hop, encrypted_payload_fragment
- [ ] Layered encryption: payload → encrypt for exit → wrap for intermediate → wrap for entry
- [ ] Session key derivation: X25519 → HKDF-BLAKE3("orr-session-" || hop_index)
- [ ] Forward secrecy via ephemeral X25519 keys per session
- [ ] Layer peeling: each relay decrypts one layer, sees next hop only
- [ ] Cover traffic generation with configurable ratio (default 20%)
- [ ] Replay protection: (route_id, sequence, logical_timestamp) in authenticated data
- [ ] RFC-0008 execution class mapping table
- [ ] `OrrError` enum with all error variants
- [ ] Unit tests: 12+ tests covering encryption round-trip, layer peeling, forward secrecy
- [ ] `cargo fmt -- --check` passes
- [ ] `cargo test -p octo-network` passes

## Location

`crates/octo-network/src/orr/`

## Complexity

Very High

## Prerequisites

- Mission 0850: DOT Core Envelope and Native P2P
- Mission 0853: OCrypt Overlay Cryptography
- Mission 0856: DRS Deterministic Route Selection

## Implementation Notes

- See `docs/07-developers/networking-implementation-guide.md` for concrete Rust code
- Each relay knows ONLY: previous hop, next hop, local relay instructions
- Route construction via DRS (RFC-0856) for deterministic path selection
- Multi-transport paths: Telegram → Matrix → QUIC → Bluetooth
- Cover traffic is indistinguishable from real traffic

## Reference

- RFC-0858: Onion Relay Routing (§4, §5, §6, §7, §13)
- `docs/07-developers/networking-implementation-guide.md` (Module Tree)
