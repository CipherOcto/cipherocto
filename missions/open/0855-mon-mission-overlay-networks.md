# Mission: MON Mission Overlay Networks

## Status

Open

## RFC

RFC-0855: Mission Overlay Networks (MON)

## Summary

Implement mission overlay networks with mission identity, lifecycle state machine (8 states), membership roles, topology models, key hierarchy derivation, and integration with BLUEPRINT's existing mission system.

## Acceptance Criteria

- [x] `MissionId` with network_id, mission_hash, version (RFC §2.1)
- [x] `MissionDescriptor` with mission_id, descriptor_version, mission_type, creation_epoch, governance_model, cryptographic_suite, mission_root, max_participants, min_participants, ttl_epochs, flags (RFC §2.2)
- [x] `MissionState` enum: Created, Discovering, Forming, Active, Degraded, Recovering, Terminated, Archived
- [x] `AdmissionPolicy` enum: Open, InviteOnly, StakeGated, TrustGated, CapabilityGated (RFC §4.3)
- [x] `TopologyModel` enum: Mesh, Hierarchical, Star, Swarm, Ring, Hybrid
- [x] `TopologyCommitment` struct: Merkle root of gateway sequence for deterministic replay
- [x] `MissionKeyHierarchy` with mission_root_key, transport_keys_root, relay_keys_root, execution_keys_root
- [x] Mission lifecycle state machine: Created → Discovering → Forming → Active → Degraded → Recovering → Terminated → Archived
- [x] State transitions require 2/3 majority voting (Coordinator proposes)
- [x] `MissionNode` with peer_id, role_flags, trust_score, capability_root, join_epoch
- [x] 8 roles: Coordinator, Executor, Relay, Validator, Observer, Archivist, Prover, Aggregator
- [x] Role escalation prevention: Observer→Executor requires Coordinator, any→Coordinator requires 2/3 vote
- [x] Topology models: Mesh, Hierarchical, Star, Swarm, Ring, Hybrid with minimum participants
- [x] `mission_genesis_secret` derivation: HKDF-BLAKE3(secret=creator_private_key, salt=mission_id.mission_hash, info="mission-genesis-secret") (RFC §7.1)
- [x] Mission naming disambiguation from BLUEPRINT missions (section in RFC)
- [x] `MonError` enum with all error variants
- [x] Unit tests: 54 tests covering lifecycle transitions, role enforcement, key derivation, governance, topology, membership, reconciliation
- [x] `cargo fmt -- --check` passes
- [x] `cargo test -p octo-network` passes (617 tests)

## Location

`crates/octo-network/src/mon/`

## Complexity

Very High

## Prerequisites

- Mission 0850: DOT Core Envelope and Native P2P
- Mission 0851: GDP Gateway Discovery
- Mission 0852: DGP Deterministic Gossip
- Mission 0853: OCrypt Overlay Cryptography

## Implementation Notes

- See `docs/07-developers/networking-implementation-guide.md` for concrete Rust code
- MON "missions" are overlay coordination events — NOT BLUEPRINT implementation missions
- Minimum participants: Mesh=2, Hierarchical=3, Star=2, Swarm=5, Ring=3, Hybrid=2
- Key hierarchy uses HKDF-BLAKE3 for all derivation
- Topology Merkle commitment for deterministic replay

## Reference

- RFC-0855: Mission Overlay Networks (§2, §3, §4, §5, §7)
- `docs/07-developers/networking-implementation-guide.md` (Module Tree)
- `docs/BLUEPRINT.md` (existing mission system for disambiguation)
