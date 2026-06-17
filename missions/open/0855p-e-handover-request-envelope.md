# Mission: 0855p-e — HandoverRequest Envelope & Coordinator Term Handover

## Status

Open (2026-06-17) — early stage; main spec pending RFC elaboration

## RFC

RFC-0855p-e (Networking): HandoverRequest Envelope & Coordinator Term Handover — `rfcs/draft/networking/0855p-e-handover-request-envelope.md`

## Summary

Implement the `DOT/1/HANDOVER_REQUEST` envelope and the coordinator term handover ceremony outlined in RFC-0855p-e. A coordinator (MissionCoordinator, DomainCoordinator, or WitnessCoordinator) may initiate a handover to a successor via this signed envelope. The handover includes the current term, slash tally, group bindings, and pending envelopes. Closes scenario family S-C4 (coordinator term handover with full state) from the gap analysis. Builds on RFC-0855p-b §5a (slash tally) and RFC-0855p-c (DC authority).

> **Status note:** RFC-0855p-e is currently an early-stage draft (Version 0.1). The main spec — slash tally serialization, group binding transfer, witness ACK aggregation, handover race handling, handover revocation, quorum semantics — is to be elaborated in the next RFC iteration. This mission will be expanded once the RFC is more mature. For now, the mission only covers the basic `HandoverRequestEnvelope`, `HandoverAckEnvelope`, and `HandoverDoneEnvelope` types.

## Dependencies

**Prerequisites:**

- RFC-0850 (Networking): Deterministic Overlay Transport
- RFC-0855p-b (Networking): Mission Coordinator Lifecycle — `CoordinatorLifecycle::Handover`, slash tally
- RFC-0855p-c (Networking): DomainCoordinator Role — DC authority
- RFC-0850p-c (Networking): Transport Group Binding Ceremony — `GroupBinding`
- RFC-0126 (Numeric): DCS

## Acceptance Criteria (preliminary)

### Phase 1: Envelope types

- [ ] `HandoverRequestEnvelope` (subtype 0x30) in `crates/octo-network/src/dot/binding.rs` with `coordinator_id`, `successor_id`, `coordinator_role: CoordinatorRole`, `current_term_id`, `new_term_id`, `slash_tally: SlashTally`, `group_bindings: Vec<GroupBinding>`, `pending_envelopes_hash: [u8; 32]`, `reason: HandoverReason`, `nonce`, `current_epoch`, `signature`
- [ ] `HandoverAckEnvelope` (subtype 0x31) with the witness signature
- [ ] `HandoverDoneEnvelope` (subtype 0x32) with the new coordinator's confirmation
- [ ] `HandoverReason` enum: `Voluntary = 0x00`, `Scheduled = 0x01`, `Suspect = 0x02`, `Demoting = 0x03`, `MissionTerminated = 0x04`
- [ ] DCS serialization for all envelope types; round-trip byte equality test
- [ ] Unit tests: signature verification, nonce uniqueness

### Phase 2: CoordinatorLifecycle::Handover transition (pending RFC elaboration)

- [ ] `Active → Handover` transition when the coordinator signs a `HANDOVER_REQUEST`
- [ ] `Handover → Active (new coordinator)` transition when the new coordinator signs a `HANDOVER_DONE`
- [ ] `Handover → Active (old coordinator restored)` transition on handover revocation
- [ ] Slash tally transfer: the new coordinator inherits the old coordinator's slashes

### Phase 3: Group binding transfer (pending RFC elaboration)

- [ ] The new coordinator takes ownership of all `GroupBinding`s from the old coordinator
- [ ] `GroupRegistry` updates: `coordinator_id` field changes from old to new
- [ ] Witnesses see the new `coordinator_id` in the BIND envelopes

### Phase 4: Witness ACK aggregation (pending RFC elaboration)

- [ ] Define the quorum for `HANDOVER_REQUEST` (≥ 1 witness ACK vs. N-of-M)
- [ ] `HANDOVER_ACK` is broadcast to the mesh; aggregated by the old coordinator

### Phase 5: Handover race handling (pending RFC elaboration)

- [ ] If two coordinators HANDOVER_REQUEST simultaneously, the lexicographic comparison on `coordinator_id` is the tiebreak

### Phase 6: Handover revocation (pending RFC elaboration)

- [ ] If the new coordinator is slashed within `HANDOVER_REVOCATION_WINDOW = 100` epochs, the old coordinator is restored
- [ ] Specify the revocation envelope and quorum

### Quality gates

- [ ] `cargo clippy --all-targets --all-features -- -D warnings` passes
- [ ] `cargo fmt -- --check` passes
- [ ] `cargo test -p octo-network` passes
- [ ] No regression in `0855p-b-coordinator-lifecycle.md` mission's existing tests

## Location

- `crates/octo-network/src/dot/binding.rs` (additive: 3 new envelope types + `HandoverReason` enum)
- `crates/octo-network/src/dot/coordinator.rs` (new) — coordinator term handover state machine
- `crates/octo-network/src/dot/group_registry.rs` (additive: `coordinator_id` field update on handover)
- `crates/octo-network/src/dot/slash.rs` (additive: `SlashTally` DCS serialization)

## Complexity

Medium (~700 lines; 3 envelope types, state machine transitions, group binding transfer, witness aggregation). Most of the work is in the RFC elaboration (slash tally serialization, handover race, revocation), not the implementation.

## Prerequisites

- Mission `0855p-b-coordinator-lifecycle.md` (claimed) — sister mission; the `CoordinatorLifecycle::Handover` state is defined there
- RFC-0855p-e status: Draft (early stage)

## Notes

### Why is this mission "preliminary"?

RFC-0855p-e is in early-stage draft (Version 0.1). The main spec — slash tally serialization, group binding transfer, witness ACK aggregation, handover race handling, handover revocation, quorum semantics — is to be elaborated in the next RFC iteration. This mission tracks the basic `HandoverRequestEnvelope`, `HandoverAckEnvelope`, and `HandoverDoneEnvelope` types (which are small additive changes) and reserves a slot for the slash tally serialization and group binding transfer logic that will be added when the RFC matures.

### Cross-RFC dependencies

This mission depends on `0855p-b-coordinator-lifecycle.md` (the base coordinator lifecycle mission) for the `CoordinatorLifecycle` enum and the `Handover` state. The two missions should be coordinated.

### Why is the slash tally important?

Per RFC-0855p-b §5a, the slash tally is a per-coordinator record of slashes issued against nodes in the coordinator's domain. When a coordinator hands over, the slash tally MUST be transferred to the successor; otherwise, the slashes are lost and the system loses accountability. The `SlashTally` DCS serialization (F-1) is a key piece of the handover protocol.

## Claimant

(none — Open mission)

## Pull Request

(none — Open mission)
