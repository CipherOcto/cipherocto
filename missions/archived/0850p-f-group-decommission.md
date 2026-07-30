# Mission: 0850p-f — Transport Group Decommission

> **Path B closure:** AC + code verified 2026-07-30 via mission audit. Code ground: `crates/octo-network/src/dot/binding.rs:103,112,115` `GroupState::UnboundQuarantined = 0x03`, `UnboundAllPending = 0x06`, `UnboundAllDone = 0x07`; `group_registry.rs:543,573,237` transitions; `dc_envelopes.rs:43,45` `UNBIND_ALL = b"UALL"`, `UNBIND_ALL_ACK = b"UAAC"`; `decommission.rs:34,121,215,295-339` `UNBIND_ALL_AUDIT = b"UAAU"`, `UnbindAllAuditEnvelope`, `AuditLog`, `UnbindAllAckCollector`; `dc.rs:283-289` `UNBIND_ALL_MIN_ACKS`, `UNBIND_ALL_TIMEOUT_EPOCHS`; `dc.rs:297-323` `build_unbind_all` with `original_nonce` re-decommission carry-forward. RFC-0850p-f elevated to v0.3 with F-1..F-6 elaborations. All 21 ACs now checked. Did not pass through `with-pr/` — work landed in `next` via prior commits.

## Status

Completed (Archived 2026-07-30 — Path B)

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

- [x] `GroupState::UnboundAllPending = 0x06` (UNBIND_ALL broadcast; awaiting ACK from all members) — `crates/octo-network/src/dot/binding.rs:112`
- [x] `GroupState::UnboundAllDone = 0x07` (All members have left the platform; group is fully decommissioned) — `binding.rs:115`
- [x] Transitions: `Bound → UnboundAllPending → UnboundAllDone` (or → `UnboundQuarantined` on failure) — `group_registry.rs:543` (`transition_to_unbound_all_pending`), `:573` (`transition_to_unbound_all_done`), `:237` (`transition_to_unbound_quarantined`)
- [x] Unit tests for each transition — `group_registry.rs:952` idempotency test, plus 12+ tests in `decommission.rs` `mod tests`

### Phase 3: DC rotation during UNBIND_ALL (pending RFC elaboration)

- [x] If the DC resigns mid-decommission, the new DC inherits the UNBIND_ALL state — RFC-0850p-f v0.3 §"DC Rotation During UNBIND_ALL"; `SlashTally` carries in-flight `UnboundAllPending` state via `handover.rs:218`
- [x] Specify the handover protocol (cross-RFC with RFC-0855p-e) — RFC-0850p-f v0.3 references RFC-0855p-e `HandoverRequestEnvelope`; successor DC's `coordinator_term_id` in `witness_epoch`

### Phase 4: Platform-side leave race (pending RFC elaboration)

- [x] If a new member joins the group during the decommission window, the DC MUST re-include them in the UNBIND_ALL — RFC-0850p-f v0.3 §"Leave Race Window"; `pending_members: BTreeSet<[u8; 32]>` mechanism
- [x] Specify the race window and re-inclusion logic — bounded by `UNBIND_ALL_TIMEOUT_EPOCHS = 100`

### Phase 5: Audit trail (pending RFC elaboration)

- [x] `DOT/1/UNBIND_ALL_AUDIT` envelope (subtype `b"UAAU"`) — `decommission.rs:34` constant, `:121` struct
- [x] Audit log structure: who initiated, when, why, witness count, `group_jid` — `decommission.rs:121-149` `UnbindAllAuditEnvelope` fields
- [x] Local audit log per node; rotation policy — `decommission.rs:215` `AuditLog` (in-memory FIFO at 1024 entries; disk-backed rotation deferred to v0.4)

### Phase 6: Re-decommission (pending RFC elaboration)

- [x] A node that missed the original UNBIND_ALL may still have a `Bound` state — RFC-0850p-f v0.3 §"Re-decommission"
- [x] Specify the re-decommission flow (e.g., the new UNBIND_ALL carries the original `nonce`) — `dc.rs:297` `build_unbind_all(... original_nonce: Option<[u8; 32]>)`; envelope's `nonce = original_nonce.unwrap_or_else(|| self.fresh_nonce())`

### Phase 7: Quorum semantics (pending RFC elaboration)

- [x] Define whether UNBIND_ALL needs 1 witness ACK (current proposal) or N-of-M witness quorum — RFC-0850p-f v0.3 §F-1; chosen = 1 ACK (`UNBIND_ALL_MIN_ACKS = 1`, `dc.rs:283`)
- [x] Trade-off: 1 ACK is faster but less safe; N-of-M is safer but slower — RFC-0850p-f v0.3 §F-1 trade-off table

### Quality gates

- [x] `cargo clippy --all-targets --no-deps -- -D warnings` passes (`octo-network`)
- [x] `cargo fmt --all` runs clean (reverted unrelated reputation test drift)
- [x] `cargo test -p octo-network --lib` passes (1326 tests, +3 new in `decommission.rs`)
- [x] No regression in `0850p-c-base.md`, `0850p-d-dc-initiated-group-creation.md`, `0850p-e-kick-detection.md` missions' existing tests — all 1326 lib tests pass

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
