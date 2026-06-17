# Mission: 0850p-d — DC-Initiated Transport Group Creation & Invite

## Status

Open (2026-06-17)

## RFC

RFC-0850p-d (Networking): DC-Initiated Transport Group Creation & Invite — `rfcs/draft/networking/0850p-d-dc-initiated-group-creation.md`

## Summary

Implement the DC-initiated group creation, invite issuance, third-party group BIND, and UNBIND_ALL ceremonies specified in RFC-0850p-d. This mission adds the new envelope types (`CGROUP`, `CGROUP_ACK`, `CGROUP_DONE`, `CGROUP_FAIL`, `INVITE`, `UNBIND_ALL`, `UNBIND_ALL_ACK`), extends `GroupState` with `Creating` (0x04) and `Inviting` (0x05), implements per-adapter `create_group` / `join_group` / `leave_group` / `dissolve_group` APIs, and adds the DC orchestration module (`octo-network/src/dot/dc.rs`). Closes scenario families S-G1, S-G2, S-G3, S-G5, S-G6 from the gap analysis.

## Dependencies

**Prerequisites (RFCs that must be Accepted or in Active Implementation):**

- RFC-0850p-c (Networking): Transport Group Binding Ceremony — base mission `0850p-c-base.md` must be claimed first (the new `Creating` and `Inviting` states are added to the existing `GroupState` enum from RFC-0850p-c §1)
- RFC-0855p-c (Networking): DomainCoordinator Role — DC authority and slash codes
- RFC-0850p-a (Networking): WhatsApp Auth Onboarding — `GroupConfig`, `BotLifecycle`
- RFC-0126 (Numeric): DCS — canonical serialization

**Sister RFC (co-implemented):**

- RFC-0850p-e (Networking): Kick & Platform Membership Change Detection — `SELF_KICKED` and `KICK_DETECTED` envelopes are needed before the `Creating → UnboundQuarantined` transition can be fully tested

## Acceptance Criteria

### Phase 1: Envelope types

- [ ] `CreateGroupEnvelope` (subtype `b"CGRO"` per RFC-0850p-d §"Envelope Types Added"; R16 R2 fix — was subtype 0x10 in v1.0; the canonical format is the 4-byte ASCII tag per RFC-0850p-c §A) in `crates/octo-network/src/dot/binding.rs` with canonical 10-byte header (`envelope_type: [u8; 4] = b"DOT1"`, `envelope_subtype: [u8; 4] = b"CGRO"`, `version: u16 = 0x0001`) plus body fields: `domain_id`, `mission_id`, `platform`, `proposed_group_metadata`, `initial_invite_count`, `dc_id`, `nonce`, `current_epoch`, `coordinator_term_id`, `signature`, `group_visibility: GroupVisibility` (R16 R1-M2 fix)
- [ ] `CreateGroupAckEnvelope` (subtype `b"CGAC"`; R16 R2-H1 fix — this struct was missing from RFC-0850p-d; the envelope type was in the table and referenced multiple times but had no struct; fields: `domain_id`, `cgroup_hash: [u8; 32]`, `witness_id`, `witness_epoch`, `ack_hash`, `nonce`, `signature`) — Witness confirms seeing the CGROUP and reserving the `domain_id`
- [ ] `CreateGroupDoneEnvelope` (subtype `b"CGDA"`; R16 R2 fix — was 0x12) with the `group_jid` and matching `nonce`
- [ ] `CreateGroupFailEnvelope` (subtype `b"CGFA"`; R16 R2 fix — was 0x13) with `reason_code` and `platform_error`
- [ ] `InviteEnvelope` (subtype `b"INVT"`; R16 R2 fix — was 0x14) with `invitee_pubkey` and `invite_token = BLAKE3-256(domain_id || mission_id || invitee_pubkey || nonce)`
- [ ] `UnbindAllEnvelope` (subtype `b"UALL"`; R16 R2 fix — was 0x15) with `domain_id`, `group_jid`, `platform`, `reason: UnbindReason`
- [ ] `UnbindAllAckEnvelope` (subtype `b"UAAC"`; R16 R2 fix — was 0x16; added in R16 R1-C1 fix since it was in the table but had no struct) with the witness signature
- [ ] `WitnessAssertion` struct (per RFC-0850p-d §D) for third-party group BIND
- [ ] DCS serialization (RFC-0126) for all envelope types; round-trip byte equality test
- [ ] Unit tests: signature verification, nonce uniqueness, 10-byte canonical header

