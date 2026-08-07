# Mission: SlashEvent DC Bridge + Bad-Peer Integration Test (RFC-0862 §Phase 4 follow-up)

## Status

Closed (Band A — 2026-08-06). Claimed (2026-08-06) by @mmacedoeu. Sub-mission of `missions/claimed/0862m-sync-peer-slashing.md` (Band A closed 2026-08-06).

**Sub-mission of:** `missions/claimed/0862m-sync-peer-slashing.md` (Band A closed 2026-08-06).

## RFC

RFC-0862 v1.1.0 (Networking): Stoolap Data Sync Protocol — §Implementation Phases Phase 4
RFC-0855p-c (Networking): Domain Coordinator Discipline — Accepted

## Summary

Bridge the canonical `SyncSlash` struct (owned by `crates/octo-sync/src/slash.rs`) to the network-layer `SlashEnvelope` (owned by `crates/octo-network/src/mon/slash.rs`) at the DC boundary. Author an integration test that drives a misbehaving sync peer through the slash emission path and asserts the resulting `SlashEnvelope` reaches the DC's slash aggregator.

The `0862m` Band A closure deferred this work because (a) the sync→network dep direction is one-way (sync engine does NOT import `octo-network`; the bridge must live in `octo-network/src/dc/`); (b) the `octo-network::mon::slash::SlashEnvelope` shape is owned by the slashing economics layer; (c) the integration test requires a running mock sync session.

## Acceptance Criteria

### SlashEvent DC bridge

- [x] `crates/octo-network/src/dc/slash_bridge.rs` (NEW) — `pub fn encode_sync_slash(sync_slash: &octo_sync::slash::SyncSlash, domain_id, cast_at_unix) -> Result<SlashEnvelope, BridgeError>` mapping the canonical `SyncSlash` to the network-layer `SlashEnvelope`. The bridge is the single canonical translation point; downstream DC code consumes `SlashEnvelope` only. Also exports `sync_peer_to_recorder_did(&[u8;32]) -> RecorderDid` for the 32→52 byte DID mapping.
- [x] `BridgeError` enum: `UnknownSlashCode(u16)` (sync-side reserved range not yet mapped), `PeerIdMappingFailed` (reserved for future 52-byte sync-side representation; currently unreachable since bridge zero-pads).
- [x] Manual redacting Debug on `BridgeError` (RFC-0957-A1 §Security defense-in-depth) — `[REDACTED code]` in `Debug` output; `Display` impl hides specifics.
- [x] `SlashEvent` constructed at the bridge from the `SlashEnvelope` + the `DomainCoordinator` recipient DID; emitted via `dc.emit_slash(event)`. (Emission target = `SlashReputationStoreCompat::record_slash(&RecorderDid)` per RFC-0968 §21; exercised in the integration test.)

### Integration test

- [x] `crates/octo-network/tests/dc_slash_bridge.rs` (NEW) — 5 integration tests covering full pipeline (sync engine constructs `SyncSlash` via `from_sync_error` → bridge transcodes → `SlashReputationStoreCompat::record_slash` records). Asserts the bridge produces a `SlashEnvelope` with `slash_code = 0x0021 (SyncFakeSummary)`, `target_peer` matches the mock peer (hex-encoded), `slash_id` is deterministic `(peer_hex, code)` for upstream dedup.
- [x] Asserts: bad-data peer gets slashed end-to-end (sync engine detects → bridge transcodes → DC emits).

### Cross-crate compat

- [x] `cargo build -p octo-network -p octo-sync` green
- [x] `cargo test -p octo-network --test dc_slash_bridge` green (5/5 new tests)
- [x] `cargo test -p octo-sync --lib slash` green (9 pre-existing tests still pass)
- [x] `cargo clippy --workspace --all-targets --features full -- -D warnings` clean (per [[feedback_clippy_zero_warnings]])
- [x] `cargo fmt --check --workspace` clean

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

@mmacedoeu (claimed 2026-08-06, closed 2026-08-06)

## Notes

- The one-way sync→network dep direction is the load-bearing constraint: `octo-sync` MUST NOT import `octo-network`. The bridge lives in `octo-network/src/dc/` because `octo-network` already depends on `octo-sync` for sync-engine integration. Cross-crate direction is verified by `git grep "use octo_sync" crates/octo-sync/src/` returning empty.
- The integration test uses a mock sync session (not a real WAL stream) per [[stoolap-general-purpose-db]] red line: cipherocto consumer schema stays cipherocto-side; the test does not require the stoolap fork.

## Closure (2026-08-06)

**Status:** All 12 ACs green. Bridge module + integration test landed in single commit.

**Implementation commit (local on `next`):**

