# Mission: 0850p-c — Transport Group Binding (Base)

## Status

Claimed (2026-06-17)

## RFC

RFC-0850p-c (Networking): Transport Group Binding Ceremony — `rfcs/accepted/networking/0850p-c-transport-group-binding.md`

> This base mission is referenced by RFC-0850p-c Dependencies §1:
> "Phase 1 base mission `0850p-c-base.md` (TBD; not yet created) will declare 0850, 0855, 0855p-b, 0850p-a, 0851p-a as prerequisites."

## Summary

Coordinate the implementation of RFC-0850p-c across the `octo-network` and `octo-adapter-*` crates. This mission is the entry point for the binding ceremony; it declares the prerequisite chain, lists the RFC types that the binding module must expose, and tracks the per-sub-mission ownership of types. Sub-missions (`0850p-c-cross-node-rebind`, `0850p-c-cross-platform-witness`, `0850p-c-libp2p-propagation`, `0850p-c-partial-bindings`, plus the new `0850p-d-dc-initiated-group-creation` mission) inherit the prerequisites declared here.

## Dependencies

**Prerequisites (RFCs that must be Accepted before this mission is claimed):**

- RFC-0850 (Networking): Deterministic Overlay Transport — `DeterministicEnvelope`, `DOT/1/*` versioning
- RFC-0855 (Networking): Mission Overlay Networks — `mission_id`, `MissionDescriptor`, mission lifecycle
- RFC-0855p-b (Networking): Mission Coordinator Lifecycle — `CoordinatorLifecycle` and `CoordinatorRecord` (DomainCoordinator reuses these)
- RFC-0850p-a (Networking): WhatsApp Auth Onboarding — `BotLifecycle` and `GroupConfig` (the operator-side config that lists groups)
- RFC-0851p-a (Networking): Network Bootstrap Protocol — a node must be bootstrapped into the mesh before participating in a binding ceremony (per 2026-06-16 batch review MR-2: moved from Optional to Required)

**Sub-missions (this base mission tracks and inherits to):**

- `missions/open/0850p-c-cross-node-rebind.md` — Future Work F1 in RFC-0850p-c
- `missions/open/0850p-c-cross-platform-witness.md` — Future Work F5 in RFC-0850p-c
- `missions/open/0850p-c-libp2p-propagation.md` — Future Work F3 in RFC-0850p-c
- `missions/open/0850p-c-partial-bindings.md` — Future Work F2 in RFC-0850p-c
- `missions/open/0850p-d-dc-initiated-group-creation.md` — DC-initiated group creation & invite (companion to new RFC-0850p-d)

## Acceptance Criteria

### Phase 1: Type definitions and serialization

- [ ] `GroupState` enum (Unbound=0x00, Bound=0x01, ReBinding=0x02, UnboundQuarantined=0x03) in `crates/octo-network/src/dot/binding.rs`
- [ ] `GroupBinding` struct with the explicit field list from RFC-0850p-c §1 (group_jid, platform, mission_id, domain_id, domain_coordinator_id, bound_at_epoch, renewed_at_epoch, state, binding_hash)
- [ ] `BindEnvelope`, `BindAck`, `UnbindEnvelope`, `RebindEnvelope`, `PlatformLossEnvelope` (from RFC-0855p-c §5a) in `crates/octo-network/src/dot/binding.rs`
- [ ] `UnbindAuthority` enum (CoordinatorResign, SlashVote, MissionTerminated) in same module
- [ ] DCS (RFC-0126) canonical serialization for all envelope types, including the 10-byte canonical header (`envelope_type || envelope_subtype || version`)
- [ ] `binding_hash = BLAKE3-256(header || body)` with explicit field lists per R1-TGB-2, R4-1, R4-2, R4-3 fixes
- [ ] `is_reconnect: bool` field on `BindEnvelope` (R3-6 fix) and `bind_hash` includes it (R3-1 fix)
- [ ] Unit tests: round-trip serialization for each envelope type; BLAKE3-collision test for `bind_hash`; rejected mutate-after-signing test

### Phase 2: GroupRegistry

- [ ] `GroupRegistry` struct with `bindings: BTreeMap<(String, String), GroupBinding>` and `domain_index: BTreeMap<([u8; 32], [u8; 32], String), (String, String)>` in `crates/octo-network/src/dot/group_registry.rs`
- [ ] `register_binding(binding: GroupBinding) -> Result<(), BindingError>` — checks for multi-platform-rule violation per RFC-0850p-c §5 (1 group per platform per domain_id)
- [ ] `lookup_by_group(platform, group_jid) -> Option<GroupBinding>` and `lookup_by_domain(mission_id, domain_id, platform) -> Option<GroupBinding>` (reverse lookup)
- [ ] State transition helpers: `transition_to_bound`, `transition_to_rebinding`, `transition_to_unbound_quarantined`, `transition_to_unbound` (per RFC-0850p-c §1 transition table)
- [ ] Unit tests: each transition path; multi-platform rule enforcement; reverse-lookup correctness

### Phase 3: Witness validation pipeline

