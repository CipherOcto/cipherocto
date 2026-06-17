# Mission: 0850p-f — Transport Group Decommission

## Status

Claimed (2026-06-17) — early stage; main scenarios pending RFC elaboration

## RFC

RFC-0850p-f (Networking): Transport Group Decommission — `rfcs/draft/networking/0850p-f-group-decommission.md`

## Summary

Implement the transport group decommission flow elaborated in RFC-0850p-f. The basic `DOT/1/UNBIND_ALL` envelope is defined in RFC-0850p-d §F and implemented in mission `0850p-d-dc-initiated-group-creation.md`. This mission elaborates the lifecycle (`UnboundAllPending`, `UnboundAllDone`), DC rotation semantics, platform-side leave race handling, audit trail, re-decommission (a node that missed the original UNBIND_ALL), and quorum semantics. Closes scenario family S-G7 (group decommission) from the gap analysis.

> **Status note:** RFC-0850p-f is currently an early-stage draft (Version 0.1). The main spec — quorum semantics, DC rotation, audit trail — is to be elaborated in the next RFC iteration. This mission will be expanded once the RFC is more mature. For now, the mission only covers the basic `UnbindAllEnvelope` and `UnbindAllAckEnvelope` already defined in RFC-0850p-d §F.

## Dependencies

**Prerequisites:**

- RFC-0850p-c (Networking): Transport Group Binding Ceremony — base mission `0850p-c-base.md`
- RFC-0850p-d (Networking): DC-Initiated Transport Group Creation & Invite — `UNBIND_ALL` envelope and authority check
- RFC-0855p-b (Networking): Mission Coordinator Lifecycle — DC rotation
- RFC-0126 (Numeric): DCS

## Acceptance Criteria (preliminary)

### Phase 1: Envelope types (already in RFC-0850p-d §F)

- [x] `UnbindAllEnvelope` (subtype `b"UALL"` per RFC-0850p-d §"Envelope Types Added"; R16 R5-M1 fix — was subtype 0x15 in v1.0; the canonical format is the 4-byte ASCII tag per RFC-0850p-c §A) — defined in RFC-0850p-d, implemented in mission `0850p-d-dc-initiated-group-creation.md`
- [x] `UnbindAllAckEnvelope` (subtype `b"UAAC"` per RFC-0850p-d §"Envelope Types Added"; R16 R5-M1 fix — was subtype 0x16 in v1.0) — defined in RFC-0850p-d, implemented in mission `0850p-d-dc-initiated-group-creation.md`

### Phase 2: Lifecycle (pending RFC elaboration)

- [ ] `GroupState::UnboundAllPending = 0x06` (UNBIND_ALL broadcast; awaiting ACK from all members)
- [ ] `GroupState::UnboundAllDone = 0x07` (All members have left the platform; group is fully decommissioned)
- [ ] Transitions: `Bound → UnboundAllPending → UnboundAllDone` (or → `UnboundQuarantined` on failure)
- [ ] Unit tests for each transition

### Phase 3: DC rotation during UNBIND_ALL (pending RFC elaboration)

- [ ] If the DC resigns mid-decommission, the new DC inherits the UNBIND_ALL state
- [ ] Specify the handover protocol (cross-RFC with RFC-0855p-e)

### Phase 4: Platform-side leave race (pending RFC elaboration)

- [ ] If a new member joins the group during the decommission window, the DC MUST re-include them in the UNBIND_ALL
- [ ] Specify the race window and re-inclusion logic

### Phase 5: Audit trail (pending RFC elaboration)

- [ ] `DOT/1/UNBIND_ALL_AUDIT` envelope (subtype `b"UAAU"` per RFC-0850p-f §"Envelope Types Added"; R16 R2 fix — was subtype 0x18 in v1.0; the canonical format is the 4-byte ASCII tag per RFC-0850p-c §A) — signed audit entry
- [ ] Audit log structure: who initiated, when, why, witness count, `group_jid`
- [ ] Local audit log per node; rotation policy

### Phase 6: Re-decommission (pending RFC elaboration)

- [ ] A node that missed the original UNBIND_ALL may still have a `Bound` state
- [ ] Specify the re-decommission flow (e.g., the new UNBIND_ALL carries the original `nonce`)

### Phase 7: Quorum semantics (pending RFC elaboration)

- [ ] Define whether UNBIND_ALL needs 1 witness ACK (current proposal) or N-of-M witness quorum
- [ ] Trade-off: 1 ACK is faster but less safe; N-of-M is safer but slower

### Quality gates

- [ ] `cargo clippy --all-targets --all-features -- -D warnings` passes
- [ ] `cargo fmt -- --check` passes
- [ ] `cargo test -p octo-network` passes
- [ ] No regression in `0850p-c-base.md`, `0850p-d-dc-initiated-group-creation.md`, `0850p-e-kick-detection.md` missions' existing tests

## Location

- `crates/octo-network/src/dot/binding.rs` (additive: `UnbindAllDoneEnvelope` subtype `b"UADN"`, `UnbindAllAuditEnvelope` subtype `b"UAAU"` per RFC-0850p-f §"Envelope Types Added"; canonical 10-byte header per RFC-0850p-c §A; R16 R2 fix — was subtypes 0x17 and 0x18 in v1.0)
- `crates/octo-network/src/dot/group_registry.rs` (additive: `GroupState::UnboundAllPending`, `GroupState::UnboundAllDone`; transitions)
- `crates/octo-network/src/dot/dc.rs` (additive: DC rotation handover, re-decommission logic)
- `crates/octo-network/src/dot/audit_log.rs` (new) — local audit log

## Complexity

Medium (~800 lines; 2 new envelope types, 2 state transitions, DC rotation handover, audit log). Most of the work is in the RFC elaboration, not the implementation.

## Prerequisites

- Mission `0850p-d-dc-initiated-group-creation.md` (Open) — sister mission; the basic UNBIND_ALL is implemented there
- Mission `0855p-e-handover-request-envelope.md` (Open) — sister mission; DC rotation handover is cross-RFC
- RFC-0850p-f status: Draft (early stage)

## Notes

### Why is this mission "preliminary"?

RFC-0850p-f is in early-stage draft (Version 0.1). The main spec — quorum semantics, DC rotation, audit trail, re-decommission — is to be elaborated in the next RFC iteration. This mission tracks the basic UNBIND_ALL envelope (which is already in RFC-0850p-d §F) and reserves a slot for the additional types and state machine extensions that will be added when the RFC matures.

### Cross-RFC dependencies

This mission depends on `0855p-e-handover-request-envelope.md` (sister mission) for DC rotation handover during UNBIND_ALL. The two missions should be coordinated.

## Claimant

@mmacedoeu (agent-assisted)

## Pull Request

(none — Open mission)
