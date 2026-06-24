# Mission: 0862m — Sync Peer Slashing

## Status

Planned

## RFC

RFC-0862 v1.1.0 (Networking): Stoolap Data Sync Protocol — §Implementation Phases Phase 4; RFC-0860 (Proof-of-Relay); RFC-0855p-c (Domain Coordinator Discipline)

## Summary

Add slashing for misbehaving sync peers. When a peer sends corrupted WAL entries, fake summaries, or violates the sync protocol, the network slashes the peer's stake.

This is a Phase 4 requirement per RFC-0862 §Implementation Phases Phase 4: "Slashing for misbehaving sync peers (slash code TBD)".

## Design

### Slash codes

New codes must avoid the `PlatformType` range (0x0001-0x0015, per `dot/domain.rs`). Use the reserved range starting at 0x0020 (per `RFC-0850p-c §6` reserved range 0x0013-0xFFFF, after PlatformType allocation).

| Code | Name | Trigger |
|------|------|---------|
| 0x0020 | `SyncCorruptedWalEntry` | WAL entry fails CRC32 verification |
| 0x0021 | `SyncFakeSummary` | Summary HMAC verification fails |
| 0x0022 | `SyncLsnRegression` | Peer claims LSN regression (LSN went backwards) |
| 0x0023 | `SyncRateLimitViolation` | Peer exceeds rate limit repeatedly |

### What needs to be added

**CRC32 validation in `apply_wal_entry`:** The WAL V2 format includes CRC32 in the header, but `MVCCEngine::apply_wal_entry_bytes` (the relay path) does NOT validate it — it only checks magic/version/header_size. This mission adds explicit CRC32 validation of the entry payload before applying. If CRC32 fails, the entry is rejected and a slash event is emitted.

**LSN regression detection:** Currently `on_lsn_ack` (via `LsnTracker`) detects regression. This mission adds a check in `apply_wal_tail` that verifies the entry's LSN is >= the peer's watermark.

**HMAC verification for summaries:** `build_summary` computes HMAC but no `verify_summary_hmac` function exists. This mission adds verification when a reader receives a `SummaryResponse`.

### Detection points

| Location | Check | Slash Code |
|----------|-------|------------|
| `apply_wal_entry` (adapter) | CRC32 of entry payload | `SyncCorruptedWalEntry` |
| `apply_wal_tail` (session) | LSN >= peer watermark | `SyncLsnRegression` |
| `verify_summary_hmac` (new) | HMAC matches published key | `SyncFakeSummary` |
| Rate limiter (session) | Repeated violations | `SyncRateLimitViolation` |

### Integration

When a slash is detected:
1. Emit a `SlashEvent` via the DomainCoordinator (RFC-0855p-c)
2. The DC aggregates slash events and applies penalties
3. Penalties: stake reduction, reputation decrease, temporary ban

## Acceptance Criteria

- [ ] Define slash codes 0x0020-0x0023 (avoid PlatformType range)
- [ ] Add CRC32 verification in `apply_wal_entry` path
- [ ] Add LSN regression check in `apply_wal_tail`
- [ ] Add `verify_summary_hmac` function
- [ ] Emit `SlashEvent` to DomainCoordinator on detection
- [ ] Unit tests for: each slash reason detection
- [ ] Integration test: peer sends bad data → peer gets slashed

## Dependencies

- **Requires:** `0862-base` (sync engine), RFC-0860 (PoRelay), RFC-0855p-c (DC discipline)
- **Required by:** none

## Complexity

Medium (~250 lines). CRC32/LSN checks are straightforward. HMAC verification and DC integration add complexity.

## Changelog

- **Round 1** (2026-06-23): Fixed slash code conflict (0x0013-0x0015 clash with PlatformType). Added CRC32/LSN/HMAC detection details. Clarified what's already implemented vs what needs adding.