### Phase 2: GroupState extensions

- [ ] Add `GroupState::Creating = 0x04` and `GroupState::Inviting = 0x05` to the `GroupState` enum (defined in `0850p-c-base.md` mission)
- [ ] Implement `Creating → Bound` transition on `CGROUP_DONE` + ≥ 1 witness BIND ACK
- [ ] Implement `Creating → Unbound` transition on `CGROUP_FAIL` or `CGROUP_TIMEOUT = 50` epochs
- [ ] Implement `Creating → UnboundQuarantined` transition on `SELF_KICKED` (per RFC-0850p-e) or `KICK_DETECTED`
- [ ] Implement `Bound → Inviting` transition on first `INVITE` emission
- [ ] Implement `Inviting → Bound` transition on all `INVITE`s acknowledged or expired
- [ ] Add `pending_invites: BTreeMap<[u8; 32], InviteEnvelope>` to `GroupRegistry`
- [ ] Unit tests: each transition path; state machine determinism

### Phase 3: DC orchestration module

- [ ] `crates/octo-network/src/dot/dc.rs` (new) — DC orchestration module
- [ ] `DcOrchestrator::create_group(domain_id, mission_id, platform, metadata) -> Result<group_jid, CreateGroupError>` — high-level API
- [ ] `DcOrchestrator::invite_member(domain_id, invitee_pubkey) -> Result<InviteEnvelope, InviteError>`
- [ ] `DcOrchestrator::unbind_all(domain_id, reason) -> Result<UnbindAllEnvelope, UnbindError>`
- [ ] `DcOrchestrator::bind_third_party_group(group_jid) -> Result<BindEnvelope, BindError>` — third-party BIND with witness assertion
- [ ] `DcOrchestrator::handle_founder_race(local_cgroup, remote_cgroup) -> RaceOutcome` — lexicographic `dc_id` tiebreak
- [ ] `DcOrchestrator::handle_kick(kick_event) -> Decision` — DC decision tree per RFC-0850p-e §Algorithm C (R16 R1-C3 fix: was RFC-0850p-d §C in v1.0, which is the wrong section — the kick decision tree is in 0850p-e §Algorithm C "DC decision tree", not in 0850p-d §C "Atomic Migration via CREATE+REBIND"; both RFCs have a §C section but they cover different topics)
- [ ] Unit tests: each high-level API; founder race; kick decision tree

### Phase 4: WhatsApp adapter integration

- [ ] `create_group(metadata) -> Result<group_jid, AdapterError>` in `octo-adapter-whatsapp` — calls WhatsApp `GroupCreate` event
- [ ] `join_group(group_jid, invite_token) -> Result<(), AdapterError>` — uses the `invite_token` to authenticate the join
- [ ] `leave_group(group_jid) -> Result<(), AdapterError>` — calls WhatsApp `GroupLeave` event
- [ ] `dissolve_group(group_jid) -> Result<(), AdapterError>` — calls WhatsApp `GroupEnd` event (requires admin rights)
- [ ] `lookup_group(group_jid) -> Result<GroupInfo, AdapterError>` — for third-party BIND verification
- [ ] Cross-check `group_jid` existence before emitting `CGROUP_DONE` (per RFC-0850p-d §A.6)
- [ ] Integration test: DC creates a new WhatsApp group, binds to `domain_id`, 3 witnesses ACK, group is `Bound`

