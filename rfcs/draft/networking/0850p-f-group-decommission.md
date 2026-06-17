# RFC-0850p-f (Networking): Transport Group Decommission

## Status

Draft (2026-06-17) — early stage; main scenarios to be elaborated in next iteration

## Authors

- @mmacedoeu

## Maintainers

- @mmacedoeu

## Summary

Specifies how a DomainCoordinator (DC), MissionController, MissionCreator, or governance vote can **decommission a transport group** (UNBIND_ALL), with platform-side leave / dissolve, post-decommission audit log, and platform-side leave race handling. The basic `DOT/1/UNBIND_ALL` envelope is defined in RFC-0850p-d §F; this RFC elaborates the lifecycle, audit trail, platform-side choreography, and edge cases (DC rotation, partial decommission, leave race with new members). Closes scenario family **S-G7** (group decommission) from `docs/research/networking-rfc-cross-reference-analysis.md`.

## Dependencies

- RFC-0850p-c (Networking): Transport Group Binding Ceremony — `GroupBinding`, `GroupState`
- RFC-0850p-d (Networking): DC-Initiated Transport Group Creation & Invite — `UNBIND_ALL` envelope (defined here, refined here)
- RFC-0855p-b (Networking): Mission Coordinator Lifecycle — DC rotation
- RFC-0126 (Numeric): DCS

## Design Goals (preliminary)

1. **Deterministic decommission.** All nodes MUST agree on the decommission of a group via ≥ 1 witness ACK.
2. **Platform-side leave race handling.** If a new member joins the group during the decommission window, the DC MUST re-include them in the UNBIND_ALL.
3. **Audit trail.** A signed decommission receipt is stored per node for forensic analysis.
4. **DC rotation support.** A DC can resign mid-decommission; the new DC inherits the decommission state.

## Motivation

RFC-0850p-d §F defines the basic `UNBIND_ALL` flow but does not address:
- DC rotation during decommission (the DC may resign or be slashed)
- Platform-side leave race (a new member joins while UNBIND_ALL is in flight)
- Audit trail (who initiated, when, why, witness count)
- Re-decommission (a group that was decommissioned but is somehow still bound on some nodes)

This RFC closes the gap.

## Status (Detailed)

This RFC is in early-stage draft. The basic UNBIND_ALL envelope is defined in RFC-0850p-d §F. This RFC will elaborate:

- Detailed state machine for `UnboundAllPending` and `UnboundAllDone` states
- DC rotation semantics during UNBIND_ALL
- Platform-side leave race handling
- Audit trail structure
- Re-decommission (a node that missed the original UNBIND_ALL)
- Quorum semantics: 1 witness ACK vs. N-of-M witness quorum

## Use Case Link

- `docs/use-cases/social-platform-transport-layer.md` — "Group Lifecycle" section
- `docs/use-cases/mission-coordinator-lifecycle.md` — "DC Resignation" section

## Specification (preliminary)

### Envelope Types Added

| Envelope Type | Subtype | Direction | Description |
|---------------|---------|-----------|-------------|
| `DOT/1/UNBIND_ALL` | 0x15 | Authority → mesh (broadcast) | (Defined in RFC-0850p-d §F) |
| `DOT/1/UNBIND_ALL_ACK` | 0x16 | Witness → Authority | (Defined in RFC-0850p-d §F) |
| `DOT/1/UNBIND_ALL_DONE` | 0x17 | Authority → mesh (broadcast) | Final confirmation; all members have left |
| `DOT/1/UNBIND_ALL_AUDIT` | 0x18 | Authority → audit log (out-of-band) | Signed audit entry |

### State Machine (preliminary)

| State | Value | Description |
|-------|-------|-------------|
| `UnboundAllPending` | 0x06 | UNBIND_ALL broadcast; awaiting ACK from all members |
| `UnboundAllDone` | 0x07 | All members have left the platform; group is fully decommissioned |

## Future Work (specific)

- **F-1: Quorum semantics.** Define whether UNBIND_ALL needs 1 witness ACK (current proposal) or N-of-M witness quorum. Trade-off: 1 ACK is faster but less safe; N-of-M is safer but slower.
- **F-2: DC rotation during UNBIND_ALL.** If the DC resigns mid-decommission, the new DC must inherit the UNBIND_ALL state. Specify the handover protocol.
- **F-3: Platform-side leave race.** If a new member joins while UNBIND_ALL is in flight, the new member is added to the UNBIND_ALL recipient list. Specify the race window.
- **F-4: Audit trail.** Define the structure of the signed audit entry (who, when, why, witness count, group_jid).
- **F-5: Re-decommission.** A node that missed the original UNBIND_ALL may still have a `Bound` state. Specify the re-decommission flow.
- **F-6: Platform-side dissolve permissions.** If the DC does not have admin rights on the platform, the group cannot be dissolved (only left). Specify the policy.

## Rationale (preliminary)

This RFC is in early-stage draft. The basic UNBIND_ALL flow is captured in RFC-0850p-d §F. This RFC will elaborate the lifecycle, audit trail, and edge cases once the base mission (`0850p-c-base.md`) and DC-initiated group creation mission (`0850p-d-dc-initiated-group-creation.md`) are complete.

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 0.1 | 2026-06-17 | Initial stub; main spec to be elaborated |

## Related RFCs

- RFC-0850p-c (Networking): Transport Group Binding Ceremony
- RFC-0850p-d (Networking): DC-Initiated Transport Group Creation & Invite (sister RFC, co-implemented)
- RFC-0850p-e (Networking): Kick & Platform Membership Change Detection (sister RFC, co-implemented)
- RFC-0855p-b (Networking): Mission Coordinator Lifecycle

## Related Use Cases

- `docs/use-cases/social-platform-transport-layer.md`
- `docs/use-cases/mission-coordinator-lifecycle.md`
- `docs/research/networking-rfc-cross-reference-analysis.md` — Scenario family S-G7

---

**Version:** 0.1
**Submission Date:** 2026-06-17
**Last Updated:** 2026-06-17
