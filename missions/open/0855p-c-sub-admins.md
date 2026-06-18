# Mission: 0855p-c — Multi-admin groups (sub-admins)

## Status

Open (2026-06-16) — future

## RFC

RFC-0855p-c (Networking): DomainCoordinator Role — §"Future Work"

## Summary

DomainCoordinators can designate sub-admins (e.g., a deputy admin in case the primary is unreachable); sub-admins can sign envelopes but only within a `SUB_ADMIN_AUTHORITY` policy (e.g., cannot REBIND, can sign BIND for new members). This is useful for redundancy: if the primary DC is offline (e.g., their phone is dead), a sub-admin can keep the group operational.

## Design

1. **Sub-admin designation:** the primary DC signs a `SUB_ADMIN_DESIGNATE { domain_id, sub_admin_pubkey, authority_policy }` envelope. The sub-admin's pubkey is added to the DomainCoordinator's `CoordinatorRecord` as a sub-admin.
2. **Authority policy:** a bitfield of allowed operations:
   - `SUB_ADMIN_CAN_BIND` (default: yes) — can sign BIND for new members
   - `SUB_ADMIN_CAN_REBIND` (default: no) — can sign REBIND
   - `SUB_ADMIN_CAN_UNBIND` (default: no) — can sign UNBIND
   - `SUB_ADMIN_CAN_SLASH` (default: no) — can sign slash votes
   - `SUB_ADMIN_CAN_ATTEST` (default: yes) — can publish `PLATFORM_ADMIN_ATTEST`
3. **Activation:** a sub-admin becomes active only if the primary DC is unreachable for `SUB_ADMIN_ACTIVATION_EPOCHS = 10` (~10 minutes). The "unreachability" signal is: no heartbeat from primary DC in the last 10 epochs.
4. **Deactivation:** when the primary DC returns (sends a heartbeat), the sub-admin is deactivated and the primary resumes sole authority.
5. **Multi-sub-admin:** multiple sub-admins can be designated; they vote among themselves (2/3 majority) for the active sub-admin role.

## Acceptance Criteria

- [ ] `SUB_ADMIN_DESIGNATE` envelope type
- [ ] `SubAdminAuthority` bitfield type
- [ ] `SUB_ADMIN_ACTIVATION_EPOCHS = 10` constant
- [ ] Sub-admin activation logic in `crates/octo-network/src/dc/sub_admin.rs`
- [ ] Multi-sub-admin 2/3 vote logic
- [ ] Unit tests: designation, activation, deactivation, multi-sub-admin vote
- [ ] Integration test: primary DC goes offline → sub-admin activates; primary returns → sub-admin deactivates
- [ ] Documentation: how to designate a sub-admin (operator guide)
- [ ] Documentation: security implications (sub-admin compromise → limited blast radius)


### Implementation Guide

Reference: `crates/octo-network/src/dc/sub_admin.rs` (new).


### Type Coverage

| RFC-0855p-c Type | Implemented By |
|-----------------|----------------|
| `SUB_ADMIN_DESIGNATE` envelope type | This mission |
| `SubAdminAuthority` bitfield | This mission |
| `SUB_ADMIN_ACTIVATION_EPOCHS = 10` constant | This mission |

## Dependencies

Depends on RFC-0855p-c status: Accepted. No prerequisite missions; this is an extension to the `CoordinatorRecord` type.

## Claimant

(none — Open mission)

## Pull Request

(none — Open mission)

## Location

`crates/octo-network/src/dc/sub_admin.rs` (new).

## Complexity

Medium (~350 lines; designation flow, authority policy, activation/deactivation, multi-sub-admin vote).

## Prerequisites

- RFC-0855p-c status: Accepted

## Notes

### Why 10-epoch activation delay?

10 minutes is long enough that a brief network hiccup doesn't accidentally trigger sub-admin activation. After 10 minutes of primary DC silence, the sub-admin is needed.

### Why multi-sub-admin with 2/3 vote?

A single sub-admin is a single point of failure. Multiple sub-admins with 2/3 vote provide redundancy.

## Mitigates

D-DC-8 (single point of failure on primary DC availability)

## Deadline

Future
