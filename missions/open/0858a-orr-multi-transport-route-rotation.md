# Mission: ORR Multi-Transport Paths and Route Rotation

## Status

Implemented (part of ORR module, 30 total ORR tests)

## RFC

RFC-0858: Onion Relay Routing (ORR) — §5, §9

## Summary

Implement multi-transport onion paths (Telegram → Matrix → QUIC → Bluetooth) and automatic route rotation for privacy preservation.

## Acceptance Criteria

- [x] Multi-transport onion paths: each hop can use different transport carrier
- [x] `TransportVector` struct (per RFC-0858 §5.2) with transport_type, domain_id, priority, bandwidth_class, censorship_score
- [x] Path construction: maximize transport diversity across hops
- [x] Carrier selection: prefer carriers with highest censorship resistance
- [x] Route rotation: periodic path changes (configurable interval)
- [x] Rotation trigger: time-based or suspicion-based (per RFC-0858 §9)
- [x] Seamless rotation: new route established before old route terminated (dual-route handshake)
- [x] Identity preservation: same peer_id across route changes
- [x] Integration with DRS (RFC-0856) for path computation
- [x] Unit tests: 10+ tests covering multi-transport paths, rotation triggers, identity preservation
- [x] `cargo fmt -- --check` passes
- [x] `cargo test -p octo-network` passes

## Claimant

@agent (Jcode)

## Location

`crates/octo-network/src/orr/route.rs` (multi-transport, rotation)

## Complexity

High

## Prerequisites

- Mission 0858: ORR Onion Relay Routing
- Mission 0856: DRS Deterministic Route Selection

## Implementation Notes

- Multi-transport: each hop selects a different carrier type when possible
- Route rotation prevents long-term traffic analysis
- Rotation is seamless: new route established before old one terminates
- Identity preservation: peer_id is constant, only transport changes

## Reference

- RFC-0858 §5: Multi-Transport Onion Paths
- RFC-0858 §9: Route Rotation
