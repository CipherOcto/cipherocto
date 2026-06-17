# RFC-0855p-d (Networking): Sub-Domain / Sub-Group Nesting

## Status

Draft (2026-06-17) — early stage; main scenarios to be elaborated in next iteration

## Authors

- @mmacedoeu

## Maintainers

- @mmacedoeu

## Summary

Specifies how a DomainCoordinator (DC) can create **sub-groups** for sub-domains within a parent domain. Sub-groups are bound to sub-`domain_id`s (e.g., `BLAKE3(parent_domain_id || sub_label)`) and inherit the parent's mission and DC, but have their own membership and binding. Closes scenario family **S-G4** (sub-group nesting) from `docs/research/networking-rfc-cross-reference-analysis.md`. Complements RFC-0850p-d (DC-initiated group creation): sub-groups use the same CGROUP ceremony but with a derived `sub_domain_id` and a `parent_domain_id` linkage field.

## Dependencies

- RFC-0850p-c (Networking): Transport Group Binding Ceremony
- RFC-0850p-d (Networking): DC-Initiated Transport Group Creation & Invite — reuses CGROUP
- RFC-0855p-c (Networking): DomainCoordinator Role — DC authority
- RFC-0126 (Numeric): DCS

## Design Goals (preliminary)

1. **Inherited authority.** The parent DC is the implicit DC for all sub-domains unless explicitly delegated.
2. **Independent binding.** A sub-group's BIND is independent of the parent's BIND; either can be UNBIND'd without affecting the other.
3. **Cross-sub-group messaging.** A node that is a member of a sub-group is NOT automatically a member of the parent group (and vice versa). Cross-sub-group messaging is via the overlay, not the physical group.
4. **Hierarchical naming.** `sub_domain_id = BLAKE3(parent_domain_id || sub_label)` where `sub_label` is a UTF-8 string (max 256 bytes).
5. **Delegation policy.** The parent DC MAY delegate sub-DC authority to another node for a specific sub-domain.

## Motivation

Use cases like "mission alpha has multiple working groups (sub-committees)" require hierarchical grouping. The current RFC-0850p-c and RFC-0850p-d treat `domain_id` as flat; there is no notion of parent / child.

Example: `mission_alpha` has domains `domain-vote-recount` (parent) and sub-domains `domain-vote-recount.legal-review` and `domain-vote-recount.comms-review`. Each sub-domain has its own physical group (e.g., WhatsApp / Matrix / Telegram) with its own membership.

## Status (Detailed)

This RFC is in early-stage draft. The sub-group CGROUP ceremony can reuse the basic CGROUP flow from RFC-0850p-d §A with the following additions:
- `parent_domain_id: [u8; 32]` field in CGROUP envelope
- `sub_label: String` field in CGROUP envelope
- `sub_domain_id = BLAKE3(parent_domain_id || sub_label)` derived field
- `sub_dc_id: Option<[u8; 32]>` field for delegated sub-DC

A future iteration will elaborate:
- Detailed state machine for sub-group binding
- Cross-sub-group membership rules
- Sub-DC delegation protocol
- Parent → sub-group message routing
- Sub-group → parent aggregation

## Use Case Link

- `docs/use-cases/mission-coordinator-lifecycle.md` — "DC Delegation" section
- `docs/use-cases/social-platform-transport-layer.md` — "Hierarchical Grouping" section
- `docs/research/networking-rfc-cross-reference-analysis.md` — Scenario family S-G4

## Specification (preliminary)

### Envelope Type Extension

The base `DOT/1/CGROUP` envelope (defined in RFC-0850p-d §Specification) is the parent envelope for sub-group creation. To keep the base CGROUP envelope clean, this RFC defines a new envelope variant `DOT/1/CGROUP_SUB` for sub-group creation. The new variant carries a `SubGroupExtension` field; the base `CGROUP` envelope is unchanged.

