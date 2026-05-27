# Mission: ORR Multi-Transport Paths and Route Rotation

## Status

Open

## RFC

RFC-0858: Onion Relay Routing (ORR) — §5, §9

## Summary

Implement multi-transport onion paths (Telegram → Matrix → QUIC → Bluetooth) and automatic route rotation for privacy preservation.

## Acceptance Criteria

- [ ] Multi-transport onion paths: each hop can use different transport carrier
- [ ] `TransportVector` struct (per RFC-0858 §5.2) with transport_type, domain_id, priority, bandwidth_class, censorship_score
- [ ] Path construction: maximize transport diversity across hops
- [ ] Carrier selection: prefer carriers with highest censorship resistance
- [ ] Route rotation: periodic path changes (configurable interval)
- [ ] Rotation trigger: time-based or suspicion-based (per RFC-0858 §9)
- [ ] Seamless rotation: new route established before old route terminated (dual-route handshake)
- [ ] Identity preservation: same peer_id across route changes
- [ ] Integration with DRS (RFC-0856) for path computation
- [ ] Unit tests: 10+ tests covering multi-transport paths, rotation triggers, identity preservation
- [ ] `cargo fmt -- --check` passes
- [ ] `cargo test -p octo-network` passes

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
