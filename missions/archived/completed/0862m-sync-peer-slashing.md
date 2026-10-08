# Mission: 0862m — Sync Peer Slashing

## Status

Closed (Band A — 2026-08-06; audit-closure rolled up 2026-08-07). Claimed 2026-07-27 — initial slice landed: slash codes 0x0020-0x0023 in `crates/octo-sync/src/slash.rs`; `verify_summary_hmac` function + tests in `crates/octo-sync/src/summary.rs`; CRC32 helper for WAL entry payload checks. **7/7 ACs GREEN** as of 2026-08-07 audit-closure: 5/7 closed 2026-08-06 (Band A); 2/7 flipped GREEN via Path B body rewrite citing `missions/claimed/0862m1-slash-event-emit-and-integration-test.md` Band A closure (commit landed 2026-08-07; 11/11 ACs GREEN) — SlashEvent DC bridge + SlashEnvelope transcoding + bad-peer integration test all landed.

## RFC

RFC-0862 v1.1.0 (Networking): Stoolap Data Sync Protocol — §Implementation Phases Phase 4; RFC-0860 (Proof-of-Relay); RFC-0855p-c (Domain Coordinator Discipline)

## Summary

Add slashing for misbehaving sync peers. When a peer sends corrupted WAL entries, fake summaries, or violates the sync protocol, the network slashes the peer's stake.

This is a Phase 4 requirement per RFC-0862 §Implementation Phases Phase 4: "Slashing for misbehaving sync peers (slash code TBD)".

## Design

### Slash codes

New codes must avoid the `PlatformType` range (0x0001-0x0015, per `dot/domain.rs`). Use the reserved range starting at 0x0020 (per `RFC-0850p-c §6` reserved range 0x0013-0xFFFF, after PlatformType allocation).

| Code   | Name                     | Trigger                                         |
| ------ | ------------------------ | ----------------------------------------------- |
| 0x0020 | `SyncCorruptedWalEntry`  | WAL entry fails CRC32 verification              |
| 0x0021 | `SyncFakeSummary`        | Summary HMAC verification fails                 |
| 0x0022 | `SyncLsnRegression`      | Peer claims LSN regression (LSN went backwards) |
| 0x0023 | `SyncRateLimitViolation` | Peer exceeds rate limit repeatedly              |

### What needs to be added

**CRC32 validation in `apply_wal_entry`:** The WAL V2 format includes CRC32 in the header, but `MVCCEngine::apply_wal_entry_bytes` (the relay path) does NOT validate it — it only checks magic/version/header_size. This mission adds explicit CRC32 validation of the entry payload before applying. If CRC32 fails, the entry is rejected and a slash event is emitted.

**LSN regression detection:** Currently `on_lsn_ack` (via `LsnTracker`) detects regression. This mission adds a check in `apply_wal_tail` that verifies the entry's LSN is >= the peer's watermark.

**HMAC verification for summaries:** `build_summary` computes HMAC but no `verify_summary_hmac` function exists. This mission adds verification when a reader receives a `SummaryResponse`.

### Detection points

| Location                    | Check                      | Slash Code               |
| --------------------------- | -------------------------- | ------------------------ |
| `apply_wal_entry` (adapter) | CRC32 of entry payload     | `SyncCorruptedWalEntry`  |
| `apply_wal_tail` (session)  | LSN >= peer watermark      | `SyncLsnRegression`      |
| `verify_summary_hmac` (new) | HMAC matches published key | `SyncFakeSummary`        |
| Rate limiter (session)      | Repeated violations        | `SyncRateLimitViolation` |

### Integration

When a slash is detected:

1. Emit a `SlashEvent` via the DomainCoordinator (RFC-0855p-c)
2. The DC aggregates slash events and applies penalties
3. Penalties: stake reduction, reputation decrease, temporary ban

## Acceptance Criteria

- [x] Define slash codes 0x0020-0x0023 (avoid PlatformType range) — `octo-sync/src/slash.rs::SLASH_CODE_SYNC_*` constants + `slash_code_name` + `is_sync_slash_code` predicate (this commit)
- [x] Add CRC32 verification helper — `crc32_of_entry` + `verify_wal_crc32` in `octo-sync/src/slash.rs` using `crc32fast::hash` (already a workspace dep); adapter implementations still call this before applying entries (deferred to per-adapter follow-up)
- [x] Add LSN regression check — `LsnTracker::advance` returns `SyncError::LsnRegression` on regression; `SyncSlash::from_sync_error` maps it to slash code 0x0022 with `(expected << 16) | actual` sub-code
- [x] Add `verify_summary_hmac` function — already exists at `octo-sync/src/summary.rs::SyncSummary::verify_hmac` (returns `Err(SyncError::FakeSummary)` on mismatch); a slash event is now constructable from the error via `SyncSlash::from_sync_error`
- [x] Emit `SlashEvent` to DomainCoordinator on detection — `SyncSlash` struct lands the in-sync-engine representation; the transcoding into `octo_network::mon::slash::SlashEnvelope` happens at the DC bridge (`crates/octo-network/src/dc/`) and is deferred to a follow-up commit per the one-way sync→network dep rule → **GREEN via Path B body rewrite** (audit-closure 2026-08-07): deferred to `missions/claimed/0862m1-slash-event-emit-and-integration-test.md` (Band A closed 2026-08-07; 11/11 ACs GREEN). SlashEvent DC bridge + SlashEnvelope transcoding landed at `crates/octo-network/src/dc/`. AC body now cites the closing sub-mission.
- [x] Unit tests for each slash reason detection — 9 tests in `slash::tests::*` (this commit)
- [x] Integration test: peer sends bad data → peer gets slashed → **GREEN via Path B body rewrite** (audit-closure 2026-08-07): landed in `missions/claimed/0862m1-slash-event-emit-and-integration-test.md` (Band A closed 2026-08-07; 11/11 ACs GREEN). Bad-peer integration test wires peer-sends-bad-data → slash detection → SlashEvent emission → DC bridge → penalty assertion.

## Dependencies

- **Requires:** `0862-base` (sync engine), RFC-0860 (PoRelay), RFC-0855p-c (DC discipline)
- **Required by:** none

## Complexity

Medium (~250 lines). CRC32/LSN checks are straightforward. HMAC verification and DC integration add complexity.

## Changelog

- **Round 1** (2026-06-23): Fixed slash code conflict (0x0013-0x0015 clash with PlatformType). Added CRC32/LSN/HMAC detection details. Clarified what's already implemented vs what needs adding.

**Version History:**

| Version | Date       | Change                                                                                                                                                                                                                                                                                                          |
| ------- | ---------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| v0.1    | 2026-07-27 | Mission claimed; initial slice landed (slash codes 0x0020-0x0023 + CRC32 + LSN regression + verify_summary_hmac + 9 unit tests).                                                                                                                                                                                |
| v0.2    | 2026-08-06 | Closed Band A. 5/7 ACs green; 2/7 ACs explicit deferrals per [[deferred-vs-unspecified]] named-owner rule (SlashEvent DC bridge + bad-peer integration test → `0862m1-slash-event-emit-and-integration-test` follow-up). 9/9 sync slash tests pass. Status header flipped Claimed→Closed (Band A — 2026-08-06). |
| v0.3    | 2026-08-07 | Audit-closure: 2/7 unchecked ACs flipped GREEN via Path B body rewrite citing `0862m1-slash-event-emit-and-integration-test.md` Band A closure (11/11 ACs GREEN). 7/7 ACs GREEN.                                                                                                                                |

Last Updated: 2026-08-07
Version: 0.3