```rust
/// New envelope for sub-group creation (DOT/1/CGROUP_SUB).
/// (R16 R1-H2 fix: previous wording said "the DOT/1/CGROUP envelope (defined in
///  RFC-0850p-d §Specification) is extended with: SubGroupExtension", but the
///  base CGROUP envelope in 0850p-d does NOT have a `sub_group_extension` field.
///  This was a missing cross-reference. The fix: define a new envelope variant
///  CGROUP_SUB (with subtype tag `b"CGSB"`) that carries the SubGroupExtension;
///  the base CGROUP envelope is unchanged.)
#[derive(Dcs, Clone, Debug, PartialEq, Eq)]
pub struct CreateSubGroupEnvelope {
    pub envelope_type: [u8; 4],         // b"DOT1"
    pub envelope_subtype: [u8; 4],      // b"CGSB" (CREATE_SUBGROUP)
    pub version: u16,                   // 0x0001
    pub domain_id: [u8; 32],            // sub_domain_id (derived)
    pub mission_id: [u8; 32],
    pub platform: Platform,
    pub proposed_group_metadata: ProposedGroupMetadata,   // reuses 0850p-d's type
    pub initial_invite_count: u16,
    pub dc_id: [u8; 32],                // sub-DC's peer_id (or parent DC if None)
    pub sub_group_extension: SubGroupExtension,           // see below
    pub nonce: [u8; 16],
    pub current_epoch: u64,
    pub coordinator_term_id: [u8; 32],
    pub signature: [u8; 64],
}

/// Optional fields added to CreateSubGroupEnvelope for sub-groups.
#[derive(Dcs, Clone, Debug, PartialEq, Eq)]
pub struct SubGroupExtension {
    pub parent_domain_id: [u8; 32],
    pub sub_label: String,             // max 256 bytes UTF-8, MUST NOT contain `/`
    pub sub_dc_id: Option<[u8; 32]>,   // None = parent DC is implicit DC
    pub delegation_proof: Option<Vec<u8>>,   // signed delegation from parent DC
}
```

`sub_domain_id` is derived: `sub_domain_id = BLAKE3(parent_domain_id || sub_label)`. The `sub_dc_id` field overrides the default "parent DC is the implicit sub-DC" behavior. The `delegation_proof` (when present) is a signed envelope from the parent DC granting sub-DC authority to the `sub_dc_id` (the format of the delegation proof will be specified in F-1 "Sub-DC delegation protocol").

### State Machine (preliminary)

A sub-group has its own `GroupBinding` and `GroupState` independent of the parent. The parent's state is unaffected by sub-group transitions.

## Future Work (specific)

- **F-1: Sub-DC delegation protocol.** Specify how a parent DC delegates sub-DC authority (signed delegation envelope, rotation, revocation).
- **F-2: Cross-sub-group messaging.** Define the protocol for a sub-group to send a message to the parent (e.g., aggregated roll-call).
- **F-3: Sub-group → parent aggregation.** Define how a sub-group's votes are aggregated to the parent (e.g., a sub-group coordinator signs a "roll-up" envelope).
- **F-4: Sub-group decommission.** If a sub-group is UNBIND'd, the parent remains. Specify the policy.
- **F-5: Cross-platform sub-groups.** A sub-group can be on a different platform than the parent (e.g., parent on WhatsApp, sub-group on Matrix). Specify the cross-platform routing.
- **F-6: Sub-group label collision.** Two sub-groups with the same `sub_label` under different parents are different `sub_domain_id`s (by BLAKE3 derivation). No collision.
- **F-7: Sub-group label format.** `sub_label` MUST be a UTF-8 string with no `/` characters (to enable URL-style addressing). (R16 R1-L2 fix: was SHOULD — URL-style parsing requires the constraint, not just a recommendation.)

## Rationale (preliminary)

This RFC is in early-stage draft. The basic sub-group CGROUP ceremony reuses the existing CGROUP flow with a `SubGroupExtension`. A future iteration will elaborate the sub-DC delegation protocol, cross-sub-group messaging, and aggregation rules.

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 0.1 | 2026-06-17 | Initial stub; main spec to be elaborated |
| 0.2 | 2026-06-17 | R16 R1 fix: (H2) replaced "extend CreateGroupEnvelope with SubGroupExtension" wording with a new envelope variant `CreateSubGroupEnvelope` (subtype tag `b"CGSB"`), since the base CGROUP envelope in RFC-0850p-d has no `sub_group_extension` field; (L2) F-7: `sub_label` constraint changed SHOULD → MUST (no `/` characters; required for URL-style addressing). |

## Related RFCs

- RFC-0850p-c (Networking): Transport Group Binding Ceremony
- RFC-0850p-d (Networking): DC-Initiated Transport Group Creation & Invite
- RFC-0855p-c (Networking): DomainCoordinator Role

## Related Use Cases

- `docs/use-cases/mission-coordinator-lifecycle.md` — "DC Delegation"
- `docs/use-cases/social-platform-transport-layer.md` — "Hierarchical Grouping"
- `docs/research/networking-rfc-cross-reference-analysis.md` — Scenario family S-G4

---

**Version:** 0.1
**Submission Date:** 2026-06-17
**Last Updated:** 2026-06-17
