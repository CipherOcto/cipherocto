# Mission: 0850p-c — BIND propagation via libp2p

## Status

Open (2026-06-16) — post-launch follow-up

## RFC

RFC-0850p-c (Networking): Transport Group Binding — §"Future Work"

## Summary

Currently the BIND envelope is only delivered to members of the physical group (via the platform's group message channel). Nodes that are not yet members of the physical group cannot learn about the binding until they join. For nodes that need to prepare in advance (e.g., pre-fetch mission configuration, validate admission), BIND propagation via the libp2p mesh is needed.

## Design

The BIND envelope is also gossiped on the libp2p mesh using the standard DOT gossip protocol (RFC-0852 "Deterministic Gossip Protocol"). The gossip topic is derived from the `domain_id`:

```rust
let topic = format!("/dot/bind/{}", domain_id.to_base58());
```

Nodes that are not yet in the physical group can subscribe to this topic and receive the BIND envelope as a "pre-admission notification". On receipt, the node:
1. Validates the BIND envelope signature (DomainCoordinator's key)
2. Checks that the `domain_id` is one the node wants to join
3. Optionally initiates platform-group join (if the platform is configured)

The libp2p-delivered BIND is informational; the authoritative BIND is the one delivered via the platform group (which has the DomainCoordinator's actual presence on the platform).

## Acceptance Criteria

- [ ] `BindEnvelope` is gossipable on the libp2p mesh under `/dot/bind/{domain_id}`
- [ ] `crates/octo-network/src/gossip/bind.rs` — bind gossip handler
- [ ] Pre-admission nodes can subscribe without being in the physical group
- [ ] Libp2p-delivered BIND is clearly marked as "informational" (not authoritative)
- [ ] Unit test: BIND gossip reaches a non-member node
- [ ] Integration test: non-member pre-fetches mission config from libp2p-delivered BIND
- [ ] Documentation: how to enable libp2p BIND gossip (off by default for privacy)

## Dependencies

Depends on RFC-0850p-c status: Accepted. Depends on RFC-0852 (Deterministic Gossip Protocol) being Accepted.

## Claimant

(none — Open mission)

## Pull Request

(none — Open mission)

## Location

`crates/octo-network/src/gossip/bind.rs` (new); `crates/octo-network/src/mon/bind_envelope.rs` (add gossip codec).

## Complexity

Medium (~300 lines; gossip codec, pre-admission handler, privacy controls).

## Prerequisites

- RFC-0852 status: Accepted

## Notes

### Why off by default?

Pre-admission nodes may be in a privacy-sensitive location (e.g., a journalist's laptop). Defaulting to off means the operator opts into receiving BIND gossip for specific `domain_id`s.

### Why informational?

The libp2p-delivered BIND is not authoritative. The authoritative BIND is the one delivered via the platform group (where the DC has actual admin status). The libp2p delivery is for pre-fetching only.

### Type Coverage

| RFC-0850p-c Type | Implemented By |
|-----------------|----------------|
| `BindEnvelope` gossipable on libp2p mesh | This mission |
| `/dot/bind/{domain_id}` gossip topic | This mission |
| Pre-admission notification handler | This mission |

### Implementation Guide

Reference: RFC-0852 (gossip protocol); `crates/octo-network/src/gossip/bind.rs` (new).

## Mitigates

Operational scaling; not a security issue (informational, signed by DomainCoordinator).

## Deadline

Post-launch