- [ ] Witness validation rules 1-10 from RFC-0850p-c §8 "Witness Validation Rules" in `crates/octo-network/src/dot/witness.rs`
- [ ] Cross-platform spoof check (rule #3) — adapter MUST reject `BIND.platform` that does not match the adapter's own platform string
- [ ] Nonce-replay table (R2-TGB-1, R3-4, R3-7 fix) with `NonceReplayTable::check_and_maybe_evict` and `record` methods, `&mut self` signature
- [ ] First-BIND-wins rule (R3-9, R4-7 fix) — lexicographic comparison on `bind_hash`; on equal `peer_id`
- [ ] Reconnect split-brain check (R2-DC-3, R3-1, R3-6 fix) — `is_reconnect: true` rejected if a different `coordinator_id` is currently `Active`
- [ ] `is_reconnect_lie` slash (reason 0x000B) on rule violation, per E2E IS-1.6 fix

### Phase 4: Binding ceremony

- [ ] Implicit designator path: first-DOT-sender self-designates; 3-way race tiebreaker by lowest `peer_id` lexicographically (E2E IS-3.2 fix)
- [ ] Explicit founder BIND path: `MissionCreator` authority; 4 explicit eligibility checks per E2E IS-3.3 fix
- [ ] Founder squat detection: `FOUNDER_HEARTBEAT_GRACE = 30` epochs; missing heartbeat triggers slash 0x0003 (E2E IS-1.5 fix)
- [ ] `BIND_WITNESS_TIMEOUT = 100` epochs; 3 retries with backoff `50, 200, 800` epochs (E2E IS-1.3 fix)
- [ ] `BIND_ACK` aggregation (`>= 1 ACK` to confirm `Bound`)

### Phase 5: Integration with WhatsApp and Matrix adapters

- [ ] Hook BIND on first DOT in `octo-adapter-whatsapp/src/adapter.rs` (per RFC-0850p-c Key Files)
- [ ] Same hook in `octo-adapter-matrix/src/lib.rs` and `octo-adapter-telegram/src/lib.rs`
- [ ] `GroupRegistry` is shared across adapters (one registry per node, not per adapter)
- [ ] Integration test: 2-node WhatsApp group, end-to-end BIND → BIND_ACK → Bound transition

### Quality gates

- [ ] `cargo clippy --all-targets --all-features -- -D warnings` passes
- [ ] `cargo fmt -- --check` passes
- [ ] `cargo test -p octo-network` passes
- [ ] No regression in `octo-adapter-whatsapp`, `octo-adapter-matrix`, `octo-adapter-telegram` existing tests

## Type Coverage

| RFC-0850p-c Type | Implemented By |
|------------------|----------------|
| `GroupState` enum | This mission |
| `GroupBinding` struct | This mission |
| `BindEnvelope` | This mission |
| `BindAck` | This mission |
| `UnbindEnvelope` | This mission |
| `UnbindAuthority` | This mission |
| `RebindEnvelope` | This mission |
| `GroupRegistry` | This mission |
| `NonceReplayTable` | This mission |
| Implicit designator ceremony | This mission |
| Explicit founder BIND | This mission |
| Cross-node REBIND atomicity (F1) | Sub-mission `0850p-c-cross-node-rebind.md` |
| Partial bindings (F2) | Sub-mission `0850p-c-partial-bindings.md` |
| libp2p BIND propagation (F3) | Sub-mission `0850p-c-libp2p-propagation.md` |
| Cross-platform witness aggregation (F5) | Sub-mission `0850p-c-cross-platform-witness.md` |
| DC-initiated group creation & invite | Sub-mission `0850p-d-dc-initiated-group-creation.md` |

## Location

- `crates/octo-network/src/dot/binding.rs` (new)
- `crates/octo-network/src/dot/group_registry.rs` (new)
- `crates/octo-network/src/dot/witness.rs` (new)
- `crates/octo-adapter-whatsapp/src/adapter.rs` (additive: BIND hook)
- `crates/octo-adapter-matrix/src/lib.rs` (additive: BIND hook)
- `crates/octo-adapter-telegram/src/lib.rs` (additive: BIND hook)

## Complexity

High (~1500 lines; envelope types, GroupRegistry, witness validation pipeline, ceremony state machines, three adapter integrations, integration test).

## Prerequisites

- RFC-0850 status: Draft (implementation has begun per CLAUDE.md; binding can proceed in parallel per the 0850p-a pattern in `missions/claimed/0850p-a-whatsapp-auth-onboarding.md`)
- RFC-0855 status: Accepted
- RFC-0855p-b status: Accepted
- RFC-0850p-a status: Accepted
- RFC-0851p-a status: Accepted
- All Required RFCs are Accepted or in Active Implementation per the RFC maturity rules in the BLUEPRINT mission-creator flow.

## Notes

### Why a base mission?

RFC-0850p-c declares 6 envelope types, 4 GroupState transitions, 10 witness validation rules, 1 NonceReplayTable, and 3 adapter integrations. Per the BLUEPRINT "Multi-Mission Decomposition" rule ("When an RFC has 10+ types, 4+ phases, or 1000+ lines of specification, decompose into multiple missions"), the work is split into:
- **Base mission (this one):** Core types, GroupRegistry, witness pipeline, ceremony state machines, adapter hooks — the parts that all sub-missions depend on
- **Sub-missions:** Future Work items (F1-F5) and the new DC-initiated group creation flow (RFC-0850p-d)

### Why is the RFC still Draft while this base mission is Open?

Per `missions/claimed/0850p-a-whatsapp-auth-onboarding.md` ("RFC Status" section): "implementation has proceeded in parallel with RFC maturation." The 0850h mission is `Implemented` while RFC-0850 is still `Draft`; the 0850ab-a mission is `Claimed` while RFC-0850ab-a is `Accepted`. This base mission follows the same pattern: the RFC is mature (Accepted as of 2026-06-16) and the base mission is now Open for claim.

### Cross-RFC consistency

The `BindEnvelope.is_reconnect: bool` field is a pre-1.0 spec change tracked in RFC-0850p-c's changelog. The field is part of `bind_hash` (R3-1 fix) so it cannot be mutated post-signing. This base mission MUST include the `is_reconnect` field in the `BindEnvelope` type definition; do NOT omit it for backward-compat reasons.

## Claimant

@mmacedoeu (agent-assisted)

## Pull Request

(none — Open mission)
