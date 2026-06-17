# Mission: 0855p-d — Sub-Domain / Sub-Group Nesting

## Status

Open (2026-06-17) — early stage; main scenarios pending RFC elaboration

## RFC

RFC-0855p-d (Networking): Sub-Domain / Sub-Group Nesting — `rfcs/draft/networking/0855p-d-subgroup-nesting.md`

## Summary

Implement the sub-domain / sub-group nesting flow outlined in RFC-0855p-d. Sub-groups are bound to sub-`domain_id`s (e.g., `BLAKE3(parent_domain_id || sub_label)`) and inherit the parent's mission and DC, but have their own membership and binding. The basic CGROUP ceremony (from RFC-0850p-d §A) is reused with a `SubGroupExtension` field. Closes scenario family S-G4 (sub-group nesting) from the gap analysis.

> **Status note:** RFC-0855p-d is currently an early-stage draft (Version 0.1). The main spec — sub-DC delegation protocol, cross-sub-group messaging, aggregation rules — is to be elaborated in the next RFC iteration. This mission will be expanded once the RFC is more mature. For now, the mission only covers the `SubGroupExtension` field in the CGROUP envelope and the derived `sub_domain_id` calculation.

## Dependencies

**Prerequisites:**

- RFC-0850p-c (Networking): Transport Group Binding Ceremony
- RFC-0850p-d (Networking): DC-Initiated Transport Group Creation & Invite — reuses CGROUP
- RFC-0855p-c (Networking): DomainCoordinator Role
- RFC-0126 (Numeric): DCS

## Acceptance Criteria (preliminary)

### Phase 1: SubGroupExtension field

- [ ] `SubGroupExtension` struct in `crates/octo-network/src/dot/binding.rs` with `parent_domain_id: [u8; 32]`, `sub_label: String`, `sub_dc_id: Option<[u8; 32]>`, `delegation_proof: Option<Vec<u8>>`
- [ ] Add `sub_group_extension: Option<SubGroupExtension>` field to `CreateGroupEnvelope` (RFC-0850p-d §Specification)
- [ ] DCS serialization for `SubGroupExtension`; round-trip byte equality test
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

- `crates/octo-network/src/dot/binding.rs` (additive: `SubGroupExtension` struct; `sub_group_extension: Option<SubGroupExtension>` field on `CreateGroupEnvelope`)
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

## Claimant

(none — Open mission)

## Pull Request

(none — Open mission)