### Phase 5: Matrix and Telegram adapter integration

- [ ] Same as Phase 4 for `octo-adapter-matrix` (using `POST /createRoom`, `POST /rooms/{roomId}/leave`, etc.)
- [ ] Same as Phase 4 for `octo-adapter-telegram` (using `createGroup`, `leaveChat`, etc.)
- [ ] Integration test for each adapter

### Phase 6: Slash codes

- [ ] Slash 0x000F (CgGroupSpam) — on CGROUP rate-limit violation
- [ ] Slash 0x0010 (FalseWitness) — on false `WitnessAssertion` for third-party BIND
- [ ] Slash 0x000E (CreateGroupFailed) — on `Creating → UnboundQuarantined` transition
- [ ] Unit tests: slash codes emitted on the correct triggers

### Quality gates

- [ ] `cargo clippy --all-targets --all-features -- -D warnings` passes
- [ ] `cargo fmt -- --check` passes
- [ ] `cargo test -p octo-network` passes
- [ ] `cargo test -p octo-adapter-whatsapp` passes
- [ ] `cargo test -p octo-adapter-matrix` passes
- [ ] `cargo test -p octo-adapter-telegram` passes
- [ ] No regression in `0850p-c-base.md` mission's existing tests

## Location

- `crates/octo-network/src/dot/binding.rs` (additive: 7 new envelope types + `WitnessAssertion`)
- `crates/octo-network/src/dot/group_registry.rs` (additive: 2 new `GroupState` variants + `pending_invites`)
- `crates/octo-network/src/dot/dc.rs` (new)
- `crates/octo-network/src/dot/slash.rs` (additive: slash codes 0x000E, 0x000F, 0x0010)
- `crates/octo-adapter-whatsapp/src/adapter.rs` (additive: `create_group`, `join_group`, `leave_group`, `dissolve_group`, `lookup_group`)
- `crates/octo-adapter-matrix/src/lib.rs` (additive: same)
- `crates/octo-adapter-telegram/src/lib.rs` (additive: same)

## Complexity

High (~2000 lines; 7 envelope types, 6 state transitions, 5 high-level DC APIs, 3 adapter integrations, founder race resolution, kick decision tree, slash codes, integration tests).

## Prerequisites

- Base mission `0850p-c-base.md` (Open) — must be claimed first to provide the `GroupState` enum and `GroupRegistry`
- RFC-0850p-c status: Accepted
- RFC-0855p-c status: Accepted
- RFC-0850p-a status: Accepted
- RFC-0850p-d status: Draft (this RFC is co-implemented with the mission; the RFC will move to Accepted once the mission is implemented and reviewed)

## Notes

### Why co-implemented with RFC-0850p-e?

`SELF_KICKED` and `KICK_DETECTED` envelopes (defined in RFC-0850p-e) are required for the `Creating → UnboundQuarantined` transition. Without RFC-0850p-e, the DC can create a group but cannot detect if it is kicked mid-create. The two RFCs are tightly coupled and should be implemented in the same mission (or with RFC-0850p-e implemented first as a hard prerequisite).

### Mission decomposition

This mission covers Phases 1, 3, 4, 5, 6 of RFC-0850p-d. Phase 2 (third-party group BIND) is non-trivial and could be split into a sub-mission `0850p-d-third-party-bind.md` if needed.

### Why "sister RFC" dependency on 0850p-e?

The kick detection RFC-0850p-e is needed to fully test the `Creating → UnboundQuarantined` transition. However, the basic CGROUP/INVITE/UNBIND_ALL envelopes can be implemented and tested without RFC-0850p-e (the `UnboundQuarantined` state can be set manually for the test). The mission can proceed in parallel with RFC-0850p-e's mission.

## Claimant

(none — Open mission)

## Pull Request

(none — Open mission)