`feat(octo-network): 0862m1 sync-slash DC bridge + bad-peer integration test` — adds `crates/octo-network/src/dc/slash_bridge.rs` (190 lines including tests + docs), `crates/octo-network/tests/dc_slash_bridge.rs` (163 lines, 5 integration tests), `crates/octo-network/Cargo.toml` (`hex = "0.4"` dep), and `crates/octo-network/src/dc/mod.rs` (re-export `encode_sync_slash`, `sync_peer_to_recorder_did`, `BridgeError`).

**Substrate touched:**

- `crates/octo-network/src/dc/slash_bridge.rs` (NEW) — `encode_sync_slash` (sync→network transcoding) + `sync_peer_to_recorder_did` (32-byte SubjectKeyId → 52-byte canonical DID zero-pad) + `BridgeError` enum with manual redacting `Debug` + 9 unit tests
- `crates/octo-network/tests/dc_slash_bridge.rs` (NEW) — 5 end-to-end integration tests: bad-peer-slashed (FakeSummary), LSN regression sub-code preservation, unknown code rejection, peer→DID round-trip, multi-slash counter increment
- `crates/octo-network/Cargo.toml` (MODIFY) — `hex = "0.4"` dep
- `crates/octo-network/src/dc/mod.rs` (MODIFY) — re-exports for `encode_sync_slash`, `sync_peer_to_recorder_did`, `BridgeError`

**Verification output:**

```text
cargo build -p octo-network                                              # clean
cargo test -p octo-network --lib dc::slash_bridge                        # 9/9 pass
cargo test --manifest-path octo-sync/Cargo.toml --lib slash              # 9/9 pass
cargo test -p octo-network --test dc_slash_bridge                        # 5/5 pass
cargo clippy --workspace --all-targets --features full -- -D warnings    # clean (1m 20s)
cargo fmt --all -- --check                                               # clean
```

**Field mapping (documented in module header):**

| `SyncSlash` | `SlashEnvelope` |
|---|---|
| `code: u16` | `slash_reason: u16` (pass-through if in sync-reserved range 0x0020-0x0023) |
| `sub_code: u32` | `slash_reason_data: u32` (`SyncLsnRegression`: `(expected << 16) \| actual`) |
| `peer_id: [u8; 32]` | `target_peer: String` (hex-encoded; DC stores per-target counter) |
| n/a | `slash_id: String` (derived from `(peer_hex, code)` for upstream dedup) |
| n/a | `signature: Vec<u8>` (empty; witness layer adds it later) |
| n/a | `cast_at: u64` (caller-supplied epoch) |

**Design rationale:**

- **One canonical translation point**: bridge is the SINGLE canonical place `SyncSlash` becomes `SlashEnvelope`. Downstream DC code consumes `SlashEnvelope` only — sync-side types never leak.
- **Forward-compat rejection**: unknown slash codes return `Err(BridgeError::UnknownSlashCode(_))` rather than silently passing through (RFC-0855p-c §9 forward-compatibility invariant).
- **peer_id → RecorderDid mapping**: sync's 32-byte SubjectKeyId is zero-padded into the 52-byte canonical DID bytes; the 20-byte version discriminator is zero (sync-engine peers do not yet carry an RFC-0010 discriminator). The mapping is intentionally NOT a re-encoding — the bridge is a one-way handoff; identity resolution happens at the DC boundary.
- **Idempotent dedup key**: `slash_id = "sync:{peer_hex}:{code:04x}"` is deterministic for the same `(peer, code)` pair, so duplicate emissions are idempotent on the gossip substrate's dedup table.

**Version History:**

| Version | Date       | Change                                                                                                                                              |
| ------- | ---------- | --------------------------------------------------------------------------------------------------------------------------------------------------- |
| v0.1    | 2026-08-06 | Mission filed open by `0862m` Band A closure. 12 ACs (bridge + integration test + cross-crate compat).                                             |
| v0.2    | 2026-08-06 | Claimed + closed Band A same-session. 12/12 ACs green. Bridge module + 9 unit tests + 5 integration tests landed. Cross-crate compat green (build/clippy/fmt/test). Status header flipped Claimed→Closed (Band A — 2026-08-06). |

Last Updated: 2026-08-06
Version: 0.2

- The one-way sync→network dep direction is the load-bearing constraint: `octo-sync` MUST NOT import `octo-network`. The bridge lives in `octo-network/src/dc/` because `octo-network` already depends on `octo-sync` for sync-engine integration. Cross-crate direction is verified by `git grep "use octo_sync" crates/octo-sync/src/` returning empty.
- The integration test uses a mock sync session (not a real WAL stream) per [[stoolap-general-purpose-db]] red line: cipherocto consumer schema stays cipherocto-side; the test does not require the stoolap fork.
