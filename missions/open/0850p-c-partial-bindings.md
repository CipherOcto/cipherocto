# Mission: 0850p-c — Partial bindings

## Status

Open (2026-06-16) — future

## RFC

RFC-0850p-c (Networking): Transport Group Binding — §"Future Work"

## Summary

Allow a BIND envelope to specify a subset of the physical group that participates in the mission. Currently a BIND binds the entire physical group; for large public groups (e.g., a 1000-member WhatsApp community), only a handful of members may be DOT participants. The non-participating members still receive all DOT messages (waste of bandwidth) and the participant list is implicit (must be inferred from envelope signatures).

## Design

Add an optional field to the BIND envelope:

```rust
pub struct BindEnvelope {
    // existing fields...
    pub participant_filter: Option<Vec<PeerId>>,
}
```

When `participant_filter` is `Some(list)`, the adapter filters DOT messages: only those with a `peer_id` in `list` are accepted; others are dropped silently (they're not DOT participants, just other members of the physical group).

When `participant_filter` is `None`, behavior is unchanged (all members participate).

The `participant_filter` is signed as part of the BIND envelope (binding the filter to the DomainCoordinator's authority).

## Acceptance Criteria

- [ ] `BindEnvelope::participant_filter: Option<Vec<PeerId>>` field
- [ ] Adapter filters DOT messages per `participant_filter`
- [ ] Filter is part of the signed payload (signature covers it)
- [ ] Unit test: 1000-member group with `participant_filter = [A, B, C]` only delivers DOT messages to/from A, B, C
- [ ] Documentation: when to use partial bindings (large public groups; small private groups should use full binding)
- [ ] Backward compatibility: envelopes without `participant_filter` work unchanged

## Dependencies

Depends on RFC-0850p-c status: Accepted. No prerequisite missions; this is a BIND envelope extension.

## Claimant

(none — Open mission)

## Pull Request

(none — Open mission)

## Location

`crates/octo-network/src/mon/bind_envelope.rs` (add field); all platform adapters (add filter check in message ingress).

## Complexity

Low (~150 lines; field addition, filter in 13 adapters).

## Prerequisites

- RFC-0850p-c status: Accepted

## Notes

### Why optional?

The filter is opt-in; existing BINDs without the filter behave as before (all members participate). Backward compatibility is preserved.

### Why signed?

The filter is part of the signed payload. A malicious DC cannot add or remove members from the filter without invalidating the BIND signature.

### Type Coverage

| RFC-0850p-c Type | Implemented By |
|-----------------|----------------|
| `BindEnvelope::participant_filter: Option<Vec<PeerId>>` | This mission |
| Adapter-side filtering of DOT messages | This mission |

### Implementation Guide

Reference: `crates/octo-network/src/mon/bind_envelope.rs` (existing `BindEnvelope` struct); adapter message routing code.

## Mitigates

Bandwidth optimization for large public groups; not a security issue (the filter is authority-bound).

## Deadline

Future
