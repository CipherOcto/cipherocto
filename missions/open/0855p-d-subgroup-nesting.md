# Mission: 0855p-d — Sub-Domain / Sub-Group Nesting

## Status

Completed (2026-06-17) — Phase 1 implemented

## RFC

RFC-0855p-d (Networking): Sub-Domain / Sub-Group Nesting — `rfcs/draft/networking/0855p-d-subgroup-nesting.md`

## Summary

Implement the sub-domain / sub-group nesting flow outlined in RFC-0855p-d. Sub-groups are bound to sub-`domain_id`s (e.g., `BLAKE3(parent_domain_id || sub_label)`) and inherit the parent's mission and DC, but have their own membership and binding. The sub-group ceremony uses a NEW envelope variant `CreateSubGroupEnvelope` (subtype `b"CGSB"`), NOT an extension to the base `CreateGroupEnvelope` (R16 R1-H2 fix — see RFC-0855p-d §"Envelope Type Extension"). Closes scenario family S-G4 (sub-group nesting) from the gap analysis.

> **Status note:** RFC-0855p-d is currently an early-stage draft (Version 0.1). The main spec — sub-DC delegation protocol, cross-sub-group messaging, aggregation rules — is to be elaborated in the next RFC iteration. This mission will be expanded once the RFC is more mature. For now, the mission only covers the `CreateSubGroupEnvelope` envelope variant (subtype `b"CGSB"`) and the derived `sub_domain_id` calculation.

## Dependencies

**Prerequisites:**

- RFC-0850p-c (Networking): Transport Group Binding Ceremony
- RFC-0850p-d (Networking): DC-Initiated Transport Group Creation & Invite — reuses CGROUP
- RFC-0855p-c (Networking): DomainCoordinator Role
- RFC-0126 (Numeric): DCS

## Acceptance Criteria (preliminary)

### Phase 1: CreateSubGroupEnvelope variant

- [ ] `CreateSubGroupEnvelope` (subtype `b"CGSB"`) — NEW envelope variant per RFC-0855p-d §"Envelope Type Extension" (R16 R1-H2 fix — was previously described as "add `sub_group_extension: Option<SubGroupExtension>` field to `CreateGroupEnvelope`" but the base CGROUP envelope in RFC-0850p-d has no such field; the fix is a new envelope variant). Canonical 10-byte header per RFC-0850p-c §A.
- [ ] `SubGroupExtension` struct in `crates/octo-network/src/dot/binding.rs` with `parent_domain_id: [u8; 32]`, `sub_label: String` (MUST NOT contain `/` per RFC-0855p-d F-7; R16 R1-L2 fix — was SHOULD), `sub_dc_id: Option<[u8; 32]>`, `delegation_proof: Option<Vec<u8>>`
- [ ] DCS serialization for `CreateSubGroupEnvelope` and `SubGroupExtension`; round-trip byte equality test
- [ ] Unit tests: sub_domain_id derivation `BLAKE3(parent_domain_id || sub_label)`; sub_label validation (no `/` characters); canonical header
- [ ] Unit tests: derived `sub_domain_id = BLAKE3(parent_domain_id || sub_label)` matches expected hash

### Phase 2: Sub-group CGROUP ceremony (pending RFC elaboration)

- [ ] The DC emits a CGROUP with `sub_group_extension` set
- [ ] The platform-side `create_group` creates the sub-group (e.g., a WhatsApp sub-group is just a regular group; a Matrix sub-room is just a regular room)
- [ ] The sub-group's BIND is independent of the parent's BIND
- [ ] Unit tests: parent DC is the implicit DC for the sub-group; `sub_dc_id` overrides this if set

### Phase 3: Sub-DC delegation protocol (pending RFC elaboration)

- [ ] The parent DC signs a delegation envelope granting sub-DC authority to another node for a specific sub-domain
- [ ] The sub-DC has all DC rights for the sub-domain (CGROUP, INVITE, UNBIND_ALL)
- [ ] Revocation protocol: the parent DC can revoke sub-DC authority at any time

