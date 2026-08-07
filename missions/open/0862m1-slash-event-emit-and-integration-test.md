# Mission: SlashEvent DC Bridge + Bad-Peer Integration Test (RFC-0862 §Phase 4 follow-up)

## Status

Open (filed 2026-08-06 by mission `0862m-sync-peer-slashing.md` Band A closure). Per [[deferred-vs-unspecified]] named-owner rule, this follow-up mission owns the deferred `SyncSlash` → `SlashEnvelope` transcoding at the DC bridge (`crates/octo-network/src/dc/`) + bad-peer-slashed integration test.

**Sub-mission of:** `missions/claimed/0862m-sync-peer-slashing.md` (Band A closed 2026-08-06).

## RFC

RFC-0862 v1.1.0 (Networking): Stoolap Data Sync Protocol — §Implementation Phases Phase 4
RFC-0855p-c (Networking): Domain Coordinator Discipline — Accepted

## Summary

Bridge the canonical `SyncSlash` struct (owned by `crates/octo-sync/src/slash.rs`) to the network-layer `SlashEnvelope` (owned by `crates/octo-network/src/mon/slash.rs`) at the DC boundary. Author an integration test that drives a misbehaving sync peer through the slash emission path and asserts the resulting `SlashEnvelope` reaches the DC's slash aggregator.

The `0862m` Band A closure deferred this work because (a) the sync→network dep direction is one-way (sync engine does NOT import `octo-network`; the bridge must live in `octo-network/src/dc/`); (b) the `octo-network::mon::slash::SlashEnvelope` shape is owned by the slashing economics layer; (c) the integration test requires a running mock sync session.

## Acceptance Criteria

### SlashEvent DC bridge

- [ ] `crates/octo-network/src/dc/slash_bridge.rs` (NEW) — `pub fn encode_sync_slash(sync_slash: &octo_sync::slash::SyncSlash) -> Result<SlashEnvelope, BridgeError>` mapping the canonical `SyncSlash` to the network-layer `SlashEnvelope`. The bridge is the single canonical translation point; downstream DC code consumes `SlashEnvelope` only.
- [ ] `BridgeError` enum: `UnknownSlashCode(u16)` (sync-side reserved range not yet mapped), `MissingPeerDid` (sync-side didn't carry the peer DID).
- [ ] Manual redacting Debug on `BridgeError` (RFC-0957-A1 §Security defense-in-depth).
- [ ] `SlashEvent` constructed at the bridge from the `SlashEnvelope` + the `DomainCoordinator` recipient DID; emitted via `dc.emit_slash(event)`.

### Integration test

- [ ] `crates/octo-network/tests/dc_slash_bridge.rs` (NEW) — bootstraps a mock sync session with a misbehaving peer (forced `SyncError::FakeSummary`); asserts the bridge produces a `SlashEnvelope` with `slash_code = 0x0021 (SyncFakeSummary)`, `peer_did` matches the mock peer, `reason` matches the fake-summary mismatch.
- [ ] Asserts: bad-data peer gets slashed end-to-end (sync engine detects → bridge transcodes → DC emits).

### Cross-crate compat

- [ ] `cargo build -p octo-network -p octo-sync` green
- [ ] `cargo test -p octo-network --test dc_slash_bridge` green (1/1 new test)
- [ ] `cargo test -p octo-sync --lib slash` green (9 pre-existing tests still pass)
- [ ] `cargo clippy --workspace --all-targets --features full -- -D warnings` clean (per [[feedback_clippy_zero_warnings]])
- [ ] `cargo fmt --check --workspace` clean

## Dependencies

**Requires (RFC gates):**
- RFC-0862 — sync engine substrate (CRC32/LSN/HMAC detection)
- RFC-0855p-c — DomainCoordinator discipline (slash emission recipient)

**Requires (mission gates):**
- `missions/claimed/0862m-sync-peer-slashing.md` (Band A closed 2026-08-06) — provides `SyncSlash` struct + `SyncError::FakeSummary` detection path consumed here

```yaml
depends_on:
  - 0862m-sync-peer-slashing # SyncSlash struct + SyncError variants consumed by bridge
  - 0855p-c # DomainCoordinator slash emission surface
```

## Location

- `crates/octo-network/src/dc/slash_bridge.rs` (NEW) — `encode_sync_slash` + `BridgeError`
- `crates/octo-network/src/dc/mod.rs` (MODIFY) — export the bridge module
- `crates/octo-network/tests/dc_slash_bridge.rs` (NEW) — bad-peer-slashed integration test

## Claimant

TBD (claim 2026-08-06+)

## Notes

- The one-way sync→network dep direction is the load-bearing constraint: `octo-sync` MUST NOT import `octo-network`. The bridge lives in `octo-network/src/dc/` because `octo-network` already depends on `octo-sync` for sync-engine integration. Cross-crate direction is verified by `git grep "use octo_sync" crates/octo-sync/src/` returning empty.
- The integration test uses a mock sync session (not a real WAL stream) per [[stoolap-general-purpose-db]] red line: cipherocto consumer schema stays cipherocto-side; the test does not require the stoolap fork.
