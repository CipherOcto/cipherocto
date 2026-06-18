# Mission: DOT Gateway Federation

## Status

Implemented (375 lines, 12 tests, FederationState, partition handling)

## RFC

RFC-0850: Deterministic Overlay Transport (DOT) — §6, §7

## Summary

Implement gateway federation with multi-homing (multiple broadcast domains per gateway), overlay route graph, gateway capacity declaration, and deterministic route computation.

## Acceptance Criteria

- [ ] `GatewayCapacity` struct with max_throughput, domain_count, platform_mask, storage_class, bandwidth_class
- [ ] Multi-homing: gateway connects to multiple broadcast domains simultaneously
- [ ] Overlay route graph: Domain → Edge Gateway → DOT Mesh → Edge Gateway → Domain
- [ ] `RouteCommitment` with BLAKE3-256(gateway_sequence_hash || weights_hash || epoch)
- [ ] Deterministic route computation from (mission_id, destination_peer, network_epoch, gateway_weights)
- [ ] Routes MUST NOT depend on latency, local heuristics, wall-clock, CPU load
- [ ] Platform partition handling: automatic rerouting through remaining carriers
- [ ] Gateway replacement via GDP (RFC-0851) when gateway fails
- [ ] Self-loop prevention: each adapter provides `self_handle()` — gateway drops messages from itself to prevent relay loops
- [ ] Unit tests: 10+ tests covering multi-homing, route computation, partition handling, self-loop prevention
- [ ] `cargo fmt -- --check` passes
- [ ] `cargo test -p octo-network` passes

## Location

`crates/octo-network/src/dot/gateway.rs`, `crates/octo-network/src/dot/route.rs`

## Complexity

High

## Prerequisites

- Mission 0850: DOT Core Envelope and Native P2P
- Mission 0851: GDP Gateway Discovery

## Implementation Notes

- Logical routing is deterministic; physical routing is non-deterministic
- Route commitment allows replay verification
- Gateway capacity is declared at registration, used for load balancing
- Platform partition = automatic failover to alternate carriers

## Reference

- RFC-0850 §6: Gateway Federation Model
- RFC-0850 §7: Routing Architecture
