# Mission: 0862m — Sync Peer Slashing

## Status

Planned

## RFC

RFC-0862 v1.1.0 (Networking): Stoolap Data Sync Protocol — §Implementation Phases Phase 4; RFC-0860 (Proof-of-Relay); RFC-0855p-c (Domain Coordinator Discipline)

## Summary

Add slashing for misbehaving sync peers. When a peer sends corrupted WAL entries, fake summaries, or violates the sync protocol, the network slashes the peer's stake.

This is a Phase 4 requirement per RFC-0862 §Implementation Phases Phase 4: "Slashing for misbehaving sync peers (slash code TBD)".

## Design

### Slash reasons (new codes in RFC-0860)

| Code | Name | Trigger |
|------|------|---------|
| 0x0013 | `SyncCorruptedWalEntry` | WAL entry fails CRC32 verification |
| 0x0014 | `SyncFakeSummary` | Summary HMAC verification fails |
| 0x0015 | `SyncLsnRegression` | Peer claims LSN regression (LSN went backwards) |
| 0x0016 | `SyncRateLimitViolation` | Peer exceeds rate limit repeatedly |

### Detection points

In `SyncSessionManager::apply_wal_tail`:
- CRC32 check on each WAL entry → `SyncCorruptedWalEntry`
- LSN monotonicity check → `SyncLsnRegression`

In `SyncSessionManager::build_summary`:
- HMAC verification on received summaries → `SyncFakeSummary`

### Integration

When a slash is detected:
1. Emit a `SlashEvent` via the DomainCoordinator (RFC-0855p-c)
2. The DC aggregates slash events and applies penalties
3. Penalties: stake reduction, reputation decrease, temporary ban

## Acceptance Criteria

- [ ] Define slash codes 0x0013-0x0016 in RFC-0860
- [ ] Detect corrupted WAL entries (CRC32 failure)
- [ ] Detect fake summaries (HMAC failure)
- [ ] Detect LSN regression
- [ ] Emit `SlashEvent` to DomainCoordinator
- [ ] Unit tests for: each slash reason detection
- [ ] Integration test: peer sends bad data → peer gets slashed

## Dependencies

- **Requires:** `0862-base` (sync engine), RFC-0860 (PoRelay), RFC-0855p-c (DC discipline)
- **Required by:** none

## Complexity

Medium (~200 lines). Detection is straightforward; integration with DC discipline system adds complexity.
