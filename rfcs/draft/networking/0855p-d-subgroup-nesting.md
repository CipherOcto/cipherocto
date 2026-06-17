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

The `DOT/1/CGROUP` envelope (defined in RFC-0850p-d §Specification) is extended with:

```rust
/// Optional fields added to CreateGroupEnvelope for sub-groups.
#[derive(Dcs, Clone, Debug, PartialEq, Eq)]
pub struct SubGroupExtension {
    pub parent_domain_id: [u8; 32],
    pub sub_label: String,             // max 256 bytes UTF-8
    pub sub_dc_id: Option<[u8; 32]>,   // None = parent DC is implicit DC
    pub delegation_proof: Option<Vec<u8>>,   // signed delegation from parent DC
}
```

`sub_domain_id` is derived: `sub_domain_id = BLAKE3(parent_domain_id || sub_label)`.

### State Machine (preliminary)

A sub-group has its own `GroupBinding` and `GroupState` independent of the parent. The parent's state is unaffected by sub-group transitions.

## Future Work (specific)

- **F-1: Sub-DC delegation protocol.** Specify how a parent DC delegates sub-DC authority (signed delegation envelope, rotation, revocation).
- **F-2: Cross-sub-group messaging.** Define the protocol for a sub-group to send a message to the parent (e.g., aggregated roll-call).
- **F-3: Sub-group → parent aggregation.** Define how a sub-group's votes are aggregated to the parent (e.g., a sub-group coordinator signs a "roll-up" envelope).
- **F-4: Sub-group decommission.** If a sub-group is UNBIND'd, the parent remains. Specify the policy.
- **F-5: Cross-platform sub-groups.** A sub-group can be on a different platform than the parent (e.g., parent on WhatsApp, sub-group on Matrix). Specify the cross-platform routing.
- **F-6: Sub-group label collision.** Two sub-groups with the same `sub_label` under different parents are different `sub_domain_id`s (by BLAKE3 derivation). No collision.
- **F-7: Sub-group label format.** `sub_label` SHOULD be a UTF-8 string with no `/` characters (to enable URL-style addressing).

## Rationale (preliminary)

This RFC is in early-stage draft. The basic sub-group CGROUP ceremony reuses the existing CGROUP flow with a `SubGroupExtension`. A future iteration will elaborate the sub-DC delegation protocol, cross-sub-group messaging, and aggregation rules.

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 0.1 | 2026-06-17 | Initial stub; main spec to be elaborated |

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