### Phase 4: Cross-sub-group messaging (pending RFC elaboration)

- [ ] Sub-group → parent roll-up envelope
- [ ] Parent → sub-group broadcast envelope
- [ ] Cross-sub-group membership rules: a node that is a member of a sub-group is NOT automatically a member of the parent (and vice versa)

### Phase 5: Sub-group decommission (pending RFC elaboration)

- [ ] If a sub-group is UNBIND'd, the parent remains
- [ ] If the parent is UNBIND'd, all sub-groups are UNBIND'd

### Quality gates

- [ ] `cargo clippy --all-targets --all-features -- -D warnings` passes
- [ ] `cargo fmt -- --check` passes
- [ ] `cargo test -p octo-network` passes
- [ ] No regression in `0850p-c-base.md`, `0850p-d-dc-initiated-group-creation.md`, `0850p-e-kick-detection.md` missions' existing tests

## Location

- `crates/octo-network/src/dot/binding.rs` (additive: `CreateSubGroupEnvelope` envelope variant with subtype `b"CGSB"`; `SubGroupExtension` struct; canonical 10-byte header per RFC-0850p-c §A; R16 R2 fix — was previously described as adding `sub_group_extension: Option<SubGroupExtension>` field to `CreateGroupEnvelope`, which is incorrect since the base CGROUP envelope has no such field)
- `crates/octo-network/src/dot/sub_group.rs` (new) — sub-group derived `sub_domain_id` calculation, sub-DC delegation helpers
- `crates/octo-network/src/dot/dc.rs` (additive: parent → sub-group, sub-group → parent roll-up)

## Complexity

Medium (~600 lines; 1 envelope extension, derived `sub_domain_id` calculation, sub-group CGROUP ceremony). Most of the work is in the RFC elaboration (sub-DC delegation, cross-sub-group messaging), not the implementation.

## Prerequisites

- Mission `0850p-d-dc-initiated-group-creation.md` (Open) — sister mission; the basic CGROUP is implemented there
- RFC-0855p-d status: Draft (early stage)

## Notes

### Why is this mission "preliminary"?

RFC-0855p-d is in early-stage draft (Version 0.1). The main spec — sub-DC delegation protocol, cross-sub-group messaging, aggregation rules — is to be elaborated in the next RFC iteration. This mission tracks the `SubGroupExtension` field (which is a small additive change to the CGROUP envelope) and reserves a slot for the sub-DC delegation protocol and cross-sub-group messaging logic that will be added when the RFC matures.

### Cross-RFC dependencies

This mission depends on `0850p-d-dc-initiated-group-creation.md` (sister mission) for the basic CGROUP flow. The two missions should be coordinated.

## Implementation

Phase 1 implemented in `crates/octo-network/src/dot/sub_group.rs` (589 lines, 18 tests, committed as part of R16 R12):

- `CreateSubGroupEnvelope` (subtype `b"CGSB"`) — NEW envelope variant (R16 R1-H2 fix) with canonical 10-byte header per RFC-0850p-c §A; body fields per RFC-0855p-d §"Envelope Type Extension"; sign/verify over `BLAKE3-256(header || body)`.
- `SubGroupExtension` struct: `parent_domain_id`, `sub_label`, `sub_dc_id: Option<[u8; 32]>`, `delegation_proof: Option<Vec<u8>>`.
- `derive_sub_domain_id = BLAKE3(parent_domain_id || sub_label)`.
- `validate_sub_label`: non-empty, length <= 256, no `/` characters (R16 R1-L2 fix; MUST NOT contain `/` per F-7).
- `SubGroupError` enum (EmptyLabel, LabelTooLong, SlashInLabel, SubDomainIdMismatch, HeaderMismatch).

All 1210 tests in `octo-network` pass. Phases 2–5 remain pending RFC elaboration.

## Claimant

@mmacedoeu (agent-assisted)

## Pull Request

(none — Open mission)
