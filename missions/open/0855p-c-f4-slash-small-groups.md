# Mission: 0855p-c F4 — Slash for < 4 member groups

## Status

Open (2026-06-16) — post-launch

## RFC

RFC-0855p-c (Networking): DomainCoordinator Role — §"Future Work" F4

## Summary

For groups with < 4 members, slash (demote+cooldown) the misbehaving member instead of UNBIND (which would lose the entire group). Currently the policy is UNBIND on any member misbehavior; for small groups, UNBIND is overly aggressive (e.g., a 3-member group loses 33% of its membership on a single slash). Slash is a softer response that preserves the group.

## Design

1. **Group size threshold:** `MIN_GROUP_SIZE_FOR_UNBIND = 4`. Groups with `member_count < 4` use slash instead of UNBIND.
2. **At BIND time:** the DomainCoordinator records `member_count` in the BIND envelope. This is the group size at binding time; it can change (members added/removed), but the threshold check is against the binding-time count.
3. **On member misbehavior:**
   - If `member_count_at_bind >= 4`: UNBIND the member (current behavior).
   - If `member_count_at_bind < 4`: slash the member (demote to `Suspect` state, apply 2^slash_count epoch cool-down, but do NOT remove from the group).
4. **Re-strike:** if the slashed member misbehaves again after the cool-down, escalate:
   - 1st slash: `Suspect` + cool-down
   - 2nd slash: `Demoting` + 2× cool-down
   - 3rd slash: UNBIND (forced)
5. **Rationale:** small groups are valuable (e.g., a 3-person group is the core of a coordination cell). Preserving the group is more important than strict enforcement. Larger groups can absorb UNBIND without losing viability.

## Acceptance Criteria

- [ ] `MIN_GROUP_SIZE_FOR_UNBIND = 4` constant
- [ ] `BindEnvelope::member_count_at_bind: u16` field (signed as part of BIND)
- [ ] Slash-vs-UNBIND decision logic in `crates/octo-network/src/dc/discipline.rs`
- [ ] Re-strike escalation: 1st Suspect, 2nd Demoting, 3rd UNBIND
- [ ] Unit tests: 3-member group slash, 4-member group UNBIND, re-strike escalation
- [ ] Documentation: operator guide for slash vs UNBIND decision
- [ ] Documentation: rationale for the threshold (preserving small groups)

## Mitigates

D-DC-7 (UNBIND too aggressive on small groups)

## Deadline

Post-launch
